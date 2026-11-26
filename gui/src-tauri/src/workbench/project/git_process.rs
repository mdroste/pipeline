//! Path normalization and bounded capture for prepared Git commands.

use super::{err, WorkbenchError, WorkbenchResult};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Canonical Windows roots are verbatim (`\\?\C:\…`); Git for Windows takes
/// the equivalent drive or UNC form as an argument and working directory.
pub(super) fn git_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// Bytes of standard error, and by default of standard output, a captured Git
/// process may print.
const GIT_OUTPUT_LIMIT: u64 = 2 * 1024 * 1024;

/// How long a captured Git process may run, what can stop it early, and how much
/// it may print.
pub(super) struct GitLimits<'a> {
    pub(super) timeout: Duration,
    pub(super) cancel: Option<&'a std::sync::atomic::AtomicBool>,
    pub(super) max_output: u64,
    /// Past `max_output`, stop Git and return the bytes before the limit
    /// instead of failing.
    pub(super) keep_prefix: bool,
}
impl Default for GitLimits<'_> {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            cancel: None,
            max_output: GIT_OUTPUT_LIMIT,
            keep_prefix: false,
        }
    }
}
/// Runs a prepared Git command in `root` with bounded, file-backed capture.
pub(super) fn capture_git(
    mut command: std::process::Command,
    root: &Path,
    limits: GitLimits<'_>,
) -> WorkbenchResult<Vec<u8>> {
    use std::process::Stdio;
    command.current_dir(git_path(root)).stdin(Stdio::null());
    // File-backed capture cannot block behind inherited pipes. Independent owner:
    // Workflow cancellation never receives this PID.
    let out = tempfile::tempfile().map_err(err)?;
    let error = tempfile::tempfile().map_err(err)?;
    command
        .stdout(out.try_clone().map_err(err)?)
        .stderr(error.try_clone().map_err(err)?);
    crate::process::configure_isolation(&mut command);
    let mut child = command.spawn().map_err(err)?;
    let pid = child.id();
    crate::process_tree::register_independent_child_pid(pid);
    let started = Instant::now();
    let mut wait = Duration::from_millis(1);
    let result = loop {
        let printed = out.metadata().map_err(err)?.len();
        if error.metadata().map_err(err)?.len() > GIT_OUTPUT_LIMIT
            || (printed > limits.max_output && !limits.keep_prefix)
        {
            break Err(WorkbenchError::invalid(
                "Git output exceeded its bounded capture limit",
            ));
        }
        if printed > limits.max_output {
            break Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(Some(status)),
            Ok(None)
                if limits
                    .cancel
                    .is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Relaxed)) =>
            {
                break Err(WorkbenchError::invalid("Stopped before Git finished."));
            }
            Ok(None) if started.elapsed() < limits.timeout => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(25));
            }
            Ok(None) => {
                break Err(WorkbenchError::invalid(
                    "Git operation timed out; inspect task state before retrying",
                ))
            }
            Err(e) => break Err(err(e)),
        }
    };
    // Reap descendants even after leader exit.
    crate::process_tree::kill_independent_process(pid);
    let _ = child.wait();
    crate::process_tree::unregister_independent_child_pid(pid);
    use std::io::{Seek, SeekFrom};
    let mut out = out;
    let mut error = error;
    out.seek(SeekFrom::Start(0)).map_err(err)?;
    error.seek(SeekFrom::Start(0)).map_err(err)?;
    let mut bytes = Vec::new();
    out.take(limits.max_output + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    // `None` is Git stopped at `max_output`, which the caller asked for.
    if result?.is_some_and(|status| !status.success()) {
        // A hook may print any bytes, and the limit can split a character.
        let mut raw = Vec::new();
        error.take(8192).read_to_end(&mut raw).map_err(err)?;
        let mut message = String::from_utf8_lossy(&raw).into_owned();
        // Some refusals, such as "nothing to commit", only reach stdout.
        if message.trim().is_empty() {
            message = String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]).into_owned();
        }
        return Err(WorkbenchError::invalid(format!(
            "Git task setup failed: {message}"
        )));
    }
    if bytes.len() as u64 > limits.max_output {
        if !limits.keep_prefix {
            return Err(WorkbenchError::invalid("Git metadata exceeds its limit"));
        }
        bytes.truncate(limits.max_output as usize);
    }
    Ok(bytes)
}
