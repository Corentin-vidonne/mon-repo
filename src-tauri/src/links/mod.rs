use crate::proc;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A repository node in the inter-repo graph.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoNode {
    pub id: String,
    pub name: String,
    pub remote_url: Option<String>,
}

/// A dependency edge: `from` declares `to` as a dependency.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoEdge {
    pub from: String,
    pub to: String,
    /// Where the link was found (e.g. "package.json", "pyproject.toml", "submodule").
    pub via: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoGraph {
    pub nodes: Vec<RepoNode>,
    pub edges: Vec<RepoEdge>,
}

#[derive(Default, Clone, Debug)]
struct Facts {
    path: String,
    display: String,
    /// Normalized identity names (manifest `name` fields).
    names: Vec<String>,
    /// Normalized remote URL.
    remote: Option<String>,
    deps: Vec<Dep>,
}

#[derive(Clone, Debug)]
struct Dep {
    target_name: Option<String>,
    target_url: Option<String>,
    via: String,
}

/// Normalize a package name (PEP 503-ish): lowercase, `_`/`.` -> `-`.
fn norm_name(s: &str) -> String {
    s.trim().to_lowercase().replace(['_', '.'], "-")
}

/// Normalize a git URL to `host/owner/repo` for matching across URL shapes.
fn norm_url(s: &str) -> Option<String> {
    let mut u = s.trim().to_string();
    if u.is_empty() {
        return None;
    }
    for p in ["git+", "ssh://", "https://", "http://", "git://"] {
        if let Some(rest) = u.strip_prefix(p) {
            u = rest.to_string();
        }
    }
    // scp-like form: user@host:owner/repo
    if let Some(idx) = u.find('@') {
        u = u[idx + 1..].to_string();
    }
    u = u.replace(':', "/");
    if let Some(stripped) = u.strip_suffix(".git") {
        u = stripped.to_string();
    }
    let u = u.trim_end_matches('/').to_lowercase();
    if u.is_empty() {
        None
    } else {
        Some(u)
    }
}

/// Leading distribution name from a PEP 508 requirement (e.g. "requests>=2" -> "requests").
fn pep508_name(s: &str) -> Option<String> {
    let s = s.trim();
    let end = s
        .find(|c: char| c.is_whitespace() || "><=!~[;(@,".contains(c))
        .unwrap_or(s.len());
    let name = s[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(norm_name(name))
    }
}

fn parse_package_json(repo: &Path, f: &mut Facts) {
    let content = match std::fs::read_to_string(repo.join("package.json")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let val: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(n) = val.get("name").and_then(|x| x.as_str()) {
        f.names.push(norm_name(n));
    }
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = val.get(section).and_then(|x| x.as_object()) {
            for (k, v) in obj {
                let mut dep = Dep {
                    target_name: Some(norm_name(k)),
                    target_url: None,
                    via: "package.json".into(),
                };
                if let Some(spec) = v.as_str() {
                    if spec.contains("://") || spec.contains("git@") || spec.starts_with("git+") {
                        dep.target_url = norm_url(spec);
                    } else if let Some(rest) = spec.strip_prefix("github:") {
                        dep.target_url = norm_url(&format!("github.com/{}", rest));
                    }
                }
                f.deps.push(dep);
            }
        }
    }
}

fn toml_git_url(v: &toml::Value) -> Option<String> {
    v.as_table()
        .and_then(|t| t.get("git"))
        .and_then(|g| g.as_str())
        .and_then(norm_url)
}

fn parse_cargo(repo: &Path, f: &mut Facts) {
    let content = match std::fs::read_to_string(repo.join("Cargo.toml")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let val: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(n) = val
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|x| x.as_str())
    {
        f.names.push(norm_name(n));
    }
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(tbl) = val.get(section).and_then(|t| t.as_table()) {
            for (k, v) in tbl {
                f.deps.push(Dep {
                    target_name: Some(norm_name(k)),
                    target_url: toml_git_url(v),
                    via: "Cargo.toml".into(),
                });
            }
        }
    }
}

fn parse_pyproject(repo: &Path, f: &mut Facts) {
    let content = match std::fs::read_to_string(repo.join("pyproject.toml")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let val: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(n) = val
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|x| x.as_str())
    {
        f.names.push(norm_name(n));
    }
    if let Some(n) = val
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("name"))
        .and_then(|x| x.as_str())
    {
        f.names.push(norm_name(n));
    }
    // PEP 621: project.dependencies = ["pkg>=1", ...]
    if let Some(arr) = val
        .get("project")
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_array())
    {
        for item in arr {
            if let Some(name) = item.as_str().and_then(pep508_name) {
                f.deps.push(Dep {
                    target_name: Some(name),
                    target_url: None,
                    via: "pyproject.toml".into(),
                });
            }
        }
    }
    // Poetry: [tool.poetry.dependencies] table
    if let Some(tbl) = val
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for (k, v) in tbl {
            if k == "python" {
                continue;
            }
            f.deps.push(Dep {
                target_name: Some(norm_name(k)),
                target_url: toml_git_url(v),
                via: "pyproject.toml".into(),
            });
        }
    }
}

