use crate::error::{AppError, Result};
use crate::model::{
    Branch, CheckRun, CommitDetail, CommitInfo, CommitNode, ConflictState, ConflictSuggestion,
    PrDescription, PrReview, RepoView, StackNode, StashEntry, StashFile, SubmitStepInfo,
};
use crate::{assist, git, github, links, meta, proc, stack};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Environment health: are `git` and `gh` available, and is `gh` authenticated?
/// (Claude Code is intentionally *not* probed here — it's checked only when an AI
/// feature is used, via `assist::ensure_claude_available`.)
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

/// Select the engine behind the `claude` CLI: `"anthropic"` (cloud, the user's own login)
/// or `"ollama"` (local models). Called by the frontend on startup and on every settings
/// change so the spawn funnels pick the right env vars.
#[tauri::command]
pub fn set_ai_backend(
    backend: String,
    ollama_host: String,
    ollama_model: String,
    anthropic_model: String,
) {
    assist::set_ai_config(&backend, ollama_host, ollama_model, anthropic_model);
}

/// List Ollama models for the picker. Two sources, merged & de-duplicated:
///   1. `~/.ollama/config.json` → `integrations.claude.models` — what `ollama launch
///      claude` uses, **including cloud models** (`*:cloud`) that `/api/tags` never lists.
///   2. `GET <host>/api/tags` — locally pulled models.
/// Done from Rust (no browser CORS) so detection works in the packaged app too.
#[tauri::command]
pub async fn ollama_models(host: String) -> Result<Vec<String>> {
    // Validate/normalize before any network use: this host becomes a `GET <host>/api/tags`
    // request, i.e. an SSRF sink. Rejects non-http(s) schemes and link-local/metadata IPs.
    let base = assist::validate_ollama_host(&host)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut names: Vec<String> = Vec::new();

        // 1) Models configured for the Claude Code integration (covers cloud models).
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let cfg = Path::new(&home).join(".ollama").join("config.json");
            if let Ok(text) = std::fs::read_to_string(&cfg) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(arr) = json
                        .pointer("/integrations/claude/models")
                        .and_then(|m| m.as_array())
                    {
                        names.extend(arr.iter().filter_map(|m| m.as_str().map(String::from)));
                    }
                }
            }
        }

        // 2) Locally pulled models from the running (validated) server.
        let url = format!("{base}/api/tags");
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(2))
            .timeout_read(std::time::Duration::from_secs(4))
            .build();
        let tags: Result<Vec<String>> = agent
            .get(&url)
            .call()
            .map_err(|e| {
                AppError::new(format!("Ollama injoignable sur {base} — est-il lancé ? ({e})"))
            })
            .and_then(|r| r.into_string().map_err(|e| AppError::new(e.to_string())))
            .and_then(|body| {
                serde_json::from_str::<serde_json::Value>(&body)
                    .map_err(|e| AppError::new(e.to_string()))
            })
            .map(|json| {
                json.get("models")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            });

        match tags {
            Ok(local) => names.extend(local),
            // Server unreachable is fine if the config already gave us (cloud) models.
            Err(e) => {
                if names.is_empty() {
                    return Err(e);
                }
            }
        }

        // De-duplicate, preserving order (config/cloud models first).
        let mut seen = std::collections::HashSet::new();
        names.retain(|n| seen.insert(n.clone()));
        Ok(names)
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
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
    // Reject a URL git would parse as an option (`--upload-pack=<cmd>`, `-c …`) — that is
    // argument injection / local command execution. The `--` separator below is the second
    // line of defense.
    if url.starts_with('-') {
        return Err(AppError::new(
            "URL de dépôt invalide (ne peut pas commencer par « - »).",
        ));
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
        // `-c protocol.ext.allow=never` blocks the `ext::<cmd>` transport (RCE vector); `--`
        // stops git from treating `url` / `target` as options.
        [
            "-c",
            "protocol.ext.allow=never",
            "clone",
            "--",
            url,
            target_str.as_str(),
        ],
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
    crate::undo::global().push(repo, "restack");
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
    bodies: HashMap<String, String>,
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
                let body = bodies
                    .get(&step.branch)
                    .map(|b| b.trim().to_string())
                    .filter(|b| !b.is_empty())
                    .unwrap_or_else(|| {
                        format!("Stacked PR — base `{}`. Managed by gitui.", step.base)
                    });
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

    crate::undo::global().push(repo, "sync");
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

/// Undo the last in-app history-rewriting command (restack / sync / commit edit) by
/// restoring the branch tips snapshotted just before it ran.
#[tauri::command]
pub fn undo(path: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    let snap = crate::undo::global()
        .pop(repo)
        .ok_or_else(|| AppError::new("Nothing to undo"))?;
    crate::undo::restore(repo, &snap)?;
    build_view(repo)
}

/// Label of what `undo` would restore next (e.g. "restack"), or null if nothing.
#[tauri::command]
pub fn undo_peek(path: String) -> Result<Option<String>> {
    let root = git::repo_root(Path::new(&path))?;
    Ok(crate::undo::global().peek_label(Path::new(&root)))
}

// ---- Stashes ----

/// Parse a `git stash` subject ("On <branch>: <msg>" / "WIP on <branch>: …") into
/// (branch, message).
fn parse_stash_subject(subject: &str) -> (String, String) {
    let (head, message) = match subject.split_once(": ") {
        Some((h, m)) => (h, m.to_string()),
        None => (subject, String::new()),
    };
    let branch = head
        .trim_start_matches("WIP on ")
        .trim_start_matches("On ")
        .trim_start_matches("on ")
        .trim()
        .to_string();
    (branch, message)
}

/// The files inside a stash (including untracked when present).
fn stash_files(repo: &Path, ref_name: &str) -> Vec<StashFile> {
    let raw = git::git(
        repo,
        &["stash", "show", "--include-untracked", "--name-status", ref_name],
    )
    .or_else(|_| git::git(repo, &["stash", "show", "--name-status", ref_name]))
    .unwrap_or_default();
    raw.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let status = it.next()?.trim().to_string();
            let path = it.last()?.trim().to_string(); // `.last()` => new name on renames
            if status.is_empty() || path.is_empty() {
                None
            } else {
                Some(StashFile { status, path })
            }
        })
        .collect()
}

