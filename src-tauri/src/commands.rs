use crate::error::{AppError, Result};
use crate::model::{
    Branch, CommitDetail, CommitInfo, CommitNode, ConflictState, RepoView, StackNode, SubmitStepInfo,
};
use crate::{assist, git, github, links, meta, proc, stack};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Environment health: are `git` and `gh` available, and is `gh` authenticated?
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    git_version: Option<String>,
    gh_version: Option<String>,
    gh_authenticated: bool,
    gh_account: Option<String>,
}

#[tauri::command]
pub fn health() -> Health {
    let git_version = proc::run("git", ["--version"], None)
        .ok()
        .filter(|r| r.success)
        .map(|r| r.stdout.trim().trim_start_matches("git version ").to_string());

    let gh_version = proc::run("gh", ["--version"], None)
        .ok()
        .filter(|r| r.success)
        .and_then(|r| r.stdout.lines().next().map(|l| l.trim().to_string()));

    let auth = proc::run("gh", ["auth", "status"], None).ok();
    let gh_authenticated = auth.as_ref().map(|r| r.success).unwrap_or(false);
    // `gh auth status` prints e.g. "Logged in to github.com account <name> (keyring)".
    let gh_account = auth.as_ref().and_then(|r| {
        let text = format!("{} {}", r.stdout, r.stderr);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        tokens
            .windows(2)
            .find(|w| w[0] == "account")
            .map(|w| w[1].to_string())
    });

    Health {
        git_version,
        gh_version,
        gh_authenticated,
        gh_account,
    }
}

/// Build the inter-repo dependency graph across the given repo paths.
#[tauri::command]
pub fn repo_graph(paths: Vec<String>) -> Result<links::RepoGraph> {
    Ok(links::analyze(&paths))
}

/// Read a repository and build its stack view.
#[tauri::command]
pub fn get_repo_view(path: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    build_view(Path::new(&root))
}

/// Derive a clone folder name from a repo URL (or path):
/// `https://github.com/owner/repo.git` -> `repo`, `git@host:owner/repo.git` -> `repo`.
fn repo_name_from_url(url: &str) -> String {
    let u = url.trim().trim_end_matches(['/', '\\']);
    let last = u
        .rsplit(|c| c == '/' || c == ':' || c == '\\')
        .next()
        .unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

/// `git clone <url>` into `dest_parent/<derived-name>`, then return the new repo's view.
/// Blocking worker; the Tauri command runs it off the UI thread.
fn clone_repo_blocking(url: &str, dest_parent: &str) -> Result<RepoView> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::new("Repository URL is required"));
    }
    let parent = Path::new(dest_parent);
    if !parent.is_dir() {
        return Err(AppError::new("Destination folder does not exist"));
    }
    let name = repo_name_from_url(url);
    if name.is_empty() {
        return Err(AppError::new("Could not derive a folder name from the URL"));
    }
    let target = parent.join(&name);
    if target.exists() {
        return Err(AppError::new(format!(
            "'{}' already exists in that folder",
            name
        )));
    }
    let target_str = target.to_string_lossy().to_string();
    // GIT_TERMINAL_PROMPT=0 makes git fail fast instead of hanging on a credentials
    // prompt for a private repo (the captured subprocess has no interactive stdin).
    let r = proc::run_env(
        "git",
        ["clone", url, target_str.as_str()],
        Some(parent),
        &[("GIT_TERMINAL_PROMPT", "0")],
    )?;
    if !r.success {
        return Err(AppError::new(format!(
            "git clone failed: {}",
            r.stderr.trim()
        )));
    }
    let root = git::repo_root(&target)?;
    build_view(Path::new(&root))
}

/// Clone a repository from `url` into `dest_parent`, returning its stack view.
#[tauri::command]
pub async fn clone_repo(url: String, dest_parent: String) -> Result<RepoView> {
    tauri::async_runtime::spawn_blocking(move || clone_repo_blocking(&url, &dest_parent))
        .await
        .map_err(|e| AppError::new(format!("clone task failed: {}", e)))?
}

/// Create a new branch on top of `parent` (defaults to the current branch) and track it.
#[tauri::command]
pub fn create_branch(path: String, name: String, parent: Option<String>) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::new("Branch name is required"));
    }
    if name.split_whitespace().count() != 1 {
        return Err(AppError::new("Branch name cannot contain spaces"));
    }
    if git::branch_exists(repo, &name) {
        return Err(AppError::new(format!("Branch '{}' already exists", name)));
    }

    let parent = match parent {
        Some(p) if !p.trim().is_empty() => p.trim().to_string(),
        _ => git::current_branch(repo)
            .ok_or_else(|| AppError::new("Detached HEAD: specify a parent branch"))?,
    };
    if !git::branch_exists(repo, &parent) {
        return Err(AppError::new(format!("Parent '{}' does not exist", parent)));
    }

    // Branch from HEAD without switching when the parent is where we already are
    // (works even with uncommitted changes); only require a clean tree when the
    // parent is a *different* branch, since git must then switch the working tree.
    let head = git::rev_parse(repo, "HEAD").ok();
    let parent_tip = git::rev_parse(repo, &parent).ok();
    if head.is_some() && head == parent_tip {
        git::git(repo, &["checkout", "-b", name.as_str()])?;
    } else if git::is_dirty(repo) {
        return Err(AppError::new(format!(
            "You have uncommitted changes — commit or stash them before branching off '{}'.",
            parent
        )));
    } else {
        git::create_branch(repo, &name, &parent)?;
    }

    let base = git::merge_base(repo, &parent, &name)?;
    meta::set_parent(repo, &name, &parent, &base)?;
    build_view(repo)
}

