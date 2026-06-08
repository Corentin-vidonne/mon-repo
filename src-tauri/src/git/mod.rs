use crate::error::{AppError, Result};
use crate::model::{CommitDetail, CommitInfo, CommitNode, FileChange};
use crate::proc;
use std::path::{Path, PathBuf};

/// Run a git command in `repo`, returning stdout on success.
///
/// The analyzed repo is untrusted input (cloned repos / PR branches), so every invocation
/// blocks the `ext::` transport — a hostile `.gitmodules` could otherwise run a command —
/// neutralizes the pager, and never hangs on a credentials prompt. (External-diff / textconv
/// drivers are *not* reachable here: they require local config a clone/PR can't carry, only
/// `.gitattributes` travels — and setting an empty `diff.external` makes git spawn an empty
/// program, which breaks normal diffs.)
pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let mut full: Vec<&str> = vec!["-c", "protocol.ext.allow=never", "-c", "core.pager=cat"];
    full.extend_from_slice(args);
    let r = proc::run_env(
        "git",
        full.iter().copied(),
        Some(repo),
        &[("GIT_TERMINAL_PROMPT", "0")],
    )?;
    if !r.success {
        return Err(AppError::new(format!(
            "git {} failed: {}",
            args.join(" "),
            r.stderr.trim()
        )));
    }
    Ok(r.stdout)
}

/// Read a file from the working tree (e.g. a conflicted file with its markers).
pub fn read_working_file(repo: &Path, rel: &str) -> Result<String> {
    std::fs::read_to_string(repo.join(rel))
        .map_err(|e| AppError::new(format!("read {rel}: {e}")))
}

/// Best-effort base/ours/theirs versions of a conflicted file, from index stages
/// 1/2/3. Any side may be absent (e.g. add/add or delete conflicts) → `None`.
pub fn conflict_versions(
    repo: &Path,
    rel: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let show = |n: u8| git(repo, &["show", &format!(":{n}:{rel}")]).ok();
    (show(1), show(2), show(3))
}

/// Overwrite a working-tree file with resolved content.
pub fn write_working_file(repo: &Path, rel: &str, content: &str) -> Result<()> {
    std::fs::write(repo.join(rel), content).map_err(|e| AppError::new(format!("write {rel}: {e}")))
}

/// Stage a path (`git add`), marking a conflicted file as resolved.
pub fn stage_file(repo: &Path, rel: &str) -> Result<()> {
    git(repo, &["add", rel]).map(|_| ())
}

/// Whether the index holds staged changes relative to HEAD (`git diff --cached --quiet`
/// exits 0 when there are none). Used to guard the two commits of a line-level split.
pub fn has_staged_changes(repo: &Path) -> bool {
    !proc::run("git", ["diff", "--cached", "--quiet"], Some(repo))
        .map(|r| r.success)
        .unwrap_or(true)
}

#[derive(Clone, Debug)]
pub struct RawBranch {
    pub name: String,
    #[allow(dead_code)] // tip SHA, reserved for future use
    pub sha: String,
}

/// Resolve the repository root (errors if `path` is not inside a git repo).
pub fn repo_root(path: &Path) -> Result<String> {
    Ok(git(path, &["rev-parse", "--show-toplevel"])?
        .trim()
        .to_string())
}

/// The currently checked-out branch, or None if detached.
pub fn current_branch(repo: &Path) -> Option<String> {
    git(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// List local branches with their tip SHA. Branch names cannot contain spaces,
/// so a single space separator is unambiguous.
pub fn local_branches(repo: &Path) -> Result<Vec<RawBranch>> {
    let out = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname:short)",
            "refs/heads",
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            let mut p = l.splitn(2, ' ');
            let sha = p.next()?.trim().to_string();
            let name = p.next()?.trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(RawBranch { name, sha })
            }
        })
        .collect())
}

