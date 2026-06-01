use crate::error::{AppError, Result};
use crate::model::{
    Comment, IssueDetail, IssueSummary, PrDetail, PrFile, PrInfo, PrSummary, Review,
};
use crate::proc;
use std::collections::HashMap;
use std::path::Path;

/// Derive an overall CI state from gh's `statusCheckRollup` array.
fn rollup_state(v: Option<&serde_json::Value>) -> Option<String> {
    let arr = v?.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut pending = false;
    for c in arr {
        if let Some(state) = c.get("state").and_then(|x| x.as_str()) {
            match state {
                "FAILURE" | "ERROR" => return Some("FAILURE".to_string()),
                "PENDING" | "EXPECTED" => pending = true,
                _ => {}
            }
        }
        if let Some(status) = c.get("status").and_then(|x| x.as_str()) {
            if status != "COMPLETED" {
                pending = true;
            }
        }
        if let Some(concl) = c.get("conclusion").and_then(|x| x.as_str()) {
            match concl {
                "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => {
                    return Some("FAILURE".to_string())
                }
                _ => {}
            }
        }
    }
    Some(if pending {
        "PENDING".to_string()
    } else {
        "SUCCESS".to_string()
    })
}

/// Fetch open PRs for the repo (inferred from its remote), keyed by head branch.
/// Returns None when gh is unavailable / unauthenticated / the repo has no GitHub remote.
pub fn list_prs(repo: &Path) -> Option<HashMap<String, PrInfo>> {
    let r = proc::run(
        "gh",
        [
            "pr",
            "list",
            "--state",
            "open",
            "-L",
            "200",
            "--json",
            "number,headRefName,baseRefName,state,url,reviewDecision,statusCheckRollup",
        ],
        Some(repo),
    )
    .ok()?;
    if !r.success {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&r.stdout).ok()?;
    let arr = value.as_array()?;
    let mut map = HashMap::new();
    for pr in arr {
        let head = match pr.get("headRefName").and_then(|v| v.as_str()) {
            Some(h) => h.to_string(),
            None => continue,
        };
        map.insert(
            head,
            PrInfo {
                number: pr.get("number").and_then(|v| v.as_u64()).unwrap_or(0),
                url: pr.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                state: pr.get("state").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                base_ref: pr
                    .get("baseRefName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                review_decision: pr
                    .get("reviewDecision")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                checks: rollup_state(pr.get("statusCheckRollup")),
            },
        );
    }
    Some(map)
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubmitAction {
    Create,
    UpdateBase,
    UpToDate,
}

#[derive(Debug, Clone)]
pub struct SubmitStep {
    pub branch: String,
    pub base: String,
    pub action: SubmitAction,
    pub pr: Option<u64>,
}

/// Pure planner: given a bottom-up branch `order`, each branch's parent, the trunk,
/// and the existing PRs (by head), decide what to do for each branch.
///
/// Each PR's base is its parent branch (the trunk for the bottom of the stack).
/// Pure and side-effect free so it can be unit-tested without touching the network.
pub fn plan(
    order: &[String],
    parent_of: &HashMap<String, String>,
    trunk: &str,
    existing: &HashMap<String, PrInfo>,
) -> Vec<SubmitStep> {
    order
        .iter()
        .map(|b| {
            let base = parent_of.get(b).cloned().unwrap_or_else(|| trunk.to_string());
            match existing.get(b) {
                Some(pr) if pr.base_ref == base => SubmitStep {
                    branch: b.clone(),
                    base,
                    action: SubmitAction::UpToDate,
                    pr: Some(pr.number),
                },
                Some(pr) => SubmitStep {
                    branch: b.clone(),
                    base,
                    action: SubmitAction::UpdateBase,
                    pr: Some(pr.number),
                },
                None => SubmitStep {
                    branch: b.clone(),
                    base,
                    action: SubmitAction::Create,
                    pr: None,
                },
            }
        })
        .collect()
}

pub struct PrCreated {
    pub number: u64,
    #[allow(dead_code)] // returned for callers that want the URL
    pub url: String,
}

/// Look up a PR by head branch, returning its number and URL.
pub fn pr_view(repo: &Path, head: &str) -> Result<PrCreated> {
    let r = proc::run(
        "gh",
        ["pr", "view", head, "--json", "number,url"],
        Some(repo),
    )?;
    if !r.success {
        return Err(AppError::new(format!("gh pr view failed: {}", r.stderr.trim())));
    }
    let v: serde_json::Value =
        serde_json::from_str(&r.stdout).map_err(|e| AppError::new(e.to_string()))?;
    Ok(PrCreated {
        number: v.get("number").and_then(|x| x.as_u64()).unwrap_or(0),
        url: v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    })
}

/// Create a PR for `head` based on `base` (optionally as a draft).
pub fn create_pr(
    repo: &Path,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> Result<PrCreated> {
    let mut args: Vec<&str> = vec![
        "pr", "create", "--head", head, "--base", base, "--title", title, "--body", body,
    ];
    if draft {
        args.push("--draft");
    }
    let r = proc::run("gh", args, Some(repo))?;
    if !r.success {
        return Err(AppError::new(format!("gh pr create failed: {}", r.stderr.trim())));
    }
    pr_view(repo, head)
}

/// Change an existing PR's base branch.
pub fn set_pr_base(repo: &Path, number: u64, base: &str) -> Result<()> {
    let number = number.to_string();
    let r = proc::run(
        "gh",
        ["pr", "edit", number.as_str(), "--base", base],
        Some(repo),
    )?;
    if !r.success {
        return Err(AppError::new(format!("gh pr edit failed: {}", r.stderr.trim())));
    }
    Ok(())
}

/// Head branch names of PRs that have been merged (to clean up landed branches).
pub fn merged_prs(repo: &Path) -> Vec<String> {
    let r = match proc::run(
        "gh",
        ["pr", "list", "--state", "merged", "-L", "100", "--json", "headRefName"],
        Some(repo),
    ) {
        Ok(r) if r.success => r,
        _ => return Vec::new(),
    };
    let value: serde_json::Value = match serde_json::from_str(&r.stdout) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    p.get("headRefName")
                        .and_then(|h| h.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

const DIFF_CAP: usize = 200_000;

/// Full detail for a single PR: metadata + changed files + unified diff.
pub fn pr_detail(repo: &Path, number: u64) -> Result<PrDetail> {
    let n = number.to_string();
    let r = proc::run(
        "gh",
        [
            "pr", "view", n.as_str(), "--json",
            "number,title,body,state,author,baseRefName,headRefName,url,additions,deletions,reviewDecision,statusCheckRollup,files,commits,comments,reviews",
        ],
        Some(repo),
    )?;
    if !r.success {
        return Err(AppError::new(format!("gh pr view failed: {}", r.stderr.trim())));
    }
    let v: serde_json::Value =
        serde_json::from_str(&r.stdout).map_err(|e| AppError::new(e.to_string()))?;

    let files = v
        .get("files")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    Some(PrFile {
                        path: f.get("path")?.as_str()?.to_string(),
                        additions: f.get("additions").and_then(|x| x.as_u64()).unwrap_or(0),
                        deletions: f.get("deletions").and_then(|x| x.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let commits = v
        .get("commits")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    c.get("messageHeadline")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    // Unified diff (separate call; may be large -> cap it on a char boundary).
    let mut diff = proc::run("gh", ["pr", "diff", n.as_str()], Some(repo))
        .ok()
        .filter(|r| r.success)
        .map(|r| r.stdout)
        .unwrap_or_default();
    if diff.len() > DIFF_CAP {
        let mut end = DIFF_CAP;
        while end > 0 && !diff.is_char_boundary(end) {
            end -= 1;
        }
        diff.truncate(end);
        diff.push_str("\n… (diff truncated)");
    }

    Ok(PrDetail {
        number,
        title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        body: v.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        state: v.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        author: v
            .get("author")
            .and_then(|a| a.get("login"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        base_ref: v.get("baseRefName").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        head_ref: v.get("headRefName").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        url: v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        additions: v.get("additions").and_then(|x| x.as_u64()).unwrap_or(0),
        deletions: v.get("deletions").and_then(|x| x.as_u64()).unwrap_or(0),
        review_decision: v
            .get("reviewDecision")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        checks: rollup_state(v.get("statusCheckRollup")),
        files,
        commits,
        diff,
        comments: parse_comments(v.get("comments")),
        reviews: parse_reviews(v.get("reviews")),
    })
}

/// Author login from a `{ "author": { "login": ... } }` or `{ "login": ... }` node.
fn author_of(node: &serde_json::Value) -> String {
    node.get("author")
        .and_then(|a| a.get("login"))
        .or_else(|| node.get("login"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse a gh `comments` array into [`Comment`]s (oldest first, as gh returns them).
fn parse_comments(v: Option<&serde_json::Value>) -> Vec<Comment> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| Comment {
                    author: author_of(c),
                    body: c.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    created_at: c
                        .get("createdAt")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a gh `reviews` array, keeping only reviews that carry a verdict or a body.
fn parse_reviews(v: Option<&serde_json::Value>) -> Vec<Review> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| Review {
                    author: author_of(c),
                    state: c.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    body: c.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    created_at: c
                        .get("submittedAt")
                        .or_else(|| c.get("createdAt"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .filter(|r| !r.body.trim().is_empty() || r.state == "APPROVED" || r.state == "CHANGES_REQUESTED")
                .collect()
        })
        .unwrap_or_default()
}

/// Labels array (`[{ "name": ... }]`) -> names.
fn parse_labels(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.get("name").and_then(|x| x.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// List issues (open by default), newest-updated first. None when gh/remote unavailable.
pub fn list_issues(repo: &Path, state: &str) -> Option<Vec<IssueSummary>> {
    let r = proc::run(
        "gh",
        [
            "issue", "list", "--state", state, "-L", "100", "--json",
            "number,title,state,author,url,labels,comments,updatedAt",
        ],
        Some(repo),
    )
    .ok()?;
    if !r.success {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&r.stdout).ok()?;
    let arr = v.as_array()?;
    Some(
        arr.iter()
            .map(|i| IssueSummary {
                number: i.get("number").and_then(|x| x.as_u64()).unwrap_or(0),
                title: i.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                state: i.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                author: author_of(i),
                url: i.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                labels: parse_labels(i.get("labels")),
                comment_count: i
                    .get("comments")
                    .and_then(|x| x.as_array())
                    .map(|a| a.len() as u64)
                    .unwrap_or(0),
                updated_at: i.get("updatedAt").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            })
            .collect(),
    )
}

/// Full detail for a single issue, including its comments.
pub fn issue_detail(repo: &Path, number: u64) -> Result<IssueDetail> {
    let n = number.to_string();
    let r = proc::run(
        "gh",
        [
            "issue", "view", n.as_str(), "--json",
            "number,title,body,state,author,url,labels,comments",
        ],
        Some(repo),
    )?;
    if !r.success {
        return Err(AppError::new(format!("gh issue view failed: {}", r.stderr.trim())));
    }
    let v: serde_json::Value =
        serde_json::from_str(&r.stdout).map_err(|e| AppError::new(e.to_string()))?;
    Ok(IssueDetail {
        number,
        title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        body: v.get("body").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        state: v.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        author: author_of(&v),
        url: v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        labels: parse_labels(v.get("labels")),
        comments: parse_comments(v.get("comments")),
    })
}

/// List pull requests for the list view (`state` = "open" | "closed" | "merged" | "all").
/// None when gh is unavailable / unauthenticated / no GitHub remote.
pub fn list_pull_requests(repo: &Path, state: &str) -> Option<Vec<PrSummary>> {
    let r = proc::run(
        "gh",
        [
            "pr", "list", "--state", state, "-L", "100", "--json",
            "number,title,state,author,headRefName,baseRefName,url,isDraft,reviewDecision,statusCheckRollup,updatedAt",
        ],
        Some(repo),
    )
    .ok()?;
    if !r.success {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&r.stdout).ok()?;
    let arr = v.as_array()?;
    Some(
        arr.iter()
            .map(|p| PrSummary {
                number: p.get("number").and_then(|x| x.as_u64()).unwrap_or(0),
                title: p.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                state: p.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                author: author_of(p),
                head_ref: p.get("headRefName").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                base_ref: p.get("baseRefName").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                url: p.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                is_draft: p.get("isDraft").and_then(|x| x.as_bool()).unwrap_or(false),
                review_decision: p
                    .get("reviewDecision")
                    .and_then(|x| x.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                checks: rollup_state(p.get("statusCheckRollup")),
                updated_at: p.get("updatedAt").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(number: u64, base: &str) -> PrInfo {
        PrInfo {
            number,
            url: format!("https://github.test/pull/{}", number),
            state: "OPEN".into(),
            base_ref: base.into(),
            review_decision: None,
            checks: None,
        }
    }

    #[test]
    fn plan_creates_bottom_up_with_parent_bases() {
        let order = vec!["a".to_string(), "b".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("a".to_string(), "main".to_string());
        parent_of.insert("b".to_string(), "a".to_string());
        let existing = HashMap::new();

        let steps = plan(&order, &parent_of, "main", &existing);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].branch, "a");
        assert_eq!(steps[0].base, "main");
        assert_eq!(steps[0].action, SubmitAction::Create);
        assert_eq!(steps[1].branch, "b");
        assert_eq!(steps[1].base, "a");
        assert_eq!(steps[1].action, SubmitAction::Create);
    }

    #[test]
    fn plan_updates_base_when_wrong() {
        let order = vec!["b".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("b".to_string(), "a".to_string());
        let mut existing = HashMap::new();
        existing.insert("b".to_string(), pr(7, "main")); // base should be "a"

        let steps = plan(&order, &parent_of, "main", &existing);
        assert_eq!(steps[0].action, SubmitAction::UpdateBase);
        assert_eq!(steps[0].base, "a");
        assert_eq!(steps[0].pr, Some(7));
    }

    #[test]
    fn plan_up_to_date_when_base_matches() {
        let order = vec!["b".to_string()];
        let mut parent_of = HashMap::new();
        parent_of.insert("b".to_string(), "a".to_string());
        let mut existing = HashMap::new();
        existing.insert("b".to_string(), pr(7, "a"));

        let steps = plan(&order, &parent_of, "main", &existing);
        assert_eq!(steps[0].action, SubmitAction::UpToDate);
        assert_eq!(steps[0].pr, Some(7));
    }
}
