use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::Command;

/// On Windows, spawn child processes without flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Build a [`Command`] that won't pop up a console window on Windows.
pub fn command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Captured result of running an external command.
#[allow(dead_code)] // `code` is reserved for finer error handling
pub struct Run {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run `program` with `args` (optionally in `cwd`) and capture its output.
pub fn run<I, S>(program: &str, args: I, cwd: Option<&Path>) -> io::Result<Run>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    run_env(program, args, cwd, &[])
}

/// Like [`run`], but also sets environment variables (e.g. `GIT_EDITOR=true`).
pub fn run_env<I, S>(
    program: &str,
    args: I,
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> io::Result<Run>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = command(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output()?;
    Ok(Run {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
