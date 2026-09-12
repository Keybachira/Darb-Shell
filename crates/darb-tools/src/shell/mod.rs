//! Shell execution inside the project root.
//!
//! Sync `std::process` with a watchdog thread: the child is waited on in
//! a helper thread while the caller blocks with a timeout, so a hanging
//! command can never stall the agent forever. Output is capped per stream
//! (low-memory rule + token budget). Permission screening happens in the
//! registry *before* this runs — this module only executes.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use darb_core::errors::{DarbError, Result};

/// Max wall-clock time per command. Long builds/tests should run in the
/// user's own terminal; the agent gets fast feedback instead.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// Max captured chars per stream (stdout / stderr).
pub const MAX_OUTPUT_CHARS: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

impl ShellOutput {
    pub fn succeeded(&self) -> bool {
        self.exit_code == 0
    }
}

/// Run `command` with `root` as working directory, killing it after
/// [`COMMAND_TIMEOUT`].
pub fn run(root: &Path, command: &str) -> Result<ShellOutput> {
    run_with_timeout(root, command, COMMAND_TIMEOUT)
}

/// Same as [`run`] with an explicit timeout (used by tests).
pub fn run_with_timeout(root: &Path, command: &str, timeout: Duration) -> Result<ShellOutput> {
    if command.trim().is_empty() {
        return Err(DarbError::Tool(
            "shell command must not be empty".to_string(),
        ));
    }
    let mut cmd = build_command(root, command);
    let mut child = cmd
        .spawn()
        .map_err(|e| DarbError::Tool(format!("could not spawn shell: {e}")))?;

    // Bounded poll loop (10 ms): the only way with std to both enforce a
    // timeout and reap the child. Ends with the command; nothing lingers.
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| DarbError::Tool(format!("could not read command output: {e}")))?;
                let (stdout, stdout_cut) = cap_stream(&output.stdout);
                let (stderr, stderr_cut) = cap_stream(&output.stderr);
                return Ok(ShellOutput {
                    exit_code: status.code().unwrap_or(-1),
                    stdout,
                    stderr,
                    truncated: stdout_cut || stderr_cut,
                });
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    // Kill and reap: never leave orphans behind.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DarbError::Tool(format!(
                        "command timed out after {}s and was killed; prefer faster commands",
                        timeout.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(DarbError::Tool(format!("could not wait for shell: {e}")));
            }
        }
    }
}

fn build_command(root: &Path, command: &str) -> Command {
    // Pipes are captured (never inherited: agent output must not leak into
    // the TUI) and stdin is null so commands can't block on input.
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command).current_dir(root);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(root);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

/// Truncate a byte stream to [`MAX_OUTPUT_CHARS`] chars (char-boundary
/// safe: truncation happens after lossy UTF-8 decoding).
fn cap_stream(bytes: &[u8]) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= MAX_OUTPUT_CHARS {
        return (text.into_owned(), false);
    }
    let kept: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
    (format!("{kept}\n…[truncated]"), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::temp_root;

    #[test]
    fn captures_stdout_and_status() {
        let root = temp_root();
        let out = run(&root, "echo hello-darb").expect("run");
        assert!(out.succeeded());
        assert!(out.stdout.contains("hello-darb"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nonzero_exit_is_reported() {
        let root = temp_root();
        #[cfg(windows)]
        let out = run(&root, "exit 3").expect("run");
        #[cfg(not(windows))]
        let out = run(&root, "exit 3").expect("run");
        assert!(!out.succeeded());
        assert_eq!(out.exit_code, 3);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_command_is_rejected() {
        let root = temp_root();
        assert!(run(&root, "   ").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn timeout_is_enforced() {
        let root = temp_root();
        // `timeout.exe` refuses redirected stdin, so ping is the sleeper.
        #[cfg(windows)]
        let sleep = "ping -n 6 127.0.0.1 >nul";
        #[cfg(not(windows))]
        let sleep = "sleep 5";
        let err = run_with_timeout(&root, sleep, Duration::from_millis(300)).unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