/// Set (or change) a branch's parent in the stack. Used both to track an untracked
/// branch and to re-parent an existing one. Rejects cycles.
#[tauri::command]
pub fn set_parent(path: String, branch: String, parent: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);

    if branch == parent {
        return Err(AppError::new("A branch cannot be its own parent"));
    }
    if !git::branch_exists(repo, &branch) {
        return Err(AppError::new(format!("Branch '{}' does not exist", branch)));
    }
    if !git::branch_exists(repo, &parent) {
        return Err(AppError::new(format!("Parent '{}' does not exist", parent)));
    }

    // Cycle check: walking the parent's ancestry must not reach `branch`.
    let metas = meta::all(repo);
    let mut cursor = Some(parent.clone());
    while let Some(c) = cursor {
        if c == branch {
            return Err(AppError::new("That would create a cycle in the stack"));
        }
        cursor = metas.get(&c).and_then(|m| m.parent.clone());
    }

    let base = git::merge_base(repo, &parent, &branch)?;
    meta::set_parent(repo, &branch, &parent, &base)?;
    build_view(repo)
}

/// Stop tracking a branch (remove its gitstack metadata).
#[tauri::command]
pub fn untrack_branch(path: String, branch: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    meta::unset_all(repo, &branch)?;
    build_view(repo)
}

/// Restack the stack (or a subtree) by rebasing each branch onto its parent.
/// Returns the refreshed view; if a conflict pauses the rebase, `view.conflict` is set.
#[tauri::command]
pub fn restack(path: String, from: Option<String>) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return build_view(repo); // already paused — surface the existing conflict
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before restacking"));
    }
    stack::run(repo, from.as_deref())?;
    build_view(repo)
}

/// Continue a paused restack after the user resolved conflicts and staged them.
#[tauri::command]
pub fn continue_restack(path: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if !git::rebase_in_progress(repo) {
        return Err(AppError::new("No restack in progress"));
    }
    stack::continue_(repo)?;
    build_view(repo)
}

/// Abort a paused restack.
#[tauri::command]
pub fn abort_restack(path: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        stack::abort(repo)?;
    }
    build_view(repo)
}

/// Push each branch and create/update its PR, bottom-up, with bases pointing at parents.
#[tauri::command]
pub fn submit(
    path: String,
    from: Option<String>,
    draft: bool,
    titles: HashMap<String, String>,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack before submitting"));
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before submitting"));
    }

    let metas = meta::all(repo);
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let existing = github::list_prs(repo).ok_or_else(|| {
        AppError::new(
            "GitHub unavailable — check `gh auth status` and that this repo has a GitHub remote",
        )
    })?;

    let order = stack::topo_order(&metas, &trunk, from.as_deref());
    let parent_of: HashMap<String, String> = metas
        .iter()
        .filter_map(|(k, m)| m.parent.clone().map(|p| (k.clone(), p)))
        .collect();
    let steps = github::plan(&order, &parent_of, &trunk, &existing);

    for step in &steps {
        git::push(repo, &step.branch)?;
        match step.action {
            github::SubmitAction::Create => {
                let title = titles
                    .get(&step.branch)
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .or_else(|| git::commit_subject(repo, &step.branch))
                    .unwrap_or_else(|| step.branch.clone());
                let body = format!("Stacked PR — base `{}`. Managed by gitui.", step.base);
                let created =
                    github::create_pr(repo, &step.branch, &step.base, &title, &body, draft)?;
                meta::set_pr(repo, &step.branch, created.number)?;
            }
            github::SubmitAction::UpdateBase => {
                if let Some(n) = step.pr {
                    github::set_pr_base(repo, n, &step.base)?;
                }
            }
            github::SubmitAction::UpToDate => {}
        }
    }

    build_view(repo)
}

/// Preview what `submit` would do (which PRs get created/updated), with suggested titles.
#[tauri::command]
pub fn submit_plan(path: String, from: Option<String>) -> Result<Vec<SubmitStepInfo>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let metas = meta::all(repo);
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let existing = github::list_prs(repo).unwrap_or_default();
    let order = stack::topo_order(&metas, &trunk, from.as_deref());
    let parent_of: HashMap<String, String> = metas
        .iter()
        .filter_map(|(k, m)| m.parent.clone().map(|p| (k.clone(), p)))
        .collect();
    let steps = github::plan(&order, &parent_of, &trunk, &existing);
    Ok(steps
        .into_iter()
        .map(|s| {
            let action = match s.action {
                github::SubmitAction::Create => "create",
                github::SubmitAction::UpdateBase => "update",
                github::SubmitAction::UpToDate => "uptodate",
            }
            .to_string();
            let default_title = if matches!(s.action, github::SubmitAction::Create) {
                git::commit_subject(repo, &s.branch).unwrap_or_else(|| s.branch.clone())
            } else {
                String::new()
            };
            SubmitStepInfo {
                branch: s.branch,
                base: s.base,
                action,
                pr: s.pr,
                default_title,
            }
        })
        .collect())
}

/// Sync with origin: fetch, fast-forward the trunk, clean up merged PRs (re-parent + untrack,
/// never delete), then restack survivors onto the updated trunk.
#[tauri::command]
pub fn sync(path: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack before syncing"));
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before syncing"));
    }

    git::fetch(repo)?;

    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let current = git::current_branch(repo);
    let _ = git::fast_forward(repo, &trunk, current.as_deref()); // best-effort: skip if non-ff

    let merged: std::collections::HashSet<String> =
        github::merged_prs(repo).into_iter().collect();
    if !merged.is_empty() {
        stack::cleanup_merged(repo, &merged, &trunk)?;
    }

    let _ = stack::run(repo, None)?; // may pause on conflict; surfaced by build_view
    build_view(repo)
}