fn parse_pixi(repo: &Path, f: &mut Facts) {
    let content = match std::fs::read_to_string(repo.join("pixi.toml")) {
        Ok(c) => c,
        Err(_) => return,
    };
    let val: toml::Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Some(n) = val
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|x| x.as_str())
    {
        f.names.push(norm_name(n));
    }
    for section in [
        "dependencies",
        "pypi-dependencies",
        "build-dependencies",
        "host-dependencies",
    ] {
        if let Some(tbl) = val.get(section).and_then(|t| t.as_table()) {
            for (k, v) in tbl {
                f.deps.push(Dep {
                    target_name: Some(norm_name(k)),
                    target_url: toml_git_url(v),
                    via: "pixi.toml".into(),
                });
            }
        }
    }
}

fn parse_gitmodules(repo: &Path, f: &mut Facts) {
    let content = match std::fs::read_to_string(repo.join(".gitmodules")) {
        Ok(c) => c,
        Err(_) => return,
    };
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("url") {
            if let Some(eq) = rest.find('=') {
                if let Some(u) = norm_url(rest[eq + 1..].trim()) {
                    f.deps.push(Dep {
                        target_name: None,
                        target_url: Some(u),
                        via: "submodule".into(),
                    });
                }
            }
        }
    }
}

fn gather(path: &Path) -> Option<Facts> {
    let root = crate::git::repo_root(path).ok()?;
    let repo = Path::new(&root);
    let display = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| root.clone());
    let mut f = Facts {
        path: root.clone(),
        display,
        ..Default::default()
    };
    if let Ok(r) = proc::run("git", ["remote", "get-url", "origin"], Some(repo)) {
        if r.success {
            f.remote = norm_url(r.stdout.trim());
        }
    }
    parse_package_json(repo, &mut f);
    parse_cargo(repo, &mut f);
    parse_pyproject(repo, &mut f);
    parse_pixi(repo, &mut f);
    parse_gitmodules(repo, &mut f);
    Some(f)
}

/// Pure edge computation: match each repo's dependency targets against the others'
/// identity names / remote URLs. Side-effect free, so it can be unit-tested.
fn compute_edges(facts: &[Facts]) -> Vec<RepoEdge> {
    let mut by_name: HashMap<String, String> = HashMap::new();
    let mut by_url: HashMap<String, String> = HashMap::new();
    for f in facts {
        for n in &f.names {
            by_name.entry(n.clone()).or_insert_with(|| f.path.clone());
        }
        if let Some(u) = &f.remote {
            by_url.entry(u.clone()).or_insert_with(|| f.path.clone());
        }
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut edges = Vec::new();
    for f in facts {
        for d in &f.deps {
            let to = d
                .target_name
                .as_ref()
                .and_then(|n| by_name.get(n))
                .or_else(|| d.target_url.as_ref().and_then(|u| by_url.get(u)));
            if let Some(to) = to {
                if *to == f.path {
                    continue;
                }
                if seen.insert((f.path.clone(), to.clone())) {
                    edges.push(RepoEdge {
                        from: f.path.clone(),
                        to: to.clone(),
                        via: d.via.clone(),
                    });
                }
            }
        }
    }
    edges
}

/// Build the inter-repo dependency graph for the given repo paths.
pub fn analyze(paths: &[String]) -> RepoGraph {
    let facts: Vec<Facts> = paths.iter().filter_map(|p| gather(Path::new(p))).collect();
    let nodes = facts
        .iter()
        .map(|f| RepoNode {
            id: f.path.clone(),
            name: f.display.clone(),
            remote_url: f.remote.clone(),
        })
        .collect();
    let edges = compute_edges(&facts);
    RepoGraph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            norm_url("git@github.com:me/ui.git").as_deref(),
            Some("github.com/me/ui")
        );
        assert_eq!(
            norm_url("https://github.com/me/ui.git").as_deref(),
            Some("github.com/me/ui")
        );
        assert_eq!(
            norm_url("git+https://github.com/Me/UI").as_deref(),
            Some("github.com/me/ui")
        );
    }

    #[test]
    fn edges_by_name_and_url() {
        let a = Facts {
            path: "/a".into(),
            display: "a".into(),
            names: vec![norm_name("@me/a")],
            remote: None,
            deps: vec![Dep {
                target_name: Some(norm_name("@me/b")),
                target_url: None,
                via: "package.json".into(),
            }],
        };
        let b = Facts {
            path: "/b".into(),
            display: "b".into(),
            names: vec![norm_name("@me/b")],
            remote: Some("github.com/me/b".into()),
            deps: vec![Dep {
                target_name: None,
                target_url: Some("github.com/me/a".into()),
                via: "submodule".into(),
            }],
        };
        // b's submodule URL points at a's remote — add a's remote so it matches.
        let mut a = a;
        a.remote = Some("github.com/me/a".into());

        let edges = compute_edges(&[a, b]);
        assert_eq!(edges.len(), 2);
        assert!(edges.iter().any(|e| e.from == "/a" && e.to == "/b"));
        assert!(edges.iter().any(|e| e.from == "/b" && e.to == "/a"));
    }

    #[test]
    fn parses_package_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@me/web","dependencies":{"@me/ui":"^1.0","react":"^19"}}"#,
        )
        .unwrap();
        let mut f = Facts::default();
        parse_package_json(dir.path(), &mut f);
        assert!(f.names.contains(&"@me/web".to_string()));
        assert!(f.deps.iter().any(|d| d.target_name.as_deref() == Some("@me/ui")));
    }
}
