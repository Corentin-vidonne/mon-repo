use crate::error::{AppError, Result};
use std::path::{Component, Path, PathBuf};

const SKIP_DIRS: [&str; 6] = [".git", "node_modules", "target", "dist", ".next", "build"];
const MAX_FILE: u64 = 2_000_000; // 2 MB cap for the viewer

/// Validate a user-supplied relative path: must be relative, `.md`, and contain no
/// `..`/root components (defends against writing outside the repo). Returns it
/// normalized with forward slashes.
pub fn safe_rel(rel: &str) -> Result<String> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() {
        return Err(AppError::new("File path is required"));
    }
    if !rel.to_lowercase().ends_with(".md") {
        return Err(AppError::new("File name must end with .md"));
    }
    let p = Path::new(&rel);
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return Err(AppError::new("Path must be relative (no '..', drive, or root)")),
        }
    }
    Ok(rel)
}

/// Resolve `rel` against `repo` and confirm it stays inside the repo (after the
/// parent dir is canonicalized, to resist symlink/`..` escapes).
fn resolve_inside(repo: &Path, rel: &str) -> Result<PathBuf> {
    let rel = safe_rel(rel)?;
    let target = repo.join(&rel);
    let repo_canon = repo
        .canonicalize()
        .map_err(|e| AppError::new(e.to_string()))?;
    // Canonicalize the deepest existing ancestor and re-append the rest.
    let mut existing = target.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(n) => tail.push(n.to_os_string()),
            None => break,
        }
        if !existing.pop() {
            break;
        }
    }
    let base = existing
        .canonicalize()
        .unwrap_or_else(|_| repo_canon.clone());
    let mut resolved = base;
    for part in tail.into_iter().rev() {
        resolved.push(part);
    }
    if !resolved.starts_with(&repo_canon) {
        return Err(AppError::new("Path escapes the repository"));
    }
    Ok(resolved)
}

/// All Markdown files in the repo (relative, forward-slashed), sorted, skipping
/// vendor/build dirs.
pub fn list(repo: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(repo, repo, &mut out);
    out.sort();
    out
}

fn walk(repo: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk(repo, &path, out);
        } else if name.to_lowercase().ends_with(".md") {
            if let Ok(rel) = path.strip_prefix(repo) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// Read a Markdown file's contents (capped).
pub fn read(repo: &Path, rel: &str) -> Result<String> {
    let path = resolve_inside(repo, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| AppError::new(e.to_string()))?;
    if meta.len() > MAX_FILE {
        return Err(AppError::new("File is too large to display"));
    }
    std::fs::read_to_string(&path).map_err(|e| AppError::new(e.to_string()))
}

/// Write `content` to `rel` (creating parent dirs). Returns the absolute path written.
pub fn write(repo: &Path, rel: &str, content: &str) -> Result<PathBuf> {
    let path = resolve_inside(repo, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::new(e.to_string()))?;
    }
    std::fs::write(&path, content).map_err(|e| AppError::new(e.to_string()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_rel_accepts_md_and_rejects_traversal() {
        assert!(safe_rel("docs/readme.md").is_ok());
        assert!(safe_rel("notes.MD").is_ok());
        assert!(safe_rel("a.txt").is_err());
        assert!(safe_rel("../escape.md").is_err());
        assert!(safe_rel("/abs.md").is_err());
        assert!(safe_rel("").is_err());
    }

    #[test]
    fn write_read_list_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("node_modules")).unwrap();
        std::fs::write(repo.join("node_modules/skip.md"), "x").unwrap();

        write(repo, "docs/intro.md", "# Hello\n\nbody").unwrap();
        let listed = list(repo);
        assert!(listed.contains(&"docs/intro.md".to_string()));
        assert!(
            !listed.iter().any(|p| p.contains("node_modules")),
            "vendor dirs must be skipped"
        );
        assert_eq!(read(repo, "docs/intro.md").unwrap(), "# Hello\n\nbody");
    }

    #[test]
    fn resolve_rejects_escape() {
        let dir = tempfile::tempdir().unwrap();
        assert!(resolve_inside(dir.path(), "../x.md").is_err());
    }
}
