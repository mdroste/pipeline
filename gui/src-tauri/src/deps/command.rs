/// A command resolved to the exact program and fixed prefix arguments that can
/// actually be passed to `CreateProcess`/`execve`.
///
/// On Windows, npm exposes package binaries as `.cmd` shims. Rust's
/// `Command::new` cannot execute those scripts directly, while routing
/// provider prompts through `cmd.exe /C` would turn untrusted prompt text into
/// shell syntax. We therefore resolve a standard npm shim either to its native
/// package executable or to `node.exe` plus its JavaScript entry point, and
/// preserve every later argument as a distinct process argument.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedCommand {
    /// Path shown in dependency diagnostics (the CLI itself or its npm shim).
    pub(super) discovered_path: PathBuf,
    /// Native executable passed to the OS (`node.exe` for an npm shim).
    pub(super) program: PathBuf,
    /// Validated arguments required before the caller's arguments.
    pub(super) prefix_args: Vec<OsString>,
}

impl ResolvedCommand {
    pub(crate) fn direct(path: PathBuf) -> Self {
        Self {
            discovered_path: path.clone(),
            program: path,
            prefix_args: Vec::new(),
        }
    }

    pub(crate) fn discovered_path(&self) -> &Path {
        &self.discovered_path
    }

    /// Resolve the native program before launching long-lived managed runtimes.
    ///
    /// Codex may re-exec its current binary as a sandbox helper. On macOS, a
    /// PATH entry is commonly a symlink into Codex's managed package directory;
    /// retaining that symlink as argv[0] can leave the helper outside the
    /// sandbox's executable allowlist even when the target itself is trusted.
    pub(crate) fn canonical_program(&self) -> Result<PathBuf, String> {
        std::fs::canonicalize(&self.program).map_err(|error| {
            format!(
                "Failed to resolve executable {}: {error}",
                self.program.display()
            )
        })
    }

    pub(crate) fn canonical_command<I, S>(&self, args: I) -> Result<Command, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(self.canonical_program()?);
        command.args(&self.prefix_args);
        command.args(args);
        Ok(command)
    }

    /// Construct a command without losing argument boundaries. Callers still
    /// apply their own cwd, stdio, PATH, and process-group configuration.
    pub(crate) fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.program);
        command.args(&self.prefix_args);
        command.args(args);
        command
    }
}

const DEFAULT_WINDOWS_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

pub(super) fn windows_pathexts(value: Option<&OsStr>) -> Vec<String> {
    let raw = value
        .and_then(|v| v.to_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(DEFAULT_WINDOWS_PATHEXT);
    let mut extensions = Vec::new();
    for value in raw.split(';') {
        let value = value.trim();
        if value.is_empty() || value.chars().any(|ch| matches!(ch, '/' | '\\' | ':')) {
            continue;
        }
        let extension = if value.starts_with('.') {
            value.to_string()
        } else {
            format!(".{value}")
        };
        if !extensions
            .iter()
            .any(|seen: &String| seen.eq_ignore_ascii_case(&extension))
        {
            extensions.push(extension);
        }
    }
    if extensions.is_empty() {
        windows_pathexts(Some(OsStr::new(DEFAULT_WINDOWS_PATHEXT)))
    } else {
        extensions
    }
}

/// The case-insensitive fallback lets the Windows resolver be unit-tested on
/// a case-sensitive filesystem and matches normal Windows path lookup.
fn existing_file(path: &Path, windows: bool) -> Option<PathBuf> {
    if path.is_file() {
        // Unix `which` semantics: a matching name without execute permission
        // must not shadow an executable later on PATH — returning None keeps
        // the directory scan going.
        #[cfg(unix)]
        if !windows && !unix_is_executable(path) {
            return None;
        }
        return Some(path.to_path_buf());
    }
    if !windows {
        return None;
    }
    let wanted = path.file_name()?.to_string_lossy();
    std::fs::read_dir(path.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&wanted)
                && entry.path().is_file()
        })
        .map(|entry| entry.path())
}

#[cfg(unix)]
fn unix_is_executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let Ok(cpath) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(cpath.as_ptr(), libc::X_OK) == 0 }
}

fn windows_relative_path(base: &Path, value: &str) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    let mut saw_component = false;
    for component in value.split(['\\', '/']).filter(|part| !part.is_empty()) {
        // npm's generated shims use a plain path below their own directory.
        // Reject interpolation and traversal instead of interpreting it.
        if component == "."
            || component == ".."
            || component.chars().any(|ch| ch == '%' || ch == ':')
            || component.contains('\0')
        {
            return None;
        }
        saw_component = true;
        path.push(component);
    }
    saw_component.then_some(path)
}

/// Extract the package entry point from npm's old (`%~dp0`) and current
/// (`%dp0%`) cmd-shim templates. Only an existing file below `node_modules`
/// is accepted; arbitrary batch files fail closed.
fn npm_entrypoint_from_shim(shim: &Path) -> Option<PathBuf> {
    const MAX_NPM_SHIM_BYTES: u64 = 64 * 1024;
    if shim.symlink_metadata().ok()?.len() > MAX_NPM_SHIM_BYTES {
        return None;
    }
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(shim).ok()?;
    let mut bytes = Vec::with_capacity(MAX_NPM_SHIM_BYTES as usize);
    file.take(MAX_NPM_SHIM_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_NPM_SHIM_BYTES as usize {
        return None;
    }
    let contents = String::from_utf8(bytes).ok()?;
    let lowercase = contents.to_ascii_lowercase();
    let base = shim.parent()?;

    for marker in ["%~dp0", "%dp0%"] {
        let mut cursor = 0;
        while let Some(offset) = lowercase[cursor..].find(marker) {
            let suffix_start = cursor + offset + marker.len();
            let remainder = &contents[suffix_start..];
            let Some(quote_offset) = remainder.find('"') else {
                break;
            };
            let suffix = &remainder[..quote_offset];
            let components = suffix.split(['\\', '/']).filter(|part| !part.is_empty());
            if components
                .clone()
                .any(|part| part.eq_ignore_ascii_case("node_modules"))
            {
                if let Some(entrypoint) = windows_relative_path(base, suffix) {
                    if entrypoint.is_file() {
                        return Some(entrypoint);
                    }
                }
            }
            cursor = suffix_start;
        }
    }
    None
}

fn is_native_windows_extension(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("com"))
        .unwrap_or(false)
}