fn build_stashes(repo: &Path) -> Result<Vec<StashEntry>> {
    let raw = git::git(repo, &["stash", "list", "--format=%gd%x1f%gs%x1f%cr"])?;
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\u{1f}');
        let ref_name = parts.next().unwrap_or("").to_string();
        let subject = parts.next().unwrap_or("");
        let date = parts.next().unwrap_or("").to_string();
        if ref_name.is_empty() {
            continue;
        }
        let (branch, message) = parse_stash_subject(subject);
        let files = stash_files(repo, &ref_name);
        out.push(StashEntry {
            index: i,
            ref_name,
            message,
            branch,
            date,
            files,
        });
    }
    Ok(out)
}

/// Reject anything that isn't a literal `stash@{N}` reference.
fn valid_stash_ref(r: &str) -> Result<()> {
    let ok = r.starts_with("stash@{")
        && r.ends_with('}')
        && r.len() > "stash@{}".len()
        && r["stash@{".len()..r.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(AppError::new("invalid stash ref"))
    }
}

/// List stashes with the files each one contains.
#[tauri::command]
pub fn list_stashes(path: String) -> Result<Vec<StashEntry>> {
    let root = git::repo_root(Path::new(&path))?;
    build_stashes(Path::new(&root))
}

/// Cheap count of stashes (for the toolbar badge) — one `git stash list`, no file detail.
#[tauri::command]
pub fn stash_count(path: String) -> Result<usize> {
    let root = git::repo_root(Path::new(&path))?;
    let raw = git::git(Path::new(&root), &["stash", "list"])?;
    Ok(raw.lines().filter(|l| !l.trim().is_empty()).count())
}

/// Stash the current changes (optionally with a message and the untracked files).
#[tauri::command]
pub fn stash_push(
    path: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<Vec<StashEntry>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let mut args: Vec<String> = vec!["stash".into(), "push".into()];
    if include_untracked {
        args.push("--include-untracked".into());
    }
    if let Some(m) = message.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        args.push("-m".into());
        args.push(m.to_string());
    }
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    git::git(repo, &argv)?;
    build_stashes(repo)
}

/// Apply a stash without removing it.
#[tauri::command]
pub fn stash_apply(path: String, ref_name: String) -> Result<Vec<StashEntry>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    valid_stash_ref(&ref_name)?;
    git::git(repo, &["stash", "apply", &ref_name])?;
    build_stashes(repo)
}

/// Apply a stash and drop it if it applied cleanly.
#[tauri::command]
pub fn stash_pop(path: String, ref_name: String) -> Result<Vec<StashEntry>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    valid_stash_ref(&ref_name)?;
    git::git(repo, &["stash", "pop", &ref_name])?;
    build_stashes(repo)
}

/// Delete a stash.
#[tauri::command]
pub fn stash_drop(path: String, ref_name: String) -> Result<Vec<StashEntry>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    valid_stash_ref(&ref_name)?;
    git::git(repo, &["stash", "drop", &ref_name])?;
    build_stashes(repo)
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

// --- Commit editing (programmatic interactive rebase) ---

/// The upstream a branch's own commits sit on: its recorded stack parent, or the
/// trunk. Editing the trunk's commits is intentionally not supported.
fn edit_base(repo: &Path, branch: &str) -> Result<String> {
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    if branch == trunk {
        return Err(AppError::new(
            "Commit editing is only available on stacked branches, not the trunk",
        ));
    }
    let metas = meta::all(repo);
    Ok(metas
        .get(branch)
        .and_then(|m| m.parent.clone())
        .unwrap_or(trunk))
}

