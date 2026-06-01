use crate::error::{AppError, Result};
use crate::{git, meta};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

/// Build a parent -> children adjacency map from branch metadata.
fn children_map(metas: &HashMap<String, meta::Meta>) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (name, m) in metas {
        if let Some(parent) = &m.parent {
            children.entry(parent.clone()).or_default().push(name.clone());
        }
    }
    children
}

/// Branches in `start`'s subtree, parents before children (BFS).
fn subtree_order(
    children: &HashMap<String, Vec<String>>,
    start: &str,
    include_start: bool,
) -> Vec<String> {
    let mut order = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(start.to_string());
    if include_start {
        order.push(start.to_string());
    }
    while let Some(node) = queue.pop_front() {
        if let Some(kids) = children.get(&node) {
            let mut kids = kids.clone();
            kids.sort();
            for k in kids {
                order.push(k.clone());
                queue.push_back(k);
            }
        }
    }
    order
}

/// Rebase each branch in `order` onto the current tip of its parent.
/// `Ok(true)` = the whole pass completed, `Ok(false)` = paused on a conflict.
///
/// `metas` is read once up front: each branch's recorded base is the *old* parent
/// tip, which is exactly the `<oldbase>` needed by `git rebase --onto`. Because we
/// process parents before children, by the time a child is rebased its parent has
/// already moved to its new tip, and the child's still-recorded base points at the
/// parent's pre-rebase tip — so no commits are duplicated or dropped.
fn run_pass(
    repo: &Path,
    order: &[String],
    metas: &HashMap<String, meta::Meta>,
) -> Result<bool> {
    for b in order {
        let parent = match metas.get(b).and_then(|m| m.parent.clone()) {
            Some(p) => p,
            None => continue,
        };
        if !git::branch_exists(repo, &parent) {
            continue; // parent gone (e.g. merged) — skip; squash handling comes later
        }
        let newbase = git::rev_parse(repo, &parent)?;
        if git::is_ancestor(repo, &newbase, b) {
            // Already sits on top of the parent — just refresh the recorded base.
            meta::set_base(repo, b, &newbase)?;
            continue;
        }
        let oldbase = match metas.get(b).and_then(|m| m.base.clone()) {
            Some(x) => x,
            None => git::merge_base(repo, &parent, b)?,
        };
        if git::rebase_onto(repo, &newbase, &oldbase, b)? {
            meta::set_base(repo, b, &newbase)?;
        } else {
            return Ok(false); // conflict; rebase is paused for the user to resolve
        }
    }
    Ok(true)
}

/// Restack the whole tree (when `from` is None) or `from`'s subtree.
/// Assumes the working tree is clean and no rebase is already in progress.
pub fn run(repo: &Path, from: Option<&str>) -> Result<bool> {
    let metas = meta::all(repo);
    let raw = git::local_branches(repo)?;
    let trunk = git::trunk(repo, &raw);
    let children = children_map(&metas);
    let order = match from {
        Some(b) => subtree_order(&children, b, true),
        None => subtree_order(&children, &trunk, false),
    };

    let original = git::current_branch(repo);
    let completed = run_pass(repo, &order, &metas)?;
    if completed {
        if let Some(b) = original {
            if git::branch_exists(repo, &b) {
                let _ = git::checkout(repo, &b);
            }
        }
    }
    Ok(completed)
}

/// Resume a paused restack: finish the current `git rebase`, then continue the
/// cascade over the rest of the stack. `Ok(false)` = still paused on a conflict.
pub fn continue_(repo: &Path) -> Result<bool> {
    let head = git::rebase_head_branch(repo);
    let cont = git::rebase_continue(repo)?;
    if !cont.success {
        if git::rebase_in_progress(repo) {
            return Ok(false); // more conflicts in the same branch
        }
        return Err(AppError::new(format!(
            "git rebase --continue failed: {}",
            cont.stderr.trim()
        )));
    }

    // The branch we just finished now sits on its parent's current tip.
    if let Some(b) = head {
        let metas = meta::all(repo);
        if let Some(parent) = metas.get(&b).and_then(|m| m.parent.clone()) {
            if let Ok(newbase) = git::rev_parse(repo, &parent) {
                let _ = meta::set_base(repo, &b, &newbase);
            }
        }
    }

    // Resume the cascade; up-to-date branches are skipped, children get rebased.
    run(repo, None)
}

/// Abort a paused restack.
pub fn abort(repo: &Path) -> Result<()> {
    git::rebase_abort(repo)
}

/// Bottom-up branch order for the whole stack (`from` = None) or a subtree.
pub fn topo_order(
    metas: &HashMap<String, meta::Meta>,
    trunk: &str,
    from: Option<&str>,
) -> Vec<String> {
    let children = children_map(metas);
    match from {
        Some(b) => subtree_order(&children, b, true),
        None => subtree_order(&children, trunk, false),
    }
}

/// After a sync, for every merged branch: re-parent its children onto its parent
/// (keeping their recorded base) and untrack it. The local branch is NOT deleted.
pub fn cleanup_merged(repo: &Path, merged: &HashSet<String>, trunk: &str) -> Result<()> {
    let metas = meta::all(repo);
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (name, m) in &metas {
        if let Some(p) = &m.parent {
            children.entry(p.clone()).or_default().push(name.clone());
        }
    }
    for (name, m) in &metas {
        if merged.contains(name) {
            let new_parent = m.parent.clone().unwrap_or_else(|| trunk.to_string());
            if let Some(kids) = children.get(name) {
                for c in kids {
                    let _ = meta::reparent(repo, c, &new_parent);
                }
            }
            let _ = meta::unset_all(repo, name);
        }
    }
    Ok(())
}
