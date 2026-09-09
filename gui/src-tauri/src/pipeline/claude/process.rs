//! Provider process construction.

use super::*;

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
