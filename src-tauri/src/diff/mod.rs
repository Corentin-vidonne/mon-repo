//! Minimal unified-diff parser + partial-patch builder, used to split a commit at the
//! hunk / individual-line level (a `git add -p`-style selection).
//!
//! `parse` turns `git diff <parent> <sha>` into a structured form where every added /
//! deleted line of a *splittable* file gets a stable `id` (sequential in diff order).
//! The frontend renders this and sends back the set of ids the user wants in the FIRST
//! commit; `build_partial_patch` rebuilds a patch containing only those changes, ready
//! for `git apply --cached --recount`. The transform per hunk:
//!   - context line                → kept as context
//!   - selected deletion           → kept as a deletion
//!   - **un**selected deletion     → demoted to context (we don't remove it yet)
//!   - selected addition           → kept as an addition
//!   - **un**selected addition     → dropped entirely
//! Counts are recomputed from the rewritten body (and `--recount` is a safety net).
//!
//! Files that can't be line-split (binary, pure deletions) are marked `selectable=false`
//! and simply fall into the *second* commit (the `git add -A` catch-all); renames make
//! the whole split refuse (handled by the caller).

use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Del,
    /// A `\ No newline at end of file` marker — carried verbatim, never selectable.
    Meta,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line text WITHOUT the leading +/-/space marker (the full line for `Meta`).
    pub text: String,
    /// Stable id for selectable add/del lines; `None` for context/meta and for any line
    /// in a non-selectable file.
    pub id: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@ …` header line, verbatim.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// New-side path (the display path).
    pub path: String,
    pub hunks: Vec<Hunk>,
    /// Whether this file's lines can be individually selected. False for binary files
    /// and pure deletions — those always land in the second commit.
    pub selectable: bool,
    /// Raw header block (`diff --git` … up to the first `@@`), kept for patch rebuilding.
    #[serde(skip)]
    pub header: Vec<String>,
    /// File is a rename/copy — the caller refuses the split when any file is.
    #[serde(skip)]
    pub renamed: bool,
}

/// Strip a leading `a/` or `b/` (and any trailing tab-decoration) from a diff path.
fn strip_ab_prefix(p: &str) -> String {
    let p = p.split('\t').next().unwrap_or(p);
    p.strip_prefix("a/")
        .or_else(|| p.strip_prefix("b/"))
        .unwrap_or(p)
        .to_string()
}

/// Best-effort new-side path from a `diff --git a/<p> b/<p>` line.
fn path_from_git_line(line: &str) -> String {
    match line.rsplit_once(" b/") {
        Some((_, b)) => b.split('\t').next().unwrap_or(b).to_string(),
        None => line.trim_start_matches("diff --git ").to_string(),
    }
}

/// Parse `(old_start, new_start)` out of a hunk header `@@ -a,b +c,d @@ …`.
fn parse_hunk_starts(header: &str) -> (u32, u32) {
    let (mut old_start, mut new_start) = (0u32, 0u32);
    for tok in header.split_whitespace() {
        if let Some(r) = tok.strip_prefix('-') {
            old_start = r.split(',').next().and_then(|n| n.parse().ok()).unwrap_or(0);
        } else if let Some(r) = tok.strip_prefix('+') {
            new_start = r.split(',').next().and_then(|n| n.parse().ok()).unwrap_or(0);
        }
    }
    (old_start, new_start)
}

/// Parse a full unified diff (`git diff <parent> <sha>`) into structured files.
pub fn parse(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cur: Option<FileDiff> = None;
    let mut binary = false;
    let mut deleted = false;
    let mut in_hunk = false;

    let finish = |files: &mut Vec<FileDiff>,
                  cur: &mut Option<FileDiff>,
                  binary: &mut bool,
                  deleted: &mut bool| {
        if let Some(mut f) = cur.take() {
            f.selectable = !(*binary || *deleted || f.renamed);
            files.push(f);
        }
        *binary = false;
        *deleted = false;
    };

    for raw in diff.lines() {
        if raw.starts_with("diff --git ") {
            finish(&mut files, &mut cur, &mut binary, &mut deleted);
            cur = Some(FileDiff {
                path: path_from_git_line(raw),
                hunks: Vec::new(),
                selectable: true,
                header: vec![raw.to_string()],
                renamed: false,
            });
            in_hunk = false;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };

        if raw.starts_with("@@") {
            f.hunks.push(Hunk {
                header: raw.to_string(),
                lines: Vec::new(),
            });
            in_hunk = true;
            continue;
        }

        if !in_hunk {
            // File header block (before the first hunk).
            if raw.starts_with("rename ") || raw.starts_with("copy ") {
                f.renamed = true;
            } else if raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch") {
                binary = true;
            } else if raw.starts_with("deleted file mode") {
                deleted = true;
            } else if let Some(p) = raw.strip_prefix("+++ ") {
                if p != "/dev/null" {
                    f.path = strip_ab_prefix(p);
                }
            } else if let Some(p) = raw.strip_prefix("--- ") {
                // For a deletion (+++ is /dev/null) the real path is on the --- side.
                if p != "/dev/null" {
                    f.path = strip_ab_prefix(p);
                }
            }
            f.header.push(raw.to_string());
            continue;
        }

        // Inside a hunk body.
        let hunk = f.hunks.last_mut().unwrap();
        let line = if let Some(t) = raw.strip_prefix('+') {
            DiffLine { kind: LineKind::Add, text: t.to_string(), id: None }
        } else if let Some(t) = raw.strip_prefix('-') {
            DiffLine { kind: LineKind::Del, text: t.to_string(), id: None }
        } else if raw.starts_with('\\') {
            DiffLine { kind: LineKind::Meta, text: raw.to_string(), id: None }
        } else if let Some(t) = raw.strip_prefix(' ') {
            DiffLine { kind: LineKind::Context, text: t.to_string(), id: None }
        } else {
            // Empty or unexpected line → treat as (possibly empty) context.
            DiffLine { kind: LineKind::Context, text: raw.to_string(), id: None }
        };
        hunk.lines.push(line);
    }
    finish(&mut files, &mut cur, &mut binary, &mut deleted);

    assign_ids(&mut files);
    files
}

/// Number every add/del line of a selectable file, sequentially in diff order.
fn assign_ids(files: &mut [FileDiff]) {
    let mut next: u32 = 0;
    for f in files.iter_mut() {
        if !f.selectable {
            continue;
        }
        for h in &mut f.hunks {
            for l in &mut h.lines {
                if matches!(l.kind, LineKind::Add | LineKind::Del) {
                    l.id = Some(next);
                    next += 1;
                }
            }
        }
    }
}

/// All selectable line ids across the diff (the universe a selection must be a subset of).
pub fn selectable_ids(files: &[FileDiff]) -> HashSet<u32> {
    let mut s = HashSet::new();
    for f in files {
        for h in &f.hunks {
            for l in &h.lines {
                if let Some(id) = l.id {
                    s.insert(id);
                }
            }
        }
    }
    s
}

/// Build a patch carrying only the `selected` lines, for `git apply --cached --recount`.
/// Files with no selected line — and non-selectable files — are omitted entirely.
pub fn build_partial_patch(files: &[FileDiff], selected: &HashSet<u32>) -> String {
    let mut out = String::new();
    for f in files {
        if !f.selectable {
            continue;
        }
        let mut chunks = String::new();
        for h in &f.hunks {
            let touched = h
                .lines
                .iter()
                .any(|l| l.id.map_or(false, |id| selected.contains(&id)));
            if !touched {
                continue;
            }
            let mut body = String::new();
            let (mut old_count, mut new_count) = (0u32, 0u32);
            let mut prev_emitted = false;
            for l in &h.lines {
                match l.kind {
                    LineKind::Context => {
                        body.push(' ');
                        body.push_str(&l.text);
                        body.push('\n');
                        old_count += 1;
                        new_count += 1;
                        prev_emitted = true;
                    }
                    LineKind::Del => {
                        if l.id.map_or(false, |id| selected.contains(&id)) {
                            body.push('-');
                            body.push_str(&l.text);
                            body.push('\n');
                            old_count += 1;
                        } else {
                            // Unselected deletion stays in the file → context.
                            body.push(' ');
                            body.push_str(&l.text);
                            body.push('\n');
                            old_count += 1;
                            new_count += 1;
                        }
                        prev_emitted = true;
                    }
                    LineKind::Add => {
                        if l.id.map_or(false, |id| selected.contains(&id)) {
                            body.push('+');
                            body.push_str(&l.text);
                            body.push('\n');
                            new_count += 1;
                            prev_emitted = true;
                        } else {
                            // Unselected addition isn't in this commit → drop the line.
                            prev_emitted = false;
                        }
                    }
                    LineKind::Meta => {
                        // "\ No newline…" only makes sense right after a kept line.
                        if prev_emitted {
                            body.push_str(&l.text);
                            body.push('\n');
                        }
                    }
                }
            }
            let (old_start, new_start) = parse_hunk_starts(&h.header);
            chunks.push_str(&format!(
                "@@ -{},{} +{},{} @@\n",
                old_start, old_count, new_start, new_count
            ));
            chunks.push_str(&body);
        }
        if chunks.is_empty() {
            continue;
        }
        for hl in &f.header {
            out.push_str(hl);
            out.push('\n');
        }
        out.push_str(&chunks);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(text: &str) -> HashSet<u32> {
        selectable_ids(&parse(text))
    }

    #[test]
    fn parses_a_modification_and_numbers_changes() {
        let diff = "diff --git a/f.txt b/f.txt\n\
                    index 111..222 100644\n\
                    --- a/f.txt\n\
                    +++ b/f.txt\n\
                    @@ -1,3 +1,3 @@\n\
                    \x20ctx\n\
                    -old line\n\
                    +new line\n\
                    \x20tail\n";
        let files = parse(diff);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path, "f.txt");
        assert!(f.selectable);
        let kinds: Vec<LineKind> = f.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![LineKind::Context, LineKind::Del, LineKind::Add, LineKind::Context]
        );
        // Only the del + add are numbered.
        assert_eq!(ids(diff), HashSet::from([0, 1]));
    }

    #[test]
    fn builds_patch_for_one_selected_addition() {
        // Two added lines; selecting only the first must demote the deletion to context
        // and drop the second addition, with recomputed counts.
        let diff = "diff --git a/f.txt b/f.txt\n\
                    --- a/f.txt\n\
                    +++ b/f.txt\n\
                    @@ -1,2 +1,3 @@\n\
                    \x20keep\n\
                    -gone\n\
                    +add A\n\
                    +add B\n";
        let files = parse(diff);
        // ids: del "gone"=0, add A=1, add B=2. Select only "add A".
        let patch = build_partial_patch(&files, &HashSet::from([1]));
        assert!(patch.contains("+add A"), "patch:\n{patch}");
        assert!(!patch.contains("+add B"), "unselected add must be dropped:\n{patch}");
        assert!(!patch.contains("-gone"), "unselected del must become context:\n{patch}");
        assert!(patch.contains(" gone"), "demoted deletion kept as context:\n{patch}");
        // Body had: context(keep), context(gone), add(add A) → -1,2 +1,3.
        assert!(patch.contains("@@ -1,2 +1,3 @@"), "recounted header:\n{patch}");
    }

    #[test]
    fn new_file_is_selectable_and_partial() {
        let diff = "diff --git a/g.txt b/g.txt\n\
                    new file mode 100644\n\
                    index 0000000..333\n\
                    --- /dev/null\n\
                    +++ b/g.txt\n\
                    @@ -0,0 +1,3 @@\n\
                    +g1\n\
                    +g2\n\
                    +g3\n";
        let files = parse(diff);
        assert!(files[0].selectable);
        assert_eq!(files[0].path, "g.txt");
        // Select g1 and g3 (ids 0 and 2).
        let patch = build_partial_patch(&files, &HashSet::from([0, 2]));
        assert!(patch.contains("+g1") && patch.contains("+g3"));
        assert!(!patch.contains("+g2"));
        assert!(patch.contains("--- /dev/null"));
        assert!(patch.contains("@@ -0,0 +1,2 @@"));
    }

    #[test]
    fn deleted_and_binary_files_are_not_selectable() {
        let del = "diff --git a/d.txt b/d.txt\n\
                   deleted file mode 100644\n\
                   index 444..0000000\n\
                   --- a/d.txt\n\
                   +++ /dev/null\n\
                   @@ -1,2 +0,0 @@\n\
                   -x\n\
                   -y\n";
        let files = parse(del);
        assert!(!files[0].selectable, "pure deletion is not line-splittable");
        assert!(ids(del).is_empty(), "a non-selectable file numbers no lines");
        assert_eq!(files[0].path, "d.txt");

        let bin = "diff --git a/img.png b/img.png\n\
                   index 1..2 100644\n\
                   Binary files a/img.png and b/img.png differ\n";
        let bfiles = parse(bin);
        assert!(!bfiles[0].selectable);
    }

    #[test]
    fn rename_is_flagged() {
        let diff = "diff --git a/old.txt b/new.txt\n\
                    similarity index 100%\n\
                    rename from old.txt\n\
                    rename to new.txt\n";
        let files = parse(diff);
        assert!(files[0].renamed);
        assert!(!files[0].selectable);
    }

    #[test]
    fn ids_are_sequential_across_files() {
        let diff = "diff --git a/a.txt b/a.txt\n\
                    --- a/a.txt\n\
                    +++ b/a.txt\n\
                    @@ -0,0 +1,2 @@\n\
                    +a1\n\
                    +a2\n\
                    diff --git a/b.txt b/b.txt\n\
                    --- a/b.txt\n\
                    +++ b/b.txt\n\
                    @@ -0,0 +1,1 @@\n\
                    +b1\n";
        let files = parse(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(ids(diff), HashSet::from([0, 1, 2]));
        // b.txt's single addition is the third id.
        let b1 = &files[1].hunks[0].lines[0];
        assert_eq!(b1.id, Some(2));
    }
}