/// Full commit SHAs in `base..branch`, oldest-first (rebase todo order).
fn branch_commit_shas(repo: &Path, base: &str, branch: &str) -> Result<Vec<String>> {
    let range = format!("{base}..{branch}");
    let out = git::git(repo, &["log", "--format=%H", "--reverse", &range])?;
    Ok(out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Resolve a (possibly short) sha to full form and ensure it belongs to `shas`.
fn resolve_on_branch(repo: &Path, shas: &[String], sha: &str) -> Result<String> {
    let full = git::git(repo, &["rev-parse", sha])?.trim().to_string();
    if !shas.iter().any(|s| s == &full) {
        return Err(AppError::new("That commit is not on this branch"));
    }
    Ok(full)
}

/// Run a prepared rebase todo on `branch`, then restack its descendants onto the
/// rewritten tip. A conflict mid-rebase surfaces via the usual ConflictState.
fn apply_edit(
    repo: &Path,
    base: &str,
    branch: &str,
    todo: &str,
    msg: Option<&str>,
) -> Result<RepoView> {
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before editing commits"));
    }
    let clean = git::rebase_edit(repo, base, branch, todo, msg)?;
    if clean {
        // Rewriting the branch moved its tip; restack the whole stack so any
        // descendants follow onto the new commits (no-op for already-based branches).
        stack::run(repo, None)?;
    }
    build_view(repo)
}

/// Replace a commit's message.
#[tauri::command]
pub fn reword_commit(
    path: String,
    branch: String,
    sha: String,
    message: String,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let base = edit_base(repo, &branch)?;
    let shas = branch_commit_shas(repo, &base, &branch)?;
    let full = resolve_on_branch(repo, &shas, &sha)?;
    let todo = shas
        .iter()
        .map(|s| format!("{} {}", if *s == full { "reword" } else { "pick" }, s))
        .collect::<Vec<_>>()
        .join("\n");
    crate::undo::global().push(repo, "reword commit");
    apply_edit(repo, &base, &branch, &todo, Some(&message))
}

/// Remove a commit from the branch.
#[tauri::command]
pub fn drop_commit(path: String, branch: String, sha: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let base = edit_base(repo, &branch)?;
    let shas = branch_commit_shas(repo, &base, &branch)?;
    if shas.len() <= 1 {
        return Err(AppError::new("Cannot drop the only commit on the branch"));
    }
    let full = resolve_on_branch(repo, &shas, &sha)?;
    let todo = shas
        .iter()
        .filter(|s| **s != full)
        .map(|s| format!("pick {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    crate::undo::global().push(repo, "drop commit");
    apply_edit(repo, &base, &branch, &todo, None)
}

/// Move a commit one step toward the tip ("up") or toward the base ("down").
/// The commit list is shown newest-first, so "up" means newer.
#[tauri::command]
pub fn move_commit(
    path: String,
    branch: String,
    sha: String,
    direction: String,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let base = edit_base(repo, &branch)?;
    let mut shas = branch_commit_shas(repo, &base, &branch)?;
    let full = resolve_on_branch(repo, &shas, &sha)?;
    let i = shas.iter().position(|s| *s == full).unwrap();
    match direction.as_str() {
        "up" => {
            if i + 1 >= shas.len() {
                return Err(AppError::new("Already the newest commit"));
            }
            shas.swap(i, i + 1);
        }
        "down" => {
            if i == 0 {
                return Err(AppError::new("Already the oldest commit"));
            }
            shas.swap(i, i - 1);
        }
        _ => return Err(AppError::new("Invalid move direction")),
    }
    let todo = shas
        .iter()
        .map(|s| format!("pick {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    crate::undo::global().push(repo, "move commit");
    apply_edit(repo, &base, &branch, &todo, None)
}

/// Squash a commit into the one before it (fixup — keeps the earlier message).
#[tauri::command]
pub fn squash_commit(path: String, branch: String, sha: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let base = edit_base(repo, &branch)?;
    let shas = branch_commit_shas(repo, &base, &branch)?;
    let full = resolve_on_branch(repo, &shas, &sha)?;
    if shas.first() == Some(&full) {
        return Err(AppError::new("No earlier commit to squash into"));
    }
    let todo = shas
        .iter()
        .map(|s| format!("{} {}", if *s == full { "fixup" } else { "pick" }, s))
        .collect::<Vec<_>>()
        .join("\n");
    crate::undo::global().push(repo, "squash commit");
    apply_edit(repo, &base, &branch, &todo, None)
}

/// Generate the diff used to split a commit: `git diff <parent> <sha>`, full and
/// uncolored, so the structured view shown to the user and the patch applied during the
/// split come from byte-identical text (hence identical line ids).
fn commit_split_diff_text(repo: &Path, sha: &str) -> Result<String> {
    let parent = format!("{sha}^");
    git::git(repo, &["diff", "--no-color", parent.as_str(), sha])
}

/// Per-call counter so concurrent splits never collide on the temp patch file
/// (process-global, like `git::EDIT_SEQ` for rebase todos).
static SPLIT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// While paused at an `edit` step, rewrite the target commit as two: soft-undo it, stage
/// exactly the selected lines (`patch`) for the first commit, then everything else for
/// the second. `git apply --recount` tolerates the rebuilt hunk headers.
fn split_paused_commit_lines(repo: &Path, patch: &str, msg1: &str, msg2: &str) -> Result<()> {
    // Undo the commit but keep all its changes (index reset to the parent, tree intact).
    git::git(repo, &["reset", "--mixed", "HEAD^"])?;

    // Stage only the selected lines, via a temp patch applied to the index.
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let n = SPLIT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let pf = dir.join(format!("gitui-split-{pid}-{n}.patch"));
    std::fs::write(&pf, patch).map_err(|e| AppError::new(format!("write patch: {e}")))?;
    let pf_arg = pf.to_string_lossy().to_string();
    let r = proc::run(
        "git",
        ["apply", "--cached", "--recount", "--whitespace=nowarn", pf_arg.as_str()],
        Some(repo),
    );
    let _ = std::fs::remove_file(&pf);
    let r = r.map_err(|e| AppError::new(e.to_string()))?;
    if !r.success {
        return Err(AppError::new(format!(
            "git apply (sélection) a échoué : {}",
            r.stderr.trim()
        )));
    }
    if !git::has_staged_changes(repo) {
        return Err(AppError::new("La sélection ne stage aucun changement."));
    }
    git::git(repo, &["commit", "-m", msg1])?;

    // Second commit: everything that remains in the working tree.
    git::git(repo, &["add", "-A"])?;
    if !git::has_staged_changes(repo) {
        return Err(AppError::new(
            "Rien ne reste pour le second commit — laisse au moins une ligne non cochée.",
        ));
    }
    git::git(repo, &["commit", "-m", msg2])?;
    Ok(())
}

/// The structured diff of a commit (files → hunks → lines, with stable ids on the add/del
/// lines of line-splittable files), for the split picker UI.
#[tauri::command]
pub fn split_diff(path: String, sha: String) -> Result<Vec<crate::diff::FileDiff>> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let text = commit_split_diff_text(repo, &sha)?;
    Ok(crate::diff::parse(&text))
}

/// Split one commit into two at the line level. `lines` are the ids (from `split_diff`)
/// of the changed lines that go into the FIRST (lower) commit with message `msg1`;
/// everything else — including binary / deleted files, which aren't line-splittable —
/// goes into a second (upper) commit with message `msg2`. The branch's newer commits and
/// any descendant branches are replayed onto the new tip.
#[tauri::command]
pub fn split_commit(
    path: String,
    branch: String,
    sha: String,
    lines: Vec<u32>,
    msg1: String,
    msg2: String,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before editing commits"));
    }
    let base = edit_base(repo, &branch)?;
    let shas = branch_commit_shas(repo, &base, &branch)?;
    let full = resolve_on_branch(repo, &shas, &sha)?;

    // Parse the commit's diff (identical text → identical ids as split_diff).
    let text = commit_split_diff_text(repo, &full)?;
    let files = crate::diff::parse(&text);
    if files.iter().any(|f| f.renamed) {
        return Err(AppError::new(
            "Ce commit contient un renommage — le découpage par lignes ne le gère pas encore.",
        ));
    }
    let universe = crate::diff::selectable_ids(&files);
    if universe.is_empty() {
        return Err(AppError::new(
            "Aucune ligne découpable dans ce commit (binaire / suppression de fichier).",
        ));
    }
    let selected: std::collections::HashSet<u32> =
        lines.into_iter().filter(|id| universe.contains(id)).collect();
    if selected.is_empty() {
        return Err(AppError::new(
            "Sélectionne au moins une ligne pour le premier commit.",
        ));
    }
    let msg1 = msg1.trim();
    let msg2 = msg2.trim();
    if msg1.is_empty() || msg2.is_empty() {
        return Err(AppError::new("Les deux messages de commit sont requis"));
    }

    let patch = crate::diff::build_partial_patch(&files, &selected);
    if patch.trim().is_empty() {
        return Err(AppError::new(
            "La sélection ne produit aucun changement à appliquer.",
        ));
    }

    crate::undo::global().push(repo, "split commit");

    // Mark the target `edit` (pauses the rebase on it); everything else is a plain pick.
    let todo = shas
        .iter()
        .map(|s| format!("{} {}", if *s == full { "edit" } else { "pick" }, s))
        .collect::<Vec<_>>()
        .join("\n");

    // Start the interactive rebase. An `edit` step makes git STOP on the target commit
    // and exit 0 (an intentional pause, not an error) — so detect the pause via
    // `rebase_in_progress`, not the exit code. A non-zero exit that leaves a rebase
    // running instead means an earlier pick hit a conflict.
    match git::rebase_edit(repo, &base, &branch, &todo, None)? {
        false => {
            let _ = git::rebase_abort(repo);
            return Err(AppError::new(
                "Conflit pendant la préparation du découpage — opération annulée.",
            ));
        }
        true => {
            if !git::rebase_in_progress(repo) {
                // Ran to completion without stopping — nothing was split.
                return Err(AppError::new("Le découpage n'a pas pu démarrer."));
            }
        }
    }

    // Do the split at the paused commit; abort the whole rebase on any failure so we
    // never leave a half-applied state behind.
    if let Err(e) = split_paused_commit_lines(repo, &patch, msg1, msg2) {
        let _ = git::rebase_abort(repo);
        return Err(e);
    }

    // Replay the branch's remaining (newer) commits onto the two new ones.
    let cont = git::rebase_continue(repo)?;
    if !cont.success {
        if git::rebase_in_progress(repo) {
            // A later commit conflicts — surface it like any restack conflict.
            return build_view(repo);
        }
        return Err(AppError::new(format!(
            "git rebase --continue failed: {}",
            cont.stderr.trim()
        )));
    }

    // Restack any child branches onto the rewritten branch tip.
    stack::run(repo, None)?;
    build_view(repo)
}

/// Cherry-pick a commit onto `target`, then return to the original branch. On conflict
/// the cherry-pick is aborted and the original branch restored (no half-applied state).
#[tauri::command]
pub fn cherry_pick(path: String, sha: String, target: String) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    if git::rebase_in_progress(repo) {
        return Err(AppError::new("Finish the in-progress restack first"));
    }
    if git::is_dirty(repo) {
        return Err(AppError::new("Commit or stash your changes before cherry-picking"));
    }
    if !git::branch_exists(repo, &target) {
        return Err(AppError::new(format!("Branch '{}' does not exist", target)));
    }
    let original = git::current_branch(repo);
    crate::undo::global().push(repo, "cherry-pick");
    git::checkout(repo, &target)?;
    let result = git::git(repo, &["cherry-pick", &sha]);
    // Whatever happens, go back to the branch the user was on.
    if let Some(orig) = &original {
        if result.is_err() {
            let _ = git::git(repo, &["cherry-pick", "--abort"]);
        }
        let _ = git::checkout(repo, orig);
    }
    match result {
        Ok(_) => build_view(repo),
        Err(e) => Err(AppError::new(format!(
            "Cherry-pick sur '{}' échoué (conflits ?). Annulé. {}",
            target, e
        ))),
    }
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

/// Submit a human review on a PR (approve / request_changes / comment).
#[tauri::command]
pub fn submit_pr_review(
    path: String,
    number: u64,
    event: String,
    body: String,
) -> Result<crate::model::PrDetail> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    github::pr_review(repo, number, &event, &body)?;
    github::pr_detail(repo, number)
}

/// Emoji prefix for a finding severity (visual punch in the posted comments).
fn severity_emoji(sev: &str) -> &'static str {
    match sev {
        "critical" => "🔴",
        "warning" => "🟡",
        "info" => "🔵",
        _ => "⚪",
    }
}

/// Post the findings of an AI review onto the PR as a single GitHub review (event
/// `COMMENT` — never auto-approves / requests changes; the human decides). Findings that
/// pin a file + line become inline comments; the `summary` and any line-less findings go
/// in the review body. If GitHub rejects the inline positions (a line not in the diff
/// fails the whole review), we fall back to a summary-only comment that folds the findings
/// into the body, so nothing is lost. Returns a short French status for the UI toast.
#[tauri::command]
pub fn post_review_comments(
    path: String,
    number: u64,
    summary: String,
    findings: Vec<crate::model::PrFinding>,
) -> Result<String> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);

    // Partition findings: those with a file + line can be pinned inline; the rest are
    // listed in the review body.
    let mut inline: Vec<github::InlineComment> = Vec::new();
    let mut leftover: Vec<&crate::model::PrFinding> = Vec::new();
    for f in &findings {
        let has_pos = !f.file.trim().is_empty() && f.line.map(|l| l > 0).unwrap_or(false);
        if has_pos {
            let title = if f.title.trim().is_empty() { "(sans titre)" } else { f.title.trim() };
            let body = format!(
                "{} **{}** — {}\n\n{}",
                severity_emoji(&f.severity),
                f.severity,
                title,
                f.detail.trim()
            );
            inline.push(github::InlineComment {
                path: f.file.trim().to_string(),
                line: f.line.unwrap(),
                body,
            });
        } else if !f.title.trim().is_empty() || !f.detail.trim().is_empty() {
            leftover.push(f);
        }
    }

    // Build the review body: the summary, plus any findings that couldn't be pinned.
    let mut body = String::new();
    if !summary.trim().is_empty() {
        body.push_str(summary.trim());
    }
    if !leftover.is_empty() {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str("**Autres remarques :**\n");
        for f in &leftover {
            let title = if f.title.trim().is_empty() { f.detail.trim() } else { f.title.trim() };
            let loc = if f.file.trim().is_empty() {
                String::new()
            } else {
                format!(" (`{}`)", f.file.trim())
            };
            body.push_str(&format!("- {} {}{}\n", severity_emoji(&f.severity), title, loc));
        }
    }
    if body.trim().is_empty() {
        body.push_str("Relecture IA (via gitui).");
    }
    let body = format!("{}\n\n— 🤖 Relecture IA via gitui", body.trim());

    // No inline positions at all → just post the summary as a COMMENT review.
    if inline.is_empty() {
        github::pr_review(repo, number, "comment", &body)?;
        return Ok("Aucune ligne à épingler — résumé posté en commentaire.".to_string());
    }

    let n_inline = inline.len();
    match github::pr_review_comments(repo, number, &body, &inline) {
        Ok(()) => Ok(format!(
            "Review postée : {n_inline} commentaire(s) en ligne + résumé."
        )),
        Err(_) => {
            // GitHub rejected one or more positions; fold the inline findings into the
            // body and post a plain summary review so the relecture isn't lost.
            let mut full = body.clone();
            full.push_str("\n\n**Commentaires (positions non rattachables au diff) :**\n");
            for c in &inline {
                full.push_str(&format!("- `{}:{}` — {}\n", c.path, c.line, c.body.replace('\n', " ")));
            }
            github::pr_review(repo, number, "comment", &full)?;
            Ok(format!(
                "Positions en ligne refusées par GitHub — résumé + {n_inline} remarque(s) postés en commentaire."
            ))
        }
    }
}

