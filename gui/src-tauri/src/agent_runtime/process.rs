//! Provider process construction.

use tokio::process::Command;

/// Build a tokio Command with the full user PATH and platform-specific flags.
///
/// `cwd` sets the subprocess working directory. When `None`, defaults to the
/// system temp dir to avoid macOS TCC permission prompts for protected folders
/// (~/Music, ~/Photos, etc.) that occur when the app inherits a broad CWD.
/// Pass the paper's source directory when the subprocess needs to read figures
/// or other assets alongside the paper.
pub fn build_silent_command(program: &str, cwd: Option<&str>) -> Command {
    #[allow(unused_mut)]
    let mut std_cmd = std::process::Command::new(program);
    configure_silent_command(&mut std_cmd);
    match cwd {
        Some(dir) => {
            std_cmd.current_dir(dir);
        }
        None => {
            std_cmd.current_dir(std::env::temp_dir());
        }
    }
    let mut command = Command::from(std_cmd);
    // The pass/run supervisors kill the complete process group. This is a
    // final leader-process safeguard if a provider future is dropped during
    // panic or runtime shutdown before normal reaping runs.
    command.kill_on_drop(true);
    command
}

/// Resolve an installed provider CLI and build a tokio command without
/// passing provider arguments through a shell. On Windows, npm `.cmd` shims
/// are represented as `node.exe <validated-entrypoint>` by the resolver.
pub(crate) fn build_provider_command(
    program: &str,
    cwd: Option<&str>,
    args: &[String],
) -> Result<Command, String> {
    let resolved = crate::deps::resolve_command(program).ok_or_else(|| {
        format!(
            "No launchable {program} CLI was found on PATH. On Windows, reinstall it with npm if its command shim is missing or damaged."
        )
    })?;
    let mut std_cmd = resolved.command(args);
    configure_silent_command(&mut std_cmd);
    match cwd {
        Some(dir) => {
            std_cmd.current_dir(dir);
        }
        None => {
            std_cmd.current_dir(std::env::temp_dir());
        }
    }
    let mut command = Command::from(std_cmd);
    command.kill_on_drop(true);
    Ok(command)
}

/// Apply the shared environment and process-isolation flags to a standard
/// command. Extraction uses this directly because it runs on blocking worker
/// threads; async provider and installer commands use `build_silent_command`.
pub fn configure_silent_command(std_cmd: &mut std::process::Command) {
    // Preserve a deliberately sanitized PATH on managed-runtime commands.
    // Ordinary provider commands do not set PATH explicitly and still receive
    // the GUI-safe resolved PATH here.
    if !std_cmd
        .get_envs()
        .any(|(key, _)| key == std::ffi::OsStr::new("PATH"))
    {
        std_cmd.env("PATH", crate::env::full_path());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Make the child the leader of a new process group so cancellation can
        // terminate every tool it spawns without signalling the app itself.
        std_cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
}

// Catalog discovery has its own lifetime; stopping a Review must not kill it.
pub(crate) struct DiscoveryProcessGuard {
    pub(crate) pid: u32,
    armed: bool,
}

impl DiscoveryProcessGuard {
    pub(crate) fn terminate(&self) {
        if self.armed {
            crate::commands::lifecycle::kill_independent_process(self.pid);
        }
    }

    pub(crate) fn register(pid: u32) -> Self {
        if pid > 0 {
            crate::commands::lifecycle::register_independent_child_pid(pid);
        }
        Self {
            pid,
            armed: pid > 0,
        }
    }

    pub(crate) fn unregister(&mut self) {
        if self.armed {
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
            self.armed = false;
        }
    }
}

impl Drop for DiscoveryProcessGuard {
    fn drop(&mut self) {
        if self.armed {
            self.terminate();
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn discovery_process_outlives_review_cancellation_and_is_reaped() {
        let run = crate::run_scope::ActiveScope::begin("discovery-process-isolation");
        crate::run_scope::scoped(run.scope().clone(), async {
            let mut child = build_silent_command("/bin/sh", None)
                .args(["-c", "sleep 60"])
                .spawn()
                .unwrap();
            let mut guard = DiscoveryProcessGuard::register(child.id().unwrap());
            run.scope().cancel();
            assert!(child.try_wait().unwrap().is_none());
            guard.terminate();
            let status = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait())
                .await
                .unwrap()
                .unwrap();
            assert!(!status.success());
            guard.unregister();
        })
        .await;
    }
}