/// Check out a branch (switch the current branch).
#[tauri::command]
pub fn checkout(path: String, branch: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    git::checkout(repo, &branch)?;
    build_view(repo)
}

/// Publish a branch to origin (`git push -u origin <branch>`, safe force-with-lease).
/// Use for the first push of a local branch or to push new local commits.
#[tauri::command]
pub fn publish_branch(path: String, branch: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if !git::branch_exists(repo, &branch) {
        return Err(AppError::new(format!("Branch '{}' does not exist", branch)));
    }
    git::push(repo, &branch)?;
    build_view(repo)
}

/// The commits a branch carries on top of its parent (or recent history for the trunk).
#[tauri::command]
pub fn branch_commits(path: String, branch: String) -> Result<Vec<CommitInfo>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let base = if branch == trunk {
        None
    } else {
        let metas = meta::all(repo);
        Some(
            metas
                .get(&branch)
                .and_then(|m| m.parent.clone())
                .unwrap_or_else(|| trunk.clone()),
        )
    };
    git::commits(repo, base.as_deref(), &branch, 30)
}

/// The commit DAG with branch tips labeled. `branches` (when non-empty) restricts the
/// view to those branches; None/empty shows trunk + every local branch (default).
#[tauri::command]
pub fn stack_commits(path: String, branches: Option<Vec<String>>) -> Result<Vec<CommitNode>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);

    let filter: Option<std::collections::HashSet<String>> = branches
        .map(|b| b.into_iter().collect::<std::collections::HashSet<_>>())
        .filter(|s| !s.is_empty());

    // Which branch tips to walk from.
    let mut revs: Vec<String> = Vec::new();
    match &filter {
        Some(set) => {
            for b in &raw {
                if set.contains(&b.name) {
                    revs.push(b.name.clone());
                }
            }
        }
        None => {
            if git::branch_exists(repo, &trunk) {
                revs.push(trunk.clone());
            }
            for b in &raw {
                if b.name != trunk {
                    revs.push(b.name.clone());
                }
            }
        }
    }
    if revs.is_empty() {
        return Ok(Vec::new());
    }

    // Bound history below by the common ancestor of the shown branches AND the trunk,
    // so a single selected branch still shows its commits down to where it forked.
    let mut floor_revs = revs.clone();
    if !floor_revs.iter().any(|r| r == &trunk) && git::branch_exists(repo, &trunk) {
        floor_revs.push(trunk.clone());
    }
    let floor = git::merge_base_octopus(repo, &floor_revs);
    let mut nodes = git::commit_graph(repo, &revs, floor.as_deref(), 400)?;

    let mut tips: HashMap<String, Vec<String>> = HashMap::new();
    for b in &raw {
        tips.entry(b.sha.clone()).or_default().push(b.name.clone());
    }
    for n in &mut nodes {
        if let Some(names) = tips.get(&n.sha) {
            let mut names = names.clone();
            names.sort();
            n.refs = names;
        }
    }
    Ok(nodes)
}

/// Full message + changed files for a single commit.
#[tauri::command]
pub fn commit_detail(path: String, sha: String) -> Result<CommitDetail> {
    let root = git::repo_root(Path::new(&path))?;
    git::commit_detail(Path::new(&root), &sha)
}

/// Open a terminal running `claude`, pre-seeded with a prompt to analyze this commit.
#[tauri::command]
pub fn analyze_commit(path: String, sha: String, mode: String) -> Result<()> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let prompt = assist::analysis_prompt(repo, &sha, &mode)?;
    assist::launch_claude(repo, &prompt)
}

/// Full detail for a single pull request (metadata + files + unified diff + comments/reviews).
#[tauri::command]
pub fn pr_detail(path: String, number: u64) -> Result<crate::model::PrDetail> {
    let root = git::repo_root(Path::new(&path))?;
    github::pr_detail(Path::new(&root), number)
}

/// List issues (`state` = "open" | "closed" | "all"). Empty list when gh is unavailable.
#[tauri::command]
pub fn list_issues(path: String, state: String) -> Result<Vec<crate::model::IssueSummary>> {
    let root = git::repo_root(Path::new(&path))?;
    Ok(github::list_issues(Path::new(&root), &state).unwrap_or_default())
}

/// List pull requests (`state` = "open" | "closed" | "merged" | "all").
#[tauri::command]
pub fn list_pull_requests(path: String, state: String) -> Result<Vec<crate::model::PrSummary>> {
    let root = git::repo_root(Path::new(&path))?;
    Ok(github::list_pull_requests(Path::new(&root), &state).unwrap_or_default())
}

/// Full detail for a single issue, including its comments.
#[tauri::command]
pub fn issue_detail(path: String, number: u64) -> Result<crate::model::IssueDetail> {
    let root = git::repo_root(Path::new(&path))?;
    github::issue_detail(Path::new(&root), number)
}

/// Check a repo for new activity (remote commits, new/updated PRs & issues) since
/// the last time it was marked seen. First check on a repo seeds the baseline silently.
#[tauri::command]
pub async fn check_updates(
    app: tauri::AppHandle,
    path: String,
) -> Result<crate::notify::UpdateReport> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new(e.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let _ = git::fetch(repo); // best-effort: offline / no remote just yields no trunk change
        Ok(crate::notify::check(&dir, &root, repo))
    })
    .await
    .map_err(|e| AppError::new(format!("update check failed: {}", e)))?
}