/// The individual CI checks for a PR (name, bucket, link to logs).
#[tauri::command]
pub fn pr_checks(path: String, number: u64) -> Result<Vec<CheckRun>> {
    let root = git::repo_root(Path::new(&path))?;
    github::pr_checks(Path::new(&root), number)
}

/// AI review of a PR: feed the unified diff to `claude` and return structured findings.
/// Async (off the main thread via `spawn_blocking`) — the model call takes tens of seconds.
#[tauri::command]
pub async fn review_pr(path: String, number: u64) -> Result<PrReview> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PrReview> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let detail = github::pr_detail(repo, number)?;
        let out = assist::run_claude_headless(repo, &assist::pr_review_prompt(&detail))?;
        let json = assist::extract_json(&out)?;
        serde_json::from_str::<PrReview>(json)
            .map_err(|e| AppError::new(format!("bad review JSON: {e}")))
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

/// Generate a commit message for `sha` via `claude`. `mode` is "simple" (≤5 words) or
/// "complet" (subject + body); the message starts with a conventional-commit type.
#[tauri::command]
pub async fn generate_commit_message(path: String, sha: String, mode: String) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let out =
            assist::run_claude_headless(repo, &assist::commit_message_prompt(&sha, &mode))?;
        let json = assist::extract_json(&out)?;
        #[derive(serde::Deserialize)]
        struct Msg {
            #[serde(alias = "commit_message", alias = "text")]
            message: String,
        }
        let m: Msg = serde_json::from_str(json)
            .map_err(|e| AppError::new(format!("bad message JSON: {e}")))?;
        Ok(m.message.trim().to_string())
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

