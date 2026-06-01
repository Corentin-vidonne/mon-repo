use crate::{git, proc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Snapshot of one PR or issue (changed `updated_at` also catches new comments/reviews).
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
struct ItemSnap {
    updated_at: String,
    title: String,
    state: String,
}

/// Persisted per-repo baseline of "what the user has already seen".
#[derive(Serialize, Deserialize, Default, Clone)]
struct Snapshot {
    trunk_remote_sha: Option<String>,
    prs: BTreeMap<u64, ItemSnap>,
    issues: BTreeMap<u64, ItemSnap>,
}

/// One piece of new activity surfaced to the UI / a notification.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItem {
    /// Stable, change-sensitive id (includes updatedAt) so the frontend notifies once.
    pub key: String,
    /// "trunk" | "pr" | "issue"
    pub kind: String,
    pub number: Option<u64>,
    pub title: String,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    pub items: Vec<UpdateItem>,
}

/// djb2 hash → stable, short, filesystem-safe filename for a repo path.
fn hash_path(s: &str) -> String {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33) ^ b as u64;
    }
    format!("{:016x}", h)
}

fn snapshot_file(dir: &Path, repo_root: &str) -> PathBuf {
    dir.join(format!("updates-{}.json", hash_path(repo_root)))
}

fn read_snapshot(file: &Path) -> Option<Snapshot> {
    let text = std::fs::read_to_string(file).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_snapshot(file: &Path, snap: &Snapshot) {
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(snap) {
        let _ = std::fs::write(file, text);
    }
}

/// Read `gh <kind> list` (kind = "pr" | "issue") into number -> snapshot.
fn gh_items(repo: &Path, kind: &str) -> BTreeMap<u64, ItemSnap> {
    let mut map = BTreeMap::new();
    let r = match proc::run(
        "gh",
        [
            kind, "list", "--state", "all", "-L", "50", "--json",
            "number,title,state,updatedAt",
        ],
        Some(repo),
    ) {
        Ok(r) if r.success => r,
        _ => return map,
    };
    let v: serde_json::Value = match serde_json::from_str(&r.stdout) {
        Ok(v) => v,
        Err(_) => return map,
    };
    if let Some(arr) = v.as_array() {
        for it in arr {
            let num = match it.get("number").and_then(|x| x.as_u64()) {
                Some(n) => n,
                None => continue,
            };
            map.insert(
                num,
                ItemSnap {
                    updated_at: it.get("updatedAt").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    title: it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    state: it.get("state").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                },
            );
        }
    }
    map
}

fn build_snapshot(repo: &Path) -> Snapshot {
    let raw = git::local_branches(repo).unwrap_or_default();
    let trunk = git::trunk(repo, &raw);
    let trunk_remote_sha = git::rev_parse(repo, &format!("origin/{}", trunk)).ok();
    Snapshot {
        trunk_remote_sha,
        prs: gh_items(repo, "pr"),
        issues: gh_items(repo, "issue"),
    }
}

/// Pure diff: what changed from the seen `old` baseline to the freshly built `new`.
fn diff(old: &Snapshot, new: &Snapshot) -> Vec<UpdateItem> {
    let mut items = Vec::new();

    if let (Some(o), Some(n)) = (&old.trunk_remote_sha, &new.trunk_remote_sha) {
        if o != n {
            items.push(UpdateItem {
                key: format!("trunk:{}", n),
                kind: "trunk".into(),
                number: None,
                title: "New commits on trunk".into(),
                detail: "The trunk has new commits on origin.".into(),
            });
        }
    }

    let scan = |out: &mut Vec<UpdateItem>,
                kind: &str,
                old_map: &BTreeMap<u64, ItemSnap>,
                new_map: &BTreeMap<u64, ItemSnap>| {
        let label = if kind == "pr" { "PR" } else { "Issue" };
        for (num, n) in new_map {
            match old_map.get(num) {
                None => out.push(UpdateItem {
                    key: format!("{}:{}:{}", kind, num, n.updated_at),
                    kind: kind.into(),
                    number: Some(*num),
                    title: n.title.clone(),
                    detail: format!("New {} #{}: {}", label, num, n.title),
                }),
                Some(o) if o.updated_at != n.updated_at => out.push(UpdateItem {
                    key: format!("{}:{}:{}", kind, num, n.updated_at),
                    kind: kind.into(),
                    number: Some(*num),
                    title: n.title.clone(),
                    detail: format!("{} #{} updated: {}", label, num, n.title),
                }),
                _ => {}
            }
        }
    };
    scan(&mut items, "pr", &old.prs, &new.prs);
    scan(&mut items, "issue", &old.issues, &new.issues);
    items
}

/// Check a repo for activity since the last seen baseline.
/// The first ever check seeds the baseline silently (returns no items).
pub fn check(dir: &Path, repo_root: &str, repo: &Path) -> UpdateReport {
    let file = snapshot_file(dir, repo_root);
    let new = build_snapshot(repo);
    match read_snapshot(&file) {
        None => {
            write_snapshot(&file, &new);
            UpdateReport::default()
        }
        Some(old) => UpdateReport {
            items: diff(&old, &new),
        },
    }
}

/// Record the current state as seen (clears the indicator for this repo).
pub fn mark_seen(dir: &Path, repo_root: &str, repo: &Path) {
    let file = snapshot_file(dir, repo_root);
    write_snapshot(&file, &build_snapshot(repo));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(trunk: &str, prs: &[(u64, &str, &str)], issues: &[(u64, &str, &str)]) -> Snapshot {
        let mk = |list: &[(u64, &str, &str)]| {
            list.iter()
                .map(|(n, upd, title)| {
                    (
                        *n,
                        ItemSnap {
                            updated_at: (*upd).into(),
                            title: (*title).into(),
                            state: "OPEN".into(),
                        },
                    )
                })
                .collect()
        };
        Snapshot {
            trunk_remote_sha: Some(trunk.into()),
            prs: mk(prs),
            issues: mk(issues),
        }
    }

    #[test]
    fn diff_detects_new_and_updated_and_trunk() {
        let old = snap("aaa", &[(1, "t1", "PR one")], &[]);
        let new = snap(
            "bbb",                                   // trunk moved
            &[(1, "t2", "PR one"), (2, "t1", "PR two")], // #1 updated, #2 new
            &[(5, "t1", "Issue five")],              // new issue
        );
        let items = diff(&old, &new);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"trunk:bbb"));
        assert!(keys.contains(&"pr:1:t2"), "updated PR #1");
        assert!(keys.contains(&"pr:2:t1"), "new PR #2");
        assert!(keys.contains(&"issue:5:t1"), "new issue #5");
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn diff_is_empty_when_nothing_changed() {
        let s = snap("aaa", &[(1, "t1", "x")], &[(2, "t1", "y")]);
        assert!(diff(&s, &s.clone()).is_empty());
    }

    #[test]
    fn check_seeds_baseline_on_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        // init a git repo so build_snapshot doesn't choke
        crate::proc::run("git", ["init", "-b", "main"], Some(repo.path())).unwrap();
        let root = repo.path().to_string_lossy().to_string();

        // First check: no baseline file yet -> seeds, returns empty, file now exists.
        let r1 = check(dir.path(), &root, repo.path());
        assert!(r1.items.is_empty());
        assert!(snapshot_file(dir.path(), &root).exists());
        // Second check immediately after: still empty (nothing changed).
        let r2 = check(dir.path(), &root, repo.path());
        assert!(r2.items.is_empty());
    }
}