/// Record a repo's current activity as seen, clearing its update indicator.
#[tauri::command]
pub fn mark_updates_seen(app: tauri::AppHandle, path: String) -> Result<()> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new(e.to_string()))?;
    let root = git::repo_root(Path::new(&path))?;
    crate::notify::mark_seen(&dir, &root, Path::new(&root));
    Ok(())
}

/// Open the repo's root folder in VS Code (`code <repo>`). On Windows `code` is a
/// `.cmd` shim, so we resolve its full path via `where` and spawn without a console.
#[tauri::command]
pub fn open_in_vscode(path: String) -> Result<()> {
    let root = git::repo_root(Path::new(&path))?;

    #[cfg(windows)]
    let mut cmd = {
        let mut c = proc::command("cmd");
        c.args(["/c", "code"]);
        c
    };

    #[cfg(not(windows))]
    let mut cmd = proc::command("code");

    cmd.arg(&root)
        .spawn()
        .map_err(|e| {
            AppError::new(format!(
                "Could not launch VS Code ({}). Is the `code` command on your PATH?",
                e
            ))
        })?;
    Ok(())
}

/// List all Markdown files in the repo (relative paths).
#[tauri::command]
pub fn list_markdown(path: String) -> Result<Vec<String>> {
    let root = git::repo_root(Path::new(&path))?;
    Ok(crate::docs::list(Path::new(&root)))
}

/// Read a Markdown file's contents.
#[tauri::command]
pub fn read_markdown(path: String, rel: String) -> Result<String> {
    let root = git::repo_root(Path::new(&path))?;
    crate::docs::read(Path::new(&root), &rel)
}

/// Create (or overwrite) a Markdown file with `content` and commit it on `branch`.
/// Switches to `branch` first (requires a clean tree if it differs from the current one).
#[tauri::command]
pub fn create_markdown(
    path: String,
    branch: String,
    rel: String,
    content: String,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);

    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    let rel = crate::docs::safe_rel(&rel)?;
    if !git::branch_exists(repo, &branch) {
        return Err(AppError::new(format!("Branch '{}' does not exist", branch)));
    }

    // Switch to the target branch if needed (git refuses with a dirty tree, which is fine).
    if git::current_branch(repo).as_deref() != Some(branch.as_str()) {
        if git::is_dirty(repo) {
            return Err(AppError::new(
                "Commit or stash your changes before writing to another branch",
            ));
        }
        git::checkout(repo, &branch)?;
    }

    crate::docs::write(repo, &rel, &content)?;
    git::git(repo, &["add", "--", rel.as_str()])?;
    let msg = format!("docs: add {}", rel);
    git::git(repo, &["commit", "-m", msg.as_str()])?;
    build_view(repo)
}

/// Pure builder: assemble the [`RepoView`] for an already-resolved repo root.
/// Kept separate from the Tauri command so it can be unit-tested against temp repos.
fn build_view(repo: &Path) -> Result<RepoView> {
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let current = git::current_branch(repo);
    let metas = meta::all(repo);
    let dirty = git::is_dirty(repo);
    let conflict = if git::rebase_in_progress(repo) {
        Some(ConflictState {
            branch: git::rebase_head_branch(repo),
            files: git::conflicted_files(repo),
        })
    } else {
        None
    };
    let prs = github::list_prs(repo);
    let prs_available = prs.is_some();
    let pr_map = prs.unwrap_or_default();

    let mut by_name: HashMap<String, Branch> = HashMap::new();
    for b in &raw {
        let m = metas.get(&b.name).cloned().unwrap_or_default();
        let is_trunk = b.name == trunk;
        let parent = if is_trunk { None } else { m.parent.clone() };
        let (ahead, behind) = match &parent {
            Some(par) => git::ahead_behind(repo, par, &b.name),
            None => (0, 0),
        };
        let is_current = current.as_deref() == Some(b.name.as_str());
        by_name.insert(
            b.name.clone(),
            Branch {
                name: b.name.clone(),
                parent,
                base_sha: m.base.clone(),
                is_trunk,
                is_current,
                ahead,
                behind,
                dirty: is_current && dirty,
                needs_push: git::remote_ahead(repo, &b.name) > 0,
                tracked: m.parent.is_some(),
                pr: pr_map.get(&b.name).cloned(),
            },
        );
    }

    // Ensure the trunk is always present as a node, even with no local branch yet.
    if !by_name.contains_key(&trunk) {
        by_name.insert(
            trunk.clone(),
            Branch {
                name: trunk.clone(),
                parent: None,
                base_sha: None,
                is_trunk: true,
                is_current: current.as_deref() == Some(trunk.as_str()),
                ahead: 0,
                behind: 0,
                dirty: false,
                needs_push: false,
                tracked: false,
                pr: None,
            },
        );
    }

    // Parent -> children adjacency (only for parents we actually know about).
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for b in by_name.values() {
        if let Some(par) = &b.parent {
            if by_name.contains_key(par) {
                children
                    .entry(par.clone())
                    .or_default()
                    .push(b.name.clone());
            }
        }
    }

    // Forest of stacks: a branch is a root when it has no parent or an unknown parent.
    // Building a forest (not just the trunk's tree) keeps branches that sit off the
    // trunk — and anything stacked on them — visible instead of orphaned.
    let mut root_names: Vec<String> = by_name
        .values()
        .filter(|b| match &b.parent {
            None => true,
            Some(p) => !by_name.contains_key(p),
        })
        .map(|b| b.name.clone())
        .collect();
    // Trunk first, then the rest alphabetically.
    root_names.sort_by(|a, b| (*b == trunk).cmp(&(*a == trunk)).then_with(|| a.cmp(b)));
    let roots: Vec<StackNode> = root_names
        .iter()
        .filter_map(|n| build_node(n, &by_name, &children))
        .collect();
    // Untracked branches are now shown as forest roots, so this list stays empty.
    let untracked: Vec<Branch> = Vec::new();

    let name = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.to_string_lossy().to_string());

    Ok(RepoView {
        repo_root: repo.to_string_lossy().to_string(),
        name,
        trunk,
        current_branch: current,
        roots,
        untracked,
        prs_available,
        dirty,
        conflict,
    })
}

