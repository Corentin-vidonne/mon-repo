use crate::error::{AppError, Result};
use crate::{assist, git};
use base64::Engine as _;
use portable_pty::{native_pty_system, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// Active embedded-terminal sessions, keyed by a frontend-generated id.
#[derive(Default)]
pub struct Terminals(Mutex<HashMap<String, Session>>);

#[derive(Clone, Serialize)]
struct TermOutput {
    id: String,
    /// Base64-encoded raw bytes (avoids UTF-8 boundary corruption over IPC).
    data: String,
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Open a PTY running `claude` (pre-seeded with `prompt`) in `repo`, streaming output
/// to the frontend under session `id`. Shared by the commit and PR analysis commands.
fn spawn_claude_session(
    app: &AppHandle,
    state: &State<'_, Terminals>,
    id: String,
    repo: &Path,
    prompt: &str,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let pair = native_pty_system()
        .openpty(pty_size(cols, rows))
        .map_err(|e| AppError::new(e.to_string()))?;

    let cmd = assist::pty_command(repo, prompt)?;
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| AppError::new(e.to_string()))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| AppError::new(e.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| AppError::new(e.to_string()))?;

    let app_handle = app.clone();
    let session_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let data = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    let _ = app_handle.emit(
                        "term-output",
                        TermOutput {
                            id: session_id.clone(),
                            data,
                        },
                    );
                }
            }
        }
        let _ = app_handle.emit("term-exit", session_id.clone());
    });

    state.0.lock().unwrap().insert(
        id,
        Session {
            master: pair.master,
            writer,
            child,
        },
    );
    Ok(())
}

#[tauri::command]
pub fn term_open_analyze(
    app: AppHandle,
    state: State<'_, Terminals>,
    id: String,
    path: String,
    sha: String,
    mode: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let prompt = assist::analysis_prompt(repo, &sha, &mode)?;
    spawn_claude_session(&app, &state, id, repo, &prompt, cols, rows)
}

/// Open an embedded terminal running `claude` to analyze a whole Pull Request.
#[tauri::command]
pub fn term_open_analyze_pr(
    app: AppHandle,
    state: State<'_, Terminals>,
    id: String,
    path: String,
    number: u64,
    mode: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    let root = git::repo_root(Path::new(&path))?;
    let repo = Path::new(&root);
    let detail = crate::github::pr_detail(repo, number)?;
    let prompt =
        assist::pr_analysis_prompt(number, &detail.title, &detail.head_ref, &detail.base_ref, &mode);
    spawn_claude_session(&app, &state, id, repo, &prompt, cols, rows)
}

/// Forward user keystrokes to the PTY.
#[tauri::command]
pub fn term_write(state: State<'_, Terminals>, id: String, data: String) -> Result<()> {
    if let Some(s) = state.0.lock().unwrap().get_mut(&id) {
        let _ = s.writer.write_all(data.as_bytes());
        let _ = s.writer.flush();
    }
    Ok(())
}

/// Resize the PTY when the terminal pane changes size.
#[tauri::command]
pub fn term_resize(state: State<'_, Terminals>, id: String, cols: u16, rows: u16) -> Result<()> {
    if let Some(s) = state.0.lock().unwrap().get(&id) {
        let _ = s.master.resize(pty_size(cols, rows));
    }
    Ok(())
}

/// Close a terminal session and kill its process.
#[tauri::command]
pub fn term_close(state: State<'_, Terminals>, id: String) -> Result<()> {
    if let Some(mut s) = state.0.lock().unwrap().remove(&id) {
        let _ = s.child.kill();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verifies portable-pty passes a multi-line / quoted argument to a NATIVE exe
    // intact (the claude.exe scenario). Requires %TEMP%\argecho.exe compiled separately:
    //   rustc argecho.rs -o argecho.exe
    // Run with: cargo test pty_passes_multiline_arg_intact -- --ignored --nocapture
    #[test]
    #[ignore]
    fn pty_passes_multiline_arg_intact() {
        let tmp = std::env::temp_dir();
        let exe = tmp.join("argecho.exe");
        if !exe.exists() {
            eprintln!("argecho.exe missing — skipping");
            return;
        }
        let out = tmp.join("argecho-out.txt");
        let _ = std::fs::remove_file(&out);

        let prompt = "Analyse le commit `862455c4` (sujet : wip: \"scratch\")\n\
                      Ligne 2 avec des espaces\n  - une puce\nFin.";

        let pair = native_pty_system().openpty(pty_size(80, 24)).unwrap();
        let mut cmd = portable_pty::CommandBuilder::new(&exe);
        cmd.arg(prompt);
        cmd.env("ARGECHO_OUT", &out);
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let _ = child.wait();

        let got = std::fs::read_to_string(&out).unwrap();
        assert_eq!(got, prompt, "PTY must pass the multi-line prompt intact");
        eprintln!("OK: claude.exe would receive {} bytes intact", got.len());
    }
}
