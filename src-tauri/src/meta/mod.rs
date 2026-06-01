use crate::error::Result;
use crate::{git, proc};
use std::collections::HashMap;
use std::path::Path;

/// Per-branch stack metadata, stored in git config under
/// `branch.<name>.gitstack-{parent,base,pr}`.
#[derive(Default, Clone, Debug)]
pub struct Meta {
    pub parent: Option<String>,
    pub base: Option<String>,
    #[allow(dead_code)] // written on submit; PR status itself comes from gh
    pub pr: Option<u64>,
}

/// Read all gitstack metadata for the repo, keyed by branch name.
pub fn all(repo: &Path) -> HashMap<String, Meta> {
    let mut map: HashMap<String, Meta> = HashMap::new();
    let r = match proc::run(
        "git",
        ["config", "--local", "--get-regexp", r"^branch\..*\.gitstack-"],
        Some(repo),
    ) {
        Ok(r) if r.success => r,
        _ => return map,
    };
    for line in r.stdout.lines() {
        // Each line is "branch.<name>.gitstack-<field> <value>".
        let mut sp = line.splitn(2, ' ');
        let key = sp.next().unwrap_or("");
        let val = sp.next().unwrap_or("").trim().to_string();
        let rest = match key.strip_prefix("branch.") {
            Some(r) => r,
            None => continue,
        };
        // Branch names may contain dots, so match the fixed suffix from the right.
        let dot = match rest.rfind(".gitstack-") {
            Some(d) => d,
            None => continue,
        };
        let name = rest[..dot].to_string();
        let field = &rest[dot + ".gitstack-".len()..];
        let e = map.entry(name).or_default();
        match field {
            "parent" => e.parent = Some(val),
            "base" => e.base = Some(val),
            "pr" => e.pr = val.parse().ok(),
            _ => {}
        }
    }
    map
}

fn key(branch: &str, field: &str) -> String {
    format!("branch.{}.gitstack-{}", branch, field)
}

fn set(repo: &Path, branch: &str, field: &str, value: &str) -> Result<()> {
    git::git(repo, &["config", key(branch, field).as_str(), value])?;
    Ok(())
}

/// Record `branch`'s parent and the restack base (the common ancestor with the parent).
pub fn set_parent(repo: &Path, branch: &str, parent: &str, base: &str) -> Result<()> {
    set(repo, branch, "parent", parent)?;
    set(repo, branch, "base", base)?;
    Ok(())
}

/// Update only the recorded restack base (after a successful rebase).
pub fn set_base(repo: &Path, branch: &str, base: &str) -> Result<()> {
    set(repo, branch, "base", base)
}

/// Record the PR number associated with a branch.
pub fn set_pr(repo: &Path, branch: &str, pr: u64) -> Result<()> {
    let pr = pr.to_string();
    set(repo, branch, "pr", pr.as_str())
}

/// Change only a branch's parent (keeping its recorded base — used when re-parenting
/// the children of a merged branch onto the merged branch's parent).
pub fn reparent(repo: &Path, branch: &str, parent: &str) -> Result<()> {
    set(repo, branch, "parent", parent)
}

/// Remove all gitstack metadata for a branch (best-effort per key).
pub fn unset_all(repo: &Path, branch: &str) -> Result<()> {
    for field in ["parent", "base", "pr"] {
        let _ = proc::run(
            "git",
            ["config", "--unset", key(branch, field).as_str()],
            Some(repo),
        );
    }
    Ok(())
}