/// Best-effort trunk detection (never hardcodes main/master).
pub fn trunk(repo: &Path, branches: &[RawBranch]) -> String {
    if let Ok(s) = git(
        repo,
        &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(name) = s.trim().strip_prefix("origin/") {
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    for cand in ["main", "master", "trunk", "develop"] {
        if names.contains(&cand) {
            return cand.to_string();
        }
    }
    current_branch(repo).unwrap_or_else(|| "main".to_string())
}

/// `(ahead, behind)` of `branch` relative to `parent`:
/// ahead = commits on branch not on parent; behind = commits on parent not on branch.
pub fn ahead_behind(repo: &Path, parent: &str, branch: &str) -> (u32, u32) {
    let range = format!("{}...{}", parent, branch);
    match git(repo, &["rev-list", "--left-right", "--count", range.as_str()]) {
        Ok(s) => {
            let mut it = s.split_whitespace();
            let left = it.next().and_then(|x| x.parse().ok()).unwrap_or(0); // parent-only = behind
            let right = it.next().and_then(|x| x.parse().ok()).unwrap_or(0); // branch-only = ahead
            (right, left)
        }
        Err(_) => (0, 0),
    }
}

/// Whether the working tree has uncommitted changes.
pub fn is_dirty(repo: &Path) -> bool {
    git(repo, &["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Resolve a revision to its full SHA.
pub fn rev_parse(repo: &Path, rev: &str) -> Result<String> {
    Ok(git(repo, &["rev-parse", rev])?.trim().to_string())
}

/// The common ancestor of two revisions (used as the recorded restack base).
pub fn merge_base(repo: &Path, a: &str, b: &str) -> Result<String> {
    Ok(git(repo, &["merge-base", a, b])?.trim().to_string())
}

/// Whether `a` is an ancestor of `b` (so `b` already sits on top of `a`).
pub fn is_ancestor(repo: &Path, a: &str, b: &str) -> bool {
    proc::run("git", ["merge-base", "--is-ancestor", a, b], Some(repo))
        .map(|r| r.success)
        .unwrap_or(false)
}

/// Whether a local branch exists.
pub fn branch_exists(repo: &Path, name: &str) -> bool {
    let refname = format!("refs/heads/{}", name);
    git(repo, &["show-ref", "--verify", "--quiet", refname.as_str()]).is_ok()
}

/// Create a new branch at `start` and check it out.
pub fn create_branch(repo: &Path, name: &str, start: &str) -> Result<()> {
    git(repo, &["checkout", "-b", name, start])?;
    Ok(())
}

/// Check out an existing branch.
pub fn checkout(repo: &Path, name: &str) -> Result<()> {
    git(repo, &["checkout", name])?;
    Ok(())
}

fn git_dir(repo: &Path) -> PathBuf {
    match git(repo, &["rev-parse", "--git-dir"]) {
        Ok(s) => {
            let p = PathBuf::from(s.trim());
            if p.is_absolute() {
                p
            } else {
                repo.join(p)
            }
        }
        Err(_) => repo.join(".git"),
    }
}

/// Whether a rebase is currently paused / in progress.
pub fn rebase_in_progress(repo: &Path) -> bool {
    let d = git_dir(repo);
    d.join("rebase-merge").exists() || d.join("rebase-apply").exists()
}

/// Files with unresolved merge conflicts.
pub fn conflicted_files(repo: &Path) -> Vec<String> {
    git(repo, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// The branch being rebased (read from `.git/rebase-merge/head-name`).
pub fn rebase_head_branch(repo: &Path) -> Option<String> {
    let f = git_dir(repo).join("rebase-merge").join("head-name");
    std::fs::read_to_string(f)
        .ok()
        .map(|s| s.trim().trim_start_matches("refs/heads/").to_string())
        .filter(|s| !s.is_empty())
}

const REBASE_ENV: [(&str, &str); 2] = [("GIT_EDITOR", "true"), ("GIT_SEQUENCE_EDITOR", "true")];

/// Rebase `branch` from `oldbase` onto `newbase`.
/// `Ok(true)` = completed, `Ok(false)` = paused on conflict, `Err` = hard failure.
pub fn rebase_onto(repo: &Path, newbase: &str, oldbase: &str, branch: &str) -> Result<bool> {
    let r = proc::run_env(
        "git",
        ["rebase", "--onto", newbase, oldbase, branch],
        Some(repo),
        &REBASE_ENV,
    )?;
    if r.success {
        Ok(true)
    } else if rebase_in_progress(repo) {
        Ok(false)
    } else {
        Err(AppError::new(format!(
            "git rebase --onto {} {} {} failed: {}",
            newbase,
            oldbase,
            branch,
            r.stderr.trim()
        )))
    }
}

static EDIT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Run `git rebase -i <upstream> <branch>` non-interactively by feeding git a
/// prepared instruction sheet (`todo`) and, for `reword` steps, a `message`.
/// Git invokes `GIT_SEQUENCE_EDITOR`/`GIT_EDITOR` through its own `sh`, so a
/// portable `cp <ourfile>` (git appends the destination path) overwrites the
/// editor's file with ours — no GUI, no shell scripting.
/// `Ok(true)` = completed, `Ok(false)` = paused on conflict, `Err` = hard failure.
pub fn rebase_edit(
    repo: &Path,
    upstream: &str,
    branch: &str,
    todo: &str,
    message: Option<&str>,
) -> Result<bool> {
    let dir = std::env::temp_dir();
    let n = EDIT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pid = std::process::id();

    let todo_path = dir.join(format!("gitui-todo-{pid}-{n}.txt"));
    std::fs::write(&todo_path, todo).map_err(|e| AppError::new(format!("write todo: {e}")))?;
    let seq_editor = format!("cp '{}'", todo_path.to_string_lossy().replace('\\', "/"));

    let msg_path = match message {
        Some(m) => {
            let p = dir.join(format!("gitui-msg-{pid}-{n}.txt"));
            std::fs::write(&p, m).map_err(|e| AppError::new(format!("write msg: {e}")))?;
            Some(p)
        }
        None => None,
    };
    let editor = match &msg_path {
        Some(p) => format!("cp '{}'", p.to_string_lossy().replace('\\', "/")),
        None => "true".to_string(),
    };

    let envs: [(&str, &str); 2] = [
        ("GIT_SEQUENCE_EDITOR", seq_editor.as_str()),
        ("GIT_EDITOR", editor.as_str()),
    ];
    let r = proc::run_env("git", ["rebase", "-i", upstream, branch], Some(repo), &envs)?;

    let _ = std::fs::remove_file(&todo_path);
    if let Some(p) = &msg_path {
        let _ = std::fs::remove_file(p);
    }

    if r.success {
        Ok(true)
    } else if rebase_in_progress(repo) {
        Ok(false)
    } else {
        Err(AppError::new(format!("git rebase -i failed: {}", r.stderr.trim())))
    }
}

/// `git rebase --continue` with no interactive editor.
pub fn rebase_continue(repo: &Path) -> Result<proc::Run> {
    Ok(proc::run_env(
        "git",
        ["rebase", "--continue"],
        Some(repo),
        &REBASE_ENV,
    )?)
}

/// `git rebase --abort`.
pub fn rebase_abort(repo: &Path) -> Result<()> {
    git(repo, &["rebase", "--abort"])?;
    Ok(())
}

/// Push a branch to origin, safely overwriting a rebased history.
pub fn push(repo: &Path, branch: &str) -> Result<()> {
    git(
        repo,
        &[
            "push",
            "--force-with-lease",
            "--force-if-includes",
            "-u",
            "origin",
            branch,
        ],
    )?;
    Ok(())
}

/// The subject line of a revision's tip commit.
pub fn commit_subject(repo: &Path, rev: &str) -> Option<String> {
    git(repo, &["log", "-1", "--format=%s", rev])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Commits reachable from `branch` but not from `base` (or the last `limit` commits
/// of `branch` when `base` is None), newest first.
pub fn commits(
    repo: &Path,
    base: Option<&str>,
    branch: &str,
    limit: usize,
) -> Result<Vec<CommitInfo>> {
    let range = match base {
        Some(b) => format!("{}..{}", b, branch),
        None => branch.to_string(),
    };
    let max = format!("--max-count={}", limit);
    let out = git(
        repo,
        &[
            "log",
            max.as_str(),
            "--format=%h%x1f%s%x1f%an%x1f%ad",
            "--date=relative",
            range.as_str(),
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            if l.trim().is_empty() {
                return None;
            }
            let mut f = l.split('\u{1f}');
            Some(CommitInfo {
                sha: f.next()?.to_string(),
                subject: f.next().unwrap_or("").to_string(),
                author: f.next().unwrap_or("").to_string(),
                date: f.next().unwrap_or("").to_string(),
            })
        })
        .collect())
}

/// Common ancestor of several revisions (the stack's fork point).
pub fn merge_base_octopus(repo: &Path, revs: &[String]) -> Option<String> {
    if revs.is_empty() {
        return None;
    }
    let mut args: Vec<&str> = vec!["merge-base", "--octopus"];
    for r in revs {
        args.push(r.as_str());
    }
    git(repo, &args)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The commit DAG reachable from `revs`, bounded below by `floor` (its fork point),
/// newest first. `refs` is left empty for the caller to fill from branch tips.
pub fn commit_graph(
    repo: &Path,
    revs: &[String],
    floor: Option<&str>,
    limit: usize,
) -> Result<Vec<CommitNode>> {
    if revs.is_empty() {
        return Ok(Vec::new());
    }
    let max = format!("--max-count={}", limit);
    let floor_excl = floor.map(|f| format!("{}^@", f));
    let mut args: Vec<&str> = vec![
        "log",
        "--topo-order",
        "--date=relative",
        max.as_str(),
        "--format=%H%x1f%P%x1f%h%x1f%s%x1f%an%x1f%ad",
    ];
    for r in revs {
        args.push(r.as_str());
    }
    if let Some(fe) = &floor_excl {
        args.push("--not");
        args.push(fe.as_str());
    }
    let out = git(repo, &args)?;
    Ok(out
        .lines()
        .filter_map(|l| {
            if l.trim().is_empty() {
                return None;
            }
            let mut f = l.split('\u{1f}');
            let sha = f.next()?.to_string();
            let parents = f
                .next()
                .unwrap_or("")
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            let short_sha = f.next().unwrap_or("").to_string();
            let subject = f.next().unwrap_or("").to_string();
            let author = f.next().unwrap_or("").to_string();
            let date = f.next().unwrap_or("").to_string();
            Some(CommitNode {
                sha,
                short_sha,
                parents,
                subject,
                author,
                date,
                refs: Vec::new(),
            })
        })
        .collect())
}

/// Full message and changed files for a single commit.
pub fn commit_detail(repo: &Path, sha: &str) -> Result<CommitDetail> {
    let message = git(repo, &["log", "-1", "--format=%B", sha])?
        .trim_end()
        .to_string();
    let files_out = git(repo, &["show", "--name-status", "--format=", sha])?;
    let files = files_out
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            let mut p = l.splitn(2, '\t');
            let status = p.next()?.trim().to_string();
            let path = p.next()?.trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(FileChange { status, path })
            }
        })
        .collect();

    let raw = git(repo, &["show", "--format=", "--patch", "--no-color", sha]).unwrap_or_default();
    let raw = raw.trim_start_matches('\n').to_string();
    let diff = if raw.len() > 200_000 {
        let mut end = 200_000;
        while end > 0 && !raw.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n… (diff truncated)", &raw[..end])
    } else {
        raw
    };

    Ok(CommitDetail {
        message,
        files,
        diff,
    })
}

/// Fetch from origin (prune deleted remote branches).
pub fn fetch(repo: &Path) -> Result<()> {
    git(repo, &["fetch", "--prune", "origin"])?;
    Ok(())
}

/// Fast-forward the local `branch` to `origin/branch` (never a non-ff clobber).
/// Errors if it isn't a fast-forward, so the caller can ignore it best-effort.
pub fn fast_forward(repo: &Path, branch: &str, current: Option<&str>) -> Result<()> {
    let remote = format!("origin/{}", branch);
    if current == Some(branch) {
        // Branch is checked out — merge fast-forward only.
        git(repo, &["merge", "--ff-only", remote.as_str()])?;
    } else {
        // Not checked out — update the ref directly (ff-only by default).
        let refspec = format!("{0}:{0}", branch);
        git(repo, &["fetch", "origin", refspec.as_str()])?;
    }
    Ok(())
}

/// Commits on local `branch` not yet on `origin/branch` (0 if there is no remote branch).
pub fn remote_ahead(repo: &Path, branch: &str) -> u32 {
    let remote = format!("origin/{}", branch);
    if git(repo, &["rev-parse", "--verify", "--quiet", remote.as_str()]).is_err() {
        return 0;
    }
    let range = format!("{}..{}", remote, branch);
    git(repo, &["rev-list", "--count", range.as_str()])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}