/// Recursively assemble a stack node and its children.
fn build_node(
    name: &str,
    by_name: &HashMap<String, Branch>,
    children: &HashMap<String, Vec<String>>,
) -> Option<StackNode> {
    let branch = by_name.get(name)?.clone();
    let kids = children
        .get(name)
        .map(|names| {
            let mut names = names.clone();
            names.sort();
            names
                .iter()
                .filter_map(|c| build_node(c, by_name, children))
                .collect()
        })
        .unwrap_or_default();
    Some(StackNode {
        branch,
        children: kids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::process::Command;

    fn git_ok(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn init_repo(repo: &Path) {
        git_ok(repo, &["init", "-b", "main"]);
        git_ok(repo, &["config", "user.email", "t@example.dev"]);
        git_ok(repo, &["config", "user.name", "Test"]);
        git_ok(repo, &["config", "commit.gpgsign", "false"]);
        git_ok(repo, &["commit", "--allow-empty", "-m", "init"]);
    }

    fn commit(repo: &Path, msg: &str) {
        git_ok(repo, &["commit", "--allow-empty", "-m", msg]);
    }

    #[test]
    fn builds_linear_stack_tree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);

        git_ok(repo, &["checkout", "-b", "feat-a"]);
        commit(repo, "a1");
        git_ok(repo, &["config", "branch.feat-a.gitstack-parent", "main"]);

        git_ok(repo, &["checkout", "-b", "feat-b"]);
        commit(repo, "b1");
        commit(repo, "b2");
        git_ok(repo, &["config", "branch.feat-b.gitstack-parent", "feat-a"]);

        let view = build_view(repo).expect("build view");
        assert_eq!(view.trunk, "main");
        assert_eq!(view.current_branch.as_deref(), Some("feat-b"));
        assert_eq!(view.roots.len(), 1);

        let root = &view.roots[0];
        assert_eq!(root.branch.name, "main");
        assert!(root.branch.is_trunk);
        assert_eq!(root.children.len(), 1);

        let a = &root.children[0];
        assert_eq!(a.branch.name, "feat-a");
        assert_eq!(a.branch.parent.as_deref(), Some("main"));
        assert!(a.branch.tracked);
        assert_eq!(a.branch.ahead, 1);
        assert_eq!(a.branch.behind, 0);
        assert_eq!(a.children.len(), 1);

        let b = &a.children[0];
        assert_eq!(b.branch.name, "feat-b");
        assert_eq!(b.branch.ahead, 2);
        assert!(view.untracked.is_empty());
    }

    #[test]
    fn detects_behind_when_parent_moves() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);

        git_ok(repo, &["checkout", "-b", "feat-a"]);
        commit(repo, "a1");
        git_ok(repo, &["config", "branch.feat-a.gitstack-parent", "main"]);

        // Parent advances after feat-a was created.
        git_ok(repo, &["checkout", "main"]);
        commit(repo, "main2");
        git_ok(repo, &["checkout", "feat-a"]);

        let view = build_view(repo).unwrap();
        let a = &view.roots[0].children[0];
        assert_eq!(a.branch.name, "feat-a");
        assert_eq!(a.branch.ahead, 1);
        assert_eq!(a.branch.behind, 1, "feat-a should be 1 behind and need a restack");
    }

    #[test]
    fn untracked_branch_appears_as_a_root() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "loose"]);
        commit(repo, "x");

        let view = build_view(repo).unwrap();
        assert_eq!(view.trunk, "main");
        // Untracked branches are forest roots now (not a separate list).
        assert!(view.untracked.is_empty());
        assert!(
            view.roots.iter().any(|n| n.branch.name == "loose"),
            "untracked `loose` must be a visible root"
        );
        let main_root = view.roots.iter().find(|n| n.branch.name == "main").unwrap();
        assert_eq!(main_root.children.len(), 0);
    }

    #[test]
    fn create_branch_tracks_with_parent() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);

        let view = create_branch(
            repo.to_string_lossy().to_string(),
            "feat-x".to_string(),
            None,
        )
        .unwrap();
        let node = &view.roots[0].children[0];
        assert_eq!(node.branch.name, "feat-x");
        assert_eq!(node.branch.parent.as_deref(), Some("main"));
        assert!(node.branch.tracked);
        assert_eq!(view.current_branch.as_deref(), Some("feat-x"));
    }

    #[test]
    fn branch_stacked_on_untracked_branch_is_visible() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        // `loose` is untracked (off main, no gitstack metadata).
        git_ok(repo, &["checkout", "-b", "loose"]);
        commit_file(repo, "l.txt", "x", "loose work");
        // Create `child` on top of `loose` (the current branch).
        create_branch(path, "child".to_string(), None).unwrap();

        let view = build_view(repo).unwrap();
        let loose_root = view
            .roots
            .iter()
            .find(|n| n.branch.name == "loose")
            .expect("untracked branch `loose` must be a visible root");
        assert!(
            loose_root.children.iter().any(|n| n.branch.name == "child"),
            "`child` stacked on an untracked branch must be visible under it"
        );
    }

    #[test]
    fn create_branch_off_current_works_with_dirty_tree() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        commit_file(repo, "f.txt", "v1", "add f");
        // Uncommitted change on the current branch (main).
        std::fs::write(repo.join("f.txt"), "dirty").unwrap();

        // Creating on top of the current branch must succeed despite the dirty tree.
        let view = create_branch(path, "feat".to_string(), None).unwrap();
        assert_eq!(view.current_branch.as_deref(), Some("feat"));
        assert!(view.roots[0].children.iter().any(|n| n.branch.name == "feat"));
    }

    #[test]
    fn create_branch_off_other_branch_when_dirty_errors_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        commit_file(repo, "f.txt", "v1", "add f");
        git_ok(repo, &["checkout", "-b", "other"]);
        commit_file(repo, "f.txt", "other-version", "diverge");
        git_ok(repo, &["checkout", "main"]);
        std::fs::write(repo.join("f.txt"), "dirty").unwrap(); // conflicts with `other`

        let result = create_branch(path, "x".to_string(), Some("other".to_string()));
        assert!(result.is_err(), "should refuse to switch with a dirty tree");
        let msg = format!("{:?}", result.err().unwrap());
        assert!(msg.contains("uncommitted"), "error should mention uncommitted changes");
    }

    #[test]
    fn set_parent_tracks_untracked_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "loose"]);
        commit(repo, "x");

        let view = set_parent(
            repo.to_string_lossy().to_string(),
            "loose".to_string(),
            "main".to_string(),
        )
        .unwrap();
        assert!(view.untracked.is_empty());
        let node = &view.roots[0].children[0];
        assert_eq!(node.branch.name, "loose");
        assert_eq!(node.branch.parent.as_deref(), Some("main"));
    }

    #[test]
    fn set_parent_rejects_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "a"]);
        commit(repo, "a1");
        git_ok(repo, &["config", "branch.a.gitstack-parent", "main"]);
        git_ok(repo, &["checkout", "-b", "b"]);
        commit(repo, "b1");
        git_ok(repo, &["config", "branch.b.gitstack-parent", "a"]);

        let result = set_parent(
            repo.to_string_lossy().to_string(),
            "a".to_string(),
            "b".to_string(),
        );
        assert!(result.is_err(), "expected cycle rejection");
    }

    fn write_file(repo: &Path, name: &str, content: &str) {
        std::fs::write(repo.join(name), content).unwrap();
    }
    fn commit_file(repo: &Path, name: &str, content: &str, msg: &str) {
        write_file(repo, name, content);
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", msg]);
    }

    #[test]
    fn restack_rebases_child_after_parent_moves() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();

        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "a.txt", "a1", "a1");

        git_ok(repo, &["checkout", "main"]);
        commit_file(repo, "main.txt", "M", "main2");

        let view = restack(path.clone(), None).unwrap();
        assert!(view.conflict.is_none(), "should restack cleanly");
        assert!(git::is_ancestor(repo, "main", "a"));
        let a = &view.roots[0].children[0];
        assert_eq!(a.branch.name, "a");
        assert_eq!(a.branch.behind, 0);
        assert_eq!(a.branch.ahead, 1);
    }

    #[test]
    fn restack_conflict_then_continue() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();

        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "conflict.txt", "from-a", "a-change");

        git_ok(repo, &["checkout", "main"]);
        commit_file(repo, "conflict.txt", "from-main", "main-change");

        let view = restack(path.clone(), None).unwrap();
        let conflict = view.conflict.expect("expected a conflict");
        assert_eq!(conflict.branch.as_deref(), Some("a"));
        assert!(conflict.files.iter().any(|f| f.ends_with("conflict.txt")));

        // Resolve and continue.
        write_file(repo, "conflict.txt", "resolved");
        git_ok(repo, &["add", "-A"]);
        let view2 = continue_restack(path.clone()).unwrap();
        assert!(view2.conflict.is_none(), "conflict should be resolved");
        let a = &view2.roots[0].children[0];
        assert_eq!(a.branch.name, "a");
        assert_eq!(a.branch.behind, 0);
    }

    #[test]
    fn branch_commits_lists_branch_only() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "f.txt", "1", "first");
        commit_file(repo, "f.txt", "2", "second");

        let commits = branch_commits(path, "a".to_string()).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "second");
        assert_eq!(commits[1].subject, "first");
    }

    #[test]
    fn stack_commits_labels_branch_tips() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "f.txt", "1", "c1");

        let nodes = stack_commits(path, None).unwrap();
        assert!(!nodes.is_empty());
        let tip = git::rev_parse(repo, "a").unwrap();
        let node = nodes.iter().find(|n| n.sha == tip).expect("tip in graph");
        assert!(node.refs.contains(&"a".to_string()));
        assert!(!node.parents.is_empty());
    }

    #[test]
    fn stack_commits_includes_untracked_branches() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        // `loose` has no gitstack metadata (untracked).
        git_ok(repo, &["checkout", "-b", "loose"]);
        commit_file(repo, "loose.txt", "x", "loose work");

        let nodes = stack_commits(path, None).unwrap();
        let tip = git::rev_parse(repo, "loose").unwrap();
        assert!(
            nodes
                .iter()
                .any(|n| n.sha == tip && n.refs.contains(&"loose".to_string())),
            "untracked branch should appear in the commit graph"
        );
    }

    #[test]
    fn stack_commits_filters_to_selected_branches() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        // Two independent feature branches off main.
        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "a.txt", "a", "a only");
        git_ok(repo, &["checkout", "main"]);
        create_branch(path.clone(), "b".to_string(), None).unwrap();
        commit_file(repo, "b.txt", "b", "b only");

        let a_tip = git::rev_parse(repo, "a").unwrap();
        let b_tip = git::rev_parse(repo, "b").unwrap();

        // Filtering to `a` shows a's commit but not b's.
        let only_a = stack_commits(path.clone(), Some(vec!["a".to_string()])).unwrap();
        assert!(only_a.iter().any(|n| n.sha == a_tip), "a's commit should show");
        assert!(!only_a.iter().any(|n| n.sha == b_tip), "b's commit must be hidden");

        // No filter shows both.
        let all = stack_commits(path, None).unwrap();
        assert!(all.iter().any(|n| n.sha == a_tip) && all.iter().any(|n| n.sha == b_tip));
    }

    #[test]
    fn repo_name_from_url_variants() {
        assert_eq!(repo_name_from_url("https://github.com/me/repo.git"), "repo");
        assert_eq!(repo_name_from_url("https://github.com/me/repo"), "repo");
        assert_eq!(repo_name_from_url("git@github.com:me/repo.git"), "repo");
        assert_eq!(repo_name_from_url("C:\\src\\my-proj"), "my-proj");
    }

    #[test]
    fn clone_repo_into_destination_offline() {
        // `git clone <local path>` works offline — clone a temp repo into another dir.
        let src = tempfile::tempdir().unwrap();
        init_repo(src.path());
        commit_file(src.path(), "f.txt", "hello", "add f");

        let dest = tempfile::tempdir().unwrap();
        let url = src.path().to_string_lossy().to_string();
        let view =
            clone_repo_blocking(&url, &dest.path().to_string_lossy()).expect("clone failed");

        // The clone lives under dest as a folder named after the source repo, and is
        // a real git repo with the cloned commit. (Avoid prefix comparison: temp dirs
        // may resolve through symlinks, so repo_root's canonical form can differ.)
        let cloned = Path::new(&view.repo_root);
        assert!(cloned.join(".git").exists(), "cloned dir is a git repo");
        assert!(cloned.join("f.txt").exists(), "cloned content present");
        assert_eq!(view.trunk, "main");

        // Cloning again into the same place must error clearly, not overwrite.
        let again = clone_repo_blocking(&url, &dest.path().to_string_lossy());
        assert!(again.is_err(), "second clone should refuse to overwrite");
    }

    // Real end-to-end against a cloned GitHub repo. Ignored by default (hits the
    // network and creates PRs); run with:
    //   GITUI_E2E_REPO=<path> cargo test e2e_submit_sandbox -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_submit_sandbox() {
        let path = match std::env::var("GITUI_E2E_REPO") {
            Ok(p) if !p.is_empty() => p,
            _ => {
                eprintln!("GITUI_E2E_REPO not set; skipping");
                return;
            }
        };
        let view =
            submit(path, None, false, std::collections::HashMap::new()).expect("submit should succeed");
        assert!(view.prs_available, "PRs should be available after submit");

        fn collect(n: &StackNode, out: &mut Vec<Branch>) {
            out.push(n.branch.clone());
            for c in &n.children {
                collect(c, out);
            }
        }
        let mut all = Vec::new();
        for r in &view.roots {
            collect(r, &mut all);
        }
        let with_pr = all.iter().filter(|b| b.tracked && b.pr.is_some()).count();
        eprintln!("submit ok — {} tracked branches now have PRs", with_pr);
        assert!(with_pr >= 1, "expected at least one PR after submit");
    }

    #[test]
    fn cleanup_merged_reparents_children_without_deleting() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "a.txt", "a", "a1");
        create_branch(path.clone(), "b".to_string(), None).unwrap(); // b on top of a
        commit_file(repo, "b.txt", "b", "b1");

        // Simulate that the PR for `a` was merged.
        let merged: std::collections::HashSet<String> = ["a".to_string()].into_iter().collect();
        crate::stack::cleanup_merged(repo, &merged, "main").unwrap();

        let view = build_view(repo).unwrap();
        assert!(git::branch_exists(repo, "a"), "branch `a` must NOT be deleted");
        assert!(
            view.roots.iter().any(|n| n.branch.name == "a"),
            "`a` should now be a forest root (untracked but still visible)"
        );
        let b = view.roots[0].children.iter().find(|n| n.branch.name == "b");
        assert_eq!(
            b.map(|n| n.branch.parent.as_deref()),
            Some(Some("main")),
            "`b` should be re-parented onto main"
        );
    }

    #[test]
    fn checkout_switches_current_branch() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "a".to_string(), None).unwrap(); // now on `a`
        let view = checkout(path, "main".to_string()).unwrap();
        assert_eq!(view.current_branch.as_deref(), Some("main"));
    }

    fn node_exists(nodes: &[StackNode], name: &str) -> bool {
        nodes
            .iter()
            .any(|n| n.branch.name == name || node_exists(&n.children, name))
    }

    // Real create-branch flow against a cloned repo (e.g. the sandbox on `experiment`).
    // Run with: GITUI_E2E_REPO=<path> cargo test e2e_create_branch_sandbox -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_create_branch_sandbox() {
        let path = match std::env::var("GITUI_E2E_REPO") {
            Ok(p) if !p.is_empty() => p,
            _ => {
                eprintln!("GITUI_E2E_REPO not set; skipping");
                return;
            }
        };
        let repo = std::path::Path::new(&path);
        let name = "gitui-selftest";
        let current = git::current_branch(repo).unwrap_or_default();
        eprintln!("current branch: {}", current);

        // Clean any leftover from a previous run.
        let _ = git::checkout(repo, &current);
        let _ = proc::run("git", ["branch", "-D", name], Some(repo));
        let _ = meta::unset_all(repo, name);

        // 1. Create on top of the current branch — must succeed and be visible.
        let view = create_branch(path.clone(), name.to_string(), None).expect("create failed");
        eprintln!("created; current now {:?}", view.current_branch);
        assert!(
            node_exists(&view.roots, name),
            "the new branch must be visible in the tree"
        );

        // 2. Creating it again must fail with a readable message.
        let again = create_branch(path.clone(), name.to_string(), None);
        assert!(again.is_err(), "duplicate create should error");
        let msg = again.err().unwrap().message;
        eprintln!("retry error (good): {}", msg);
        assert!(msg.contains("already exists"));

        // Cleanup: leave the sandbox as we found it.
        let _ = git::checkout(repo, &current);
        let _ = proc::run("git", ["branch", "-D", name], Some(repo));
        let _ = meta::unset_all(repo, name);
        eprintln!("cleaned up");
    }

    // End-to-end concrete scenarios against a FRESH repo (main + 1 commit, clean).
    // Run: GITUI_E2E_REPO=<fresh repo> cargo test e2e_scenarios_sandbox -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_scenarios_sandbox() {
        let path = match std::env::var("GITUI_E2E_REPO") {
            Ok(p) if !p.is_empty() => p,
            _ => {
                eprintln!("GITUI_E2E_REPO not set; skipping");
                return;
            }
        };
        let repo = std::path::Path::new(&path);

        // Build a stack: a on main, b on a.
        let v = create_branch(path.clone(), "a".to_string(), None).unwrap();
        assert!(node_exists(&v.roots, "a"), "a visible");
        commit_file(repo, "a.txt", "a1", "a1");
        create_branch(path.clone(), "b".to_string(), None).unwrap();
        commit_file(repo, "b.txt", "b1", "b1");
        eprintln!("stack built: main -> a -> b");

        // main advances, then restack the whole stack.
        git_ok(repo, &["checkout", "main"]);
        commit_file(repo, "main.txt", "m2", "main moves");
        let v = restack(path.clone(), None).unwrap();
        assert!(v.conflict.is_none(), "clean restack");
        assert!(git::is_ancestor(repo, "main", "a"), "a rebased onto new main");
        assert!(git::is_ancestor(repo, "a", "b"), "b rebased onto new a");
        eprintln!("restack OK: a and b rebased onto the new main");

        // Creating off a DIFFERENT branch with a dirty tree → clear error.
        git_ok(repo, &["checkout", "main"]);
        std::fs::write(repo.join("f.txt"), "dirty").unwrap();
        let err = create_branch(path.clone(), "c".to_string(), Some("a".to_string()));
        assert!(err.is_err(), "dirty + off-other should error");
        let msg = err.err().unwrap().message;
        eprintln!("dirty off-other error (good): {}", msg);
        assert!(msg.contains("uncommitted"));

        // Creating off the current branch still works even while dirty.
        let v = create_branch(path.clone(), "d".to_string(), None).unwrap();
        assert!(node_exists(&v.roots, "d"), "d created off current despite dirty tree");
        eprintln!("all scenarios passed");
    }

    // Hard case: a 3-level stack (main -> a -> b -> c). The trunk and `a` edit the
    // SAME line -> restack conflicts on `a`. After resolving, `b` (which edits a far
    // line) and `c` must rebase automatically — the cascade resumes on its own.
    #[test]
    fn restack_three_level_cascade_resumes_after_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        commit_file(repo, "shared.txt", "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n", "base");

        create_branch(path.clone(), "a".to_string(), None).unwrap();
        commit_file(repo, "shared.txt", "l1\nA2\nl3\nl4\nl5\nl6\nl7\nl8\n", "a edits l2");
        create_branch(path.clone(), "b".to_string(), None).unwrap();
        commit_file(repo, "shared.txt", "l1\nA2\nl3\nl4\nl5\nl6\nB7\nl8\n", "b edits l7");
        create_branch(path.clone(), "c".to_string(), None).unwrap();
        commit_file(repo, "c.txt", "c", "c adds file");

        // Trunk edits l2 too -> conflicts with `a`.
        git_ok(repo, &["checkout", "main"]);
        commit_file(repo, "shared.txt", "l1\nM2\nl3\nl4\nl5\nl6\nl7\nl8\n", "main edits l2");

        let v = restack(path.clone(), None).unwrap();
        assert_eq!(
            v.conflict.expect("conflict on a").branch.as_deref(),
            Some("a")
        );

        // Resolve l2 and continue; b (l7) and c are far away -> cascade completes.
        std::fs::write(repo.join("shared.txt"), "l1\nR2\nl3\nl4\nl5\nl6\nl7\nl8\n").unwrap();
        git_ok(repo, &["add", "-A"]);
        let v = continue_restack(path).unwrap();
        assert!(v.conflict.is_none(), "cascade should finish: {:?}", v.conflict);
        assert!(git::is_ancestor(repo, "main", "a"));
        assert!(git::is_ancestor(repo, "a", "b"));
        assert!(git::is_ancestor(repo, "b", "c"));
    }
}