/// AI review of a single commit: structured findings (same shape as `review_pr`).
#[tauri::command]
pub async fn review_commit(path: String, sha: String) -> Result<PrReview> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PrReview> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let detail = git::commit_detail(repo, &sha)?;
        // Cap the diff (char-boundary safe) to keep the prompt bounded.
        let diff: String = detail.diff.chars().take(60_000).collect();
        let prompt = assist::commit_review_prompt(&sha, &detail.message, &diff);
        let out = assist::run_claude_headless(repo, &prompt)?;
        let json = assist::extract_json(&out)?;
        serde_json::from_str::<PrReview>(json)
            .map_err(|e| AppError::new(format!("bad review JSON: {e}")))
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

/// Suggest a branch name from the current changes (or the last commit when clean).
#[tauri::command]
pub async fn suggest_branch_name(path: String) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let stat = git::git(repo, &["diff", "HEAD", "--stat"]).unwrap_or_default();
        let context = if stat.trim().is_empty() {
            let subj = git::git(repo, &["log", "-1", "--format=%s"]).unwrap_or_default();
            format!("Aucun changement non commité. Dernier commit : {}", subj.trim())
        } else {
            format!("Changements en cours :\n{}", stat.trim())
        };
        let out = assist::run_claude_headless(repo, &assist::branch_name_prompt(&context))?;
        let json = assist::extract_json(&out)?;
        #[derive(serde::Deserialize)]
        struct N {
            name: String,
        }
        let n: N = serde_json::from_str(json)
            .map_err(|e| AppError::new(format!("bad name JSON: {e}")))?;
        Ok(n.name.trim().to_string())
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