fn find_windows_node(directories: &[PathBuf], extensions: &[String]) -> Option<PathBuf> {
    for directory in directories {
        for extension in extensions {
            if !extension.eq_ignore_ascii_case(".exe") && !extension.eq_ignore_ascii_case(".com") {
                continue;
            }
            let candidate = directory.join(format!("node{extension}"));
            if let Some(path) = existing_file(&candidate, true) {
                return Some(path);
            }
        }
    }
    None
}

fn resolve_windows_candidate(
    path: PathBuf,
    directories: &[PathBuf],
    extensions: &[String],
) -> Option<ResolvedCommand> {
    if is_native_windows_extension(&path) {
        return Some(ResolvedCommand::direct(path));
    }

    let extension = path.extension().and_then(OsStr::to_str)?;
    if !extension.eq_ignore_ascii_case("cmd") && !extension.eq_ignore_ascii_case("bat") {
        // Other PATHEXT entries require file associations or a command shell,
        // neither of which preserves arbitrary provider arguments safely.
        return None;
    }

    let entrypoint = npm_entrypoint_from_shim(&path)?;
    if is_native_windows_extension(&entrypoint) {
        return Some(ResolvedCommand {
            discovered_path: path,
            program: entrypoint,
            prefix_args: Vec::new(),
        });
    }
    let is_node_entrypoint = entrypoint
        .extension()
        .and_then(OsStr::to_str)
        .map(|extension| {
            extension.eq_ignore_ascii_case("js")
                || extension.eq_ignore_ascii_case("cjs")
                || extension.eq_ignore_ascii_case("mjs")
        })
        .unwrap_or(false);
    if !is_node_entrypoint {
        return None;
    }
    let local_node = path
        .parent()
        .and_then(|parent| existing_file(&parent.join("node.exe"), true));
    let program = local_node.or_else(|| find_windows_node(directories, extensions))?;
    Some(ResolvedCommand {
        discovered_path: path,
        program,
        prefix_args: vec![entrypoint.into_os_string()],
    })
}

pub(super) fn resolve_command_in(
    name: &str,
    directories: &[PathBuf],
    windows: bool,
    pathext: Option<&OsStr>,
) -> Option<ResolvedCommand> {
    if !windows {
        return directories.iter().find_map(|directory| {
            existing_file(&directory.join(name), false).map(ResolvedCommand::direct)
        });
    }

    let extensions = windows_pathexts(pathext);
    let has_extension = Path::new(name).extension().is_some();
    for directory in directories {
        if has_extension {
            if let Some(path) = existing_file(&directory.join(name), true) {
                if let Some(command) = resolve_windows_candidate(path, directories, &extensions) {
                    return Some(command);
                }
            }
            continue;
        }

        // PATHEXT candidates must win over an extensionless npm POSIX shim,
        // which is a shell script and is not launchable by CreateProcess.
        for extension in &extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            let Some(path) = existing_file(&candidate, true) else {
                continue;
            };
            if let Some(command) = resolve_windows_candidate(path, directories, &extensions) {
                return Some(command);
            }
        }
    }
    None
}

/// Resolve a PATH command to a representation that is safe to launch with
/// arbitrary arguments. On Windows this honors PATHEXT and unwraps npm shims.
pub(crate) fn resolve_command(name: &str) -> Option<ResolvedCommand> {
    let path = OsString::from(env::full_path());
    let current_dir = std::env::current_dir().ok();
    let directories: Vec<PathBuf> = std::env::split_paths(&path)
        // Preserve the prior resolver's behavior: empty PATH entries do not
        // implicitly grant execution from the app's current directory.
        .filter(|directory| !directory.as_os_str().is_empty())
        // Provider subprocesses change cwd before spawn. Resolve relative PATH
        // entries against the app cwd now so the retained target stays valid.
        .map(|directory| {
            if directory.is_relative() {
                current_dir
                    .as_ref()
                    .map(|cwd| cwd.join(&directory))
                    .unwrap_or(directory)
            } else {
                directory
            }
        })
        .collect();
    let pathext = std::env::var_os("PATHEXT");
    resolve_command_in(name, &directories, cfg!(windows), pathext.as_deref())
}

/// Find a launchable binary on PATH by scanning directories directly.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    resolve_command(name).map(|command| command.discovered_path)
}

pub(super) fn configure_probe_command(command: &mut Command) {
    command.env("PATH", env::full_path());
}

pub(super) fn probe_resolved(command: &ResolvedCommand, version_args: &[&str]) -> Option<String> {
    let mut process = command.command(version_args);
    configure_probe_command(&mut process);
    let output =
        crate::process::run_bounded(&mut process, PROBE_TIMEOUT, PROBE_OUTPUT_LIMIT).ok()?;
    if !output.status.success() || output.stdout_truncated || output.stderr_truncated {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let version = if stdout.is_empty() { stderr } else { stdout };
    let first_line = version.lines().next().unwrap_or("").trim();
    (!first_line.is_empty()).then(|| first_line.to_string())
}