/// Draft a PR title + Markdown body for a branch from its commits/diff vs its base.
#[tauri::command]
pub async fn generate_pr_description(path: String, branch: String) -> Result<PrDescription> {
    tauri::async_runtime::spawn_blocking(move || -> Result<PrDescription> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let metas = meta::all(repo);
        let raw = git::local_branches(repo)?;
        let trunk = git::trunk(repo, &raw);
        let base = metas
            .get(&branch)
            .and_then(|m| m.parent.clone())
            .unwrap_or(trunk);
        let range = format!("{base}..{branch}");
        let commits = git::git(repo, &["log", "--format=- %s", &range]).unwrap_or_default();
        let stat = git::git(repo, &["diff", "--stat", &range]).unwrap_or_default();
        let prompt =
            assist::pr_description_prompt(&branch, &base, commits.trim(), stat.trim());
        let out = assist::run_claude_headless(repo, &prompt)?;
        let json = assist::extract_json(&out)?;
        #[derive(serde::Deserialize)]
        struct D {
            #[serde(default)]
            title: String,
            #[serde(default)]
            body: String,
        }
        let d: D = serde_json::from_str(json)
            .map_err(|e| AppError::new(format!("bad description JSON: {e}")))?;
        Ok(PrDescription {
            title: d.title.trim().to_string(),
            body: d.body.trim().to_string(),
        })
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

#[derive(serde::Deserialize)]
struct ConflictRaw {
    #[serde(default)]
    explanation: String,
    // Required (no default): a missing/renamed resolution must error rather than
    // silently wipe the file. Accept the common synonyms the model may emit.
    #[serde(alias = "merged", alias = "content", alias = "resolved")]
    resolution: String,
}

/// AI assistance for one conflicted file: send its content (with markers) plus the
/// base/ours/theirs versions to `claude` and return a proposed full-file resolution.
#[tauri::command]
pub async fn suggest_conflict_resolution(
    path: String,
    file: String,
) -> Result<ConflictSuggestion> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ConflictSuggestion> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let marked = git::read_working_file(repo, &file)?;
        let (base, ours, theirs) = git::conflict_versions(repo, &file);
        let prompt = assist::conflict_resolution_prompt(
            &file,
            &marked,
            base.as_deref(),
            ours.as_deref(),
            theirs.as_deref(),
        );
        let out = assist::run_claude_headless(repo, &prompt)?;
        let json = assist::extract_json(&out)?;
        let raw: ConflictRaw = serde_json::from_str(json)
            .map_err(|e| AppError::new(format!("bad resolution JSON: {e}")))?;
        Ok(ConflictSuggestion {
            file,
            explanation: raw.explanation,
            resolution: raw.resolution,
        })
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
}

/// Write an AI-resolved file back to the working tree, stage it (so it drops out of
/// the conflicted set), and return the refreshed view.
#[tauri::command]
pub fn apply_conflict_resolution(
    path: String,
    file: String,
    content: String,
) -> Result<RepoView> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    git::write_working_file(repo, &file, &content)?;
    git::stage_file(repo, &file)?;
    build_view(repo)
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

/// Summarize a repo's pending updates into a short French digest via `claude`.
/// Items are passed in (already fetched by the frontend) so no extra git/gh work is done.
#[tauri::command]
pub async fn summarize_updates(
    path: String,
    items: Vec<crate::notify::UpdateItem>,
) -> Result<String> {
    if items.is_empty() {
        return Ok(String::new());
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<String> {
        let root = git::repo_root(Path::new(&path))?;
        let repo = Path::new(&root);
        let lines: Vec<String> = items
            .iter()
            .map(|i| {
                let label = match i.kind.as_str() {
                    "pr" => format!("PR #{}", i.number.unwrap_or(0)),
                    "issue" => format!("Issue #{}", i.number.unwrap_or(0)),
                    "trunk" => "Tronc".to_string(),
                    other => other.to_string(),
                };
                format!("- [{}] {} — {}", label, i.title, i.detail)
            })
            .collect();
        let prompt = format!(
            "Voici les nouveautés d'un dépôt git depuis la dernière visite de l'utilisateur :\n{}\n\n\
             Rédige un DIGEST en français, 2 à 4 lignes MAXIMUM, factuel et utile. Regroupe par thème \
             si pertinent (PRs, issues, tronc) et mets en avant ce qui demande une action (CI en échec, \
             review demandée, PR mergée). Réponds DIRECTEMENT par le digest — pas d'introduction, pas de \
             titres Markdown, pas de bloc de code — et N'EXPLORE PAS le dépôt.",
            lines.join("\n")
        );
        let out = assist::run_claude_headless(repo, &prompt)?;
        Ok(out.trim().to_string())
    })
    .await
    .map_err(|e| AppError::new(e.to_string()))?
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

    // --- commit-editing helpers ---
    fn edit_subjects(repo: &Path, base: &str, branch: &str) -> Vec<String> {
        git::git(repo, &["log", "--format=%s", &format!("{base}..{branch}"), "--reverse"])
            .unwrap()
            .lines()
            .map(|s| s.to_string())
            .collect()
    }
    fn nth_sha(repo: &Path, base: &str, branch: &str, idx: usize) -> String {
        git::git(repo, &["log", "--format=%H", &format!("{base}..{branch}"), "--reverse"])
            .unwrap()
            .lines()
            .nth(idx)
            .unwrap()
            .trim()
            .to_string()
    }
    /// A `feat` branch (parent main) carrying empty commits with the given messages.
    fn feat_with(repo: &Path, msgs: &[&str]) -> String {
        git_ok(repo, &["checkout", "-b", "feat"]);
        for m in msgs {
            commit(repo, m);
        }
        git_ok(repo, &["config", "branch.feat.gitstack-parent", "main"]);
        repo.to_string_lossy().to_string()
    }

    #[test]
    fn reword_changes_only_the_target_message() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["c1", "c2"]);
        let c1 = nth_sha(repo, "main", "feat", 0);

        reword_commit(path, "feat".into(), c1, "c1 reworded".into()).unwrap();

        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["c1 reworded", "c2"]);
    }

    #[test]
    fn drop_removes_the_target_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["c1", "c2", "c3"]);
        let c2 = nth_sha(repo, "main", "feat", 1);

        drop_commit(path, "feat".into(), c2).unwrap();

        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["c1", "c3"]);
    }

    #[test]
    fn drop_refuses_the_only_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["only"]);
        let sha = nth_sha(repo, "main", "feat", 0);

        assert!(drop_commit(path, "feat".into(), sha).is_err());
    }

    #[test]
    fn move_up_makes_a_commit_newer() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["c1", "c2"]);
        let c1 = nth_sha(repo, "main", "feat", 0);

        // c1 is oldest; "up" = newer → order becomes c2, c1.
        move_commit(path, "feat".into(), c1, "up".into()).unwrap();

        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["c2", "c1"]);
    }

    #[test]
    fn squash_fixups_into_previous() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["c1", "c2"]);
        let c2 = nth_sha(repo, "main", "feat", 1);

        squash_commit(path, "feat".into(), c2).unwrap();

        // fixup keeps the earlier message and collapses to a single commit.
        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["c1"]);
    }

    #[test]
    fn squash_refuses_the_oldest_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = feat_with(repo, &["c1", "c2"]);
        let c1 = nth_sha(repo, "main", "feat", 0);

        assert!(squash_commit(path, "feat".into(), c1).is_err());
    }

    /// Selectable line ids belonging to `file` in commit `sha`'s diff (test helper).
    fn split_ids_for(repo: &Path, sha: &str, file: &str) -> Vec<u32> {
        let text = commit_split_diff_text(repo, sha).unwrap();
        let mut out = Vec::new();
        for f in crate::diff::parse(&text) {
            if f.path == file && f.selectable {
                for h in &f.hunks {
                    for l in &h.lines {
                        if let Some(id) = l.id {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out
    }

    #[test]
    fn split_partitions_by_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "feat".to_string(), None).unwrap();
        // One commit touching two files.
        write_file(repo, "a.txt", "a\n");
        write_file(repo, "b.txt", "b\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "both files"]);
        let sha = nth_sha(repo, "main", "feat", 0);

        // Select every line of a.txt → it lands in commit 1, b.txt in commit 2.
        let ids = split_ids_for(repo, &sha, "a.txt");
        assert!(!ids.is_empty());
        let view =
            split_commit(path, "feat".into(), sha, ids, "only a".into(), "only b".into()).unwrap();
        assert!(view.conflict.is_none(), "split should be clean");

        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["only a", "only b"]);

        let c0 = nth_sha(repo, "main", "feat", 0);
        let f0 = git::git(repo, &["show", "--name-only", "--format=", &c0]).unwrap();
        assert!(f0.contains("a.txt") && !f0.contains("b.txt"), "c0 = {f0}");
        let c1 = nth_sha(repo, "main", "feat", 1);
        let f1 = git::git(repo, &["show", "--name-only", "--format=", &c1]).unwrap();
        assert!(f1.contains("b.txt") && !f1.contains("a.txt"), "c1 = {f1}");
    }

    #[test]
    fn split_by_lines_within_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "feat".to_string(), None).unwrap();
        git_ok(repo, &["config", "core.autocrlf", "false"]); // deterministic blob content
        // A new 4-line file in a single commit.
        write_file(repo, "g.txt", "g1\ng2\ng3\ng4\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "add g (4 lines)"]);
        let sha = nth_sha(repo, "main", "feat", 0);

        // ids 0..3 = g1..g4; put g1 and g3 in the first commit.
        let ids = split_ids_for(repo, &sha, "g.txt");
        assert_eq!(ids.len(), 4, "four added lines");
        let first = vec![ids[0], ids[2]];

        let view =
            split_commit(path, "feat".into(), sha, first, "g1+g3".into(), "g2+g4".into()).unwrap();
        assert!(view.conflict.is_none(), "line split should be clean");
        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["g1+g3", "g2+g4"]);

        // Commit 1 created g.txt with ONLY the two selected lines…
        let c0 = nth_sha(repo, "main", "feat", 0);
        let v0 = git::git(repo, &["show", &format!("{c0}:g.txt")]).unwrap();
        assert_eq!(v0, "g1\ng3\n", "first commit holds only the selected lines");
        // …and the branch tip still has the whole file.
        let tip = git::git(repo, &["show", "feat:g.txt"]).unwrap();
        assert_eq!(tip, "g1\ng2\ng3\ng4\n");
    }

    #[test]
    fn split_modification_with_context_lines() {
        // Exercises `git apply --cached` of a rewritten hunk that mixes context, a kept
        // change, and a deferred change (the realistic line-split case).
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        git_ok(repo, &["config", "core.autocrlf", "false"]);
        write_file(repo, "f.txt", "l1\nl2\nl3\nl4\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "base f"]);
        create_branch(path.clone(), "feat".to_string(), None).unwrap();
        // One commit edits two distinct lines (l2 and l4), with context around them.
        write_file(repo, "f.txt", "l1\nL2\nl3\nL4\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "edit l2 and l4"]);
        let sha = nth_sha(repo, "main", "feat", 0);

        // Select only the l2→L2 change (its deletion + addition).
        let parsed = crate::diff::parse(&commit_split_diff_text(repo, &sha).unwrap());
        let mut sel = Vec::new();
        for f in &parsed {
            for h in &f.hunks {
                for l in &h.lines {
                    if let Some(id) = l.id {
                        if l.text == "l2" || l.text == "L2" {
                            sel.push(id);
                        }
                    }
                }
            }
        }
        assert_eq!(sel.len(), 2, "the l2→L2 change is one deletion + one addition");

        split_commit(path, "feat".into(), sha, sel, "edit l2".into(), "edit l4".into()).unwrap();
        assert_eq!(edit_subjects(repo, "main", "feat"), vec!["edit l2", "edit l4"]);

        // Commit 1 applied only the l2 edit; l4 is still deferred to commit 2.
        let c0 = nth_sha(repo, "main", "feat", 0);
        let v0 = git::git(repo, &["show", &format!("{c0}:f.txt")]).unwrap();
        assert_eq!(v0, "l1\nL2\nl3\nl4\n");
        let tip = git::git(repo, &["show", "feat:f.txt"]).unwrap();
        assert_eq!(tip, "l1\nL2\nl3\nL4\n");
    }

    #[test]
    fn split_refuses_empty_and_full_selection() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "feat".to_string(), None).unwrap();
        write_file(repo, "g.txt", "g1\ng2\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "add g"]);
        let sha = nth_sha(repo, "main", "feat", 0);
        let ids = split_ids_for(repo, &sha, "g.txt");

        // Nothing selected → first commit would be empty.
        assert!(split_commit(path.clone(), "feat".into(), sha.clone(), vec![], "m1".into(), "m2".into()).is_err());
        // Everything selected → second commit would be empty.
        assert!(split_commit(path.clone(), "feat".into(), sha, ids, "m1".into(), "m2".into()).is_err());
        // A refused split must not leave a rebase paused.
        assert!(!git::rebase_in_progress(repo));
    }

    #[test]
    fn split_replays_newer_commits() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        let path = repo.to_string_lossy().to_string();
        create_branch(path.clone(), "feat".to_string(), None).unwrap();
        // c1 touches a.txt + b.txt; c2 (newer) touches c.txt.
        write_file(repo, "a.txt", "a\n");
        write_file(repo, "b.txt", "b\n");
        git_ok(repo, &["add", "-A"]);
        git_ok(repo, &["commit", "-m", "c1 both"]);
        commit_file(repo, "c.txt", "c\n", "c2");
        let c1 = nth_sha(repo, "main", "feat", 0);

        let ids = split_ids_for(repo, &c1, "a.txt");
        split_commit(path, "feat".into(), c1, ids, "a only".into(), "b only".into()).unwrap();

        // The split happens in place; the newer commit is replayed on top.
        assert_eq!(
            edit_subjects(repo, "main", "feat"),
            vec!["a only", "b only", "c2"]
        );
    }

    #[test]
    fn editing_a_parent_restacks_its_child() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "feat-a"]);
        commit_file(repo, "a.txt", "a1\n", "a1");
        git_ok(repo, &["config", "branch.feat-a.gitstack-parent", "main"]);
        git_ok(repo, &["checkout", "-b", "feat-b"]);
        commit_file(repo, "b.txt", "b1\n", "b1");
        git_ok(repo, &["config", "branch.feat-b.gitstack-parent", "feat-a"]);
        let path = repo.to_string_lossy().to_string();

        let a1 = nth_sha(repo, "main", "feat-a", 0);
        reword_commit(path, "feat-a".into(), a1, "a1 reworded".into()).unwrap();

        // The child must have followed the rewritten parent: still 1 ahead, 0 behind.
        let view = build_view(repo).unwrap();
        let a = &view.roots[0].children[0];
        assert_eq!(a.branch.name, "feat-a");
        let b = &a.children[0];
        assert_eq!(b.branch.name, "feat-b");
        assert_eq!(b.branch.behind, 0, "child should be restacked onto the new parent tip");
        assert_eq!(b.branch.ahead, 1);
    }

    #[test]
    fn reordering_conflicting_commits_surfaces_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "feat"]);
        commit_file(repo, "f.txt", "A\n", "c1 adds A");
        commit_file(repo, "f.txt", "B\n", "c2 sets B");
        git_ok(repo, &["config", "branch.feat.gitstack-parent", "main"]);
        let path = repo.to_string_lossy().to_string();
        let c1 = nth_sha(repo, "main", "feat", 0);

        // Moving c1 after c2 replays both onto base and collides on f.txt.
        let view = move_commit(path, "feat".into(), c1, "up".into()).unwrap();
        assert!(view.conflict.is_some(), "expected a conflict to surface via ConflictState");

        // The existing abort flow must clean it up.
        abort_restack(repo.to_string_lossy().to_string()).unwrap();
        assert!(!git::rebase_in_progress(repo));
    }

    #[test]
    fn pr_review_parses_drifted_llm_keys() {
        // Observed real claude output: array labeled `issues`, fields path/message/description.
        let json = r#"{"summary":"s","issues":[{"path":"a.js","line":3,"severity":"warning","message":"off-by-one","description":"loop uses <="}]}"#;
        let r: crate::model::PrReview = serde_json::from_str(json).unwrap();
        assert_eq!(r.summary, "s");
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].file, "a.js");
        assert_eq!(r.findings[0].line, Some(3));
        assert_eq!(r.findings[0].title, "off-by-one");
        assert_eq!(r.findings[0].detail, "loop uses <=");
    }

    #[test]
    fn conflict_resolution_parses_synonym_keys() {
        let json = r#"{"explanation":"e","merged":"final content"}"#;
        let r: ConflictRaw = serde_json::from_str(json).unwrap();
        assert_eq!(r.explanation, "e");
        assert_eq!(r.resolution, "final content");
    }

    #[test]
    fn parse_stash_subject_extracts_branch_and_message() {
        assert_eq!(
            parse_stash_subject("On main: mon travail"),
            ("main".to_string(), "mon travail".to_string())
        );
        assert_eq!(
            parse_stash_subject("WIP on feat/x: abc123 sujet"),
            ("feat/x".to_string(), "abc123 sujet".to_string())
        );
    }

    #[test]
    fn valid_stash_ref_rejects_non_stash_refs() {
        assert!(valid_stash_ref("stash@{0}").is_ok());
        assert!(valid_stash_ref("stash@{12}").is_ok());
        assert!(valid_stash_ref("stash@{}").is_err());
        assert!(valid_stash_ref("HEAD").is_err());
        assert!(valid_stash_ref("stash@{0}; rm -rf /").is_err());
    }

    // End-to-end: hits the REAL claude CLI + gh against the local sandbox PR #3.
    // Run explicitly: cargo test --lib e2e_review_sandbox_pr -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_review_sandbox_pr() {
        let path = r"C:\Users\coren\Documents\projet\gitui-sandbox".to_string();
        let review = tauri::async_runtime::block_on(review_pr(path, 3)).expect("review_pr");
        eprintln!("summary: {}", review.summary);
        for f in &review.findings {
            eprintln!("[{}] {}:{:?} — {}", f.severity, f.file, f.line, f.title);
        }
        assert!(!review.summary.is_empty(), "expected a non-empty summary");
    }

    // End-to-end: posts a hand-crafted AI review onto the local sandbox PR #3 via the
    // real `gh api .../reviews` contract (no claude call). utils.js:4 is in the diff, so
    // the inline comment should attach; the line-less finding lands in the body.
    // Run explicitly: cargo test --lib e2e_post_review_comments_sandbox -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_post_review_comments_sandbox() {
        use crate::model::PrFinding;
        let path = r"C:\Users\coren\Documents\projet\gitui-sandbox".to_string();
        let findings = vec![
            PrFinding {
                file: "utils.js".into(),
                line: Some(4),
                severity: "critical".into(),
                title: "Off-by-one dans la boucle".into(),
                detail: "`i <= nums.length` lit un index hors borne — utilise `<`.".into(),
            },
            PrFinding {
                file: String::new(),
                line: None,
                severity: "info".into(),
                title: "Aucun test pour average()".into(),
                detail: "Ajouter un test unitaire couvrant la liste vide.".into(),
            },
        ];
        let status =
            post_review_comments(path, 3, "Relecture de test (gitui e2e).".into(), findings)
                .expect("post_review_comments");
        eprintln!("status: {status}");
        assert!(!status.is_empty());
    }

    // End-to-end: real claude resolving a real conflict, then applying it.
    // Run explicitly: cargo test --lib e2e_suggest_and_apply_conflict -- --ignored --nocapture
    #[test]
    #[ignore]
    fn e2e_suggest_and_apply_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo(repo);
        git_ok(repo, &["checkout", "-b", "feat"]);
        commit_file(repo, "config.json", "{\n  \"port\": 3000\n}\n", "feat sets 3000");
        git_ok(repo, &["config", "branch.feat.gitstack-parent", "main"]);
        git_ok(repo, &["checkout", "main"]);
        commit_file(repo, "config.json", "{\n  \"port\": 8080\n}\n", "main sets 8080");
        git_ok(repo, &["checkout", "feat"]);

        let path = repo.to_string_lossy().to_string();
        let view = restack(path.clone(), None).unwrap();
        let file = view.conflict.expect("expected conflict").files[0].clone();

        let sugg = tauri::async_runtime::block_on(suggest_conflict_resolution(
            path.clone(),
            file.clone(),
        ))
        .expect("suggest");
        eprintln!("explanation: {}\nresolution:\n{}", sugg.explanation, sugg.resolution);
        assert!(!sugg.resolution.is_empty());
        assert!(!sugg.resolution.contains("<<<<<<<"));

        let view2 = apply_conflict_resolution(path, file.clone(), sugg.resolution).unwrap();
        let still = view2.conflict.map(|c| c.files.contains(&file)).unwrap_or(false);
        assert!(!still, "file should be staged and out of the conflict list");
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
        let view = submit(
            path,
            None,
            false,
            std::collections::HashMap::new(),
            std::collections::HashMap::new(),
        )
        .expect("submit should succeed");
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
