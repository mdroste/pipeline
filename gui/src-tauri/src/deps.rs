//! Dependency detection for the GUI startup check.

use crate::env;
use serde::Serialize;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct DepStatus {
    pub name: String,
    pub found: bool,
    pub version: String,
    pub path: String,
    pub required: bool,
    pub hint: String,
    /// Whether the provider has usable authentication, either through its CLI
    /// session or a configured direct-API key. None = not applicable or the
    /// result could not be verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated: Option<bool>,
    /// Sign-in state reported by the installed CLI itself. This stays
    /// separate from `authenticated`: a configured direct-API key can make a
    /// provider ready without saying anything about the CLI's own session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_auth_status: Option<CliAuthStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CliAuthStatus {
    SignedIn,
    SignedOut,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepsReport {
    pub deps: Vec<DepStatus>,
    pub ready: bool,
}

/// A command resolved to the exact program and fixed prefix arguments that can
/// actually be passed to `CreateProcess`/`execve`.
///
/// On Windows, npm exposes package binaries as `.cmd` shims. Rust's
/// `Command::new` cannot execute those scripts directly, while routing
/// provider prompts through `cmd.exe /C` would turn untrusted prompt text into
/// shell syntax. We therefore resolve a standard npm shim either to its native
/// package executable or to `node.exe` plus its JavaScript entry point, and
/// preserve every later argument as a distinct process argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedCommand {
    /// Path shown in dependency diagnostics (the CLI itself or its npm shim).
    discovered_path: PathBuf,
    /// Native executable passed to the OS (`node.exe` for an npm shim).
    program: PathBuf,
    /// Validated arguments required before the caller's arguments.
    prefix_args: Vec<OsString>,
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

fn windows_pathexts(value: Option<&OsStr>) -> Vec<String> {
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

fn resolve_command_in(
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

/// Harden an already-discovered command path (for managed tools). On Windows
/// this applies the same native-executable/npm-shim rules as PATH discovery;
/// arbitrary batch files are rejected.
pub(crate) fn resolve_discovered_command(path: PathBuf) -> Option<ResolvedCommand> {
    if !path.is_file() {
        return None;
    }
    if !cfg!(windows) {
        return Some(ResolvedCommand::direct(path));
    }
    let path_var = OsString::from(env::full_path());
    let mut directories: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    if let Some(parent) = path.parent() {
        directories.insert(0, parent.to_path_buf());
    }
    let extensions = windows_pathexts(std::env::var_os("PATHEXT").as_deref());
    resolve_windows_candidate(path, &directories, &extensions)
}

/// Find a launchable binary on PATH by scanning directories directly.
pub(crate) fn find_on_path(name: &str) -> Option<PathBuf> {
    resolve_command(name).map(|command| command.discovered_path)
}

fn configure_probe_command(command: &mut Command) {
    command.env("PATH", env::full_path());
}

fn probe_resolved(command: &ResolvedCommand, version_args: &[&str]) -> Option<String> {
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

struct ProbeResult {
    command: Option<ResolvedCommand>,
    version: String,
    path: String,
}

/// Resolve and run a version probe. A discovered path is retained for
/// diagnostics, but a spawn error or non-zero exit is not considered found.
fn probe(name: &str, version_args: &[&str]) -> ProbeResult {
    let Some(command) = resolve_command(name) else {
        return ProbeResult {
            command: None,
            version: String::new(),
            path: String::new(),
        };
    };
    let path = command.discovered_path().to_string_lossy().to_string();
    let Some(version) = probe_resolved(&command, version_args) else {
        return ProbeResult {
            command: None,
            version: String::new(),
            path,
        };
    };
    ProbeResult {
        command: Some(command),
        version,
        path,
    }
}

/// Check if Claude CLI is authenticated via `claude auth status`. Claude
/// returns JSON even when the user is signed out (with a non-zero exit code),
/// so parse the payload before considering process status. Execution or parse
/// failures are unknown, not evidence that the user signed out.
fn check_claude_auth(command: &ResolvedCommand) -> Option<bool> {
    let mut process = command.command(["auth", "status"]);
    configure_probe_command(&mut process);
    let output =
        crate::process::run_bounded(&mut process, PROBE_TIMEOUT, PROBE_OUTPUT_LIMIT).ok()?;
    if output.stdout_truncated || output.stderr_truncated {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(stdout.trim())
        .ok()
        .and_then(|value| {
            value
                .get("loggedIn")
                .and_then(|logged_in| logged_in.as_bool())
        })
}

/// Check if Codex CLI has credentials available.
/// Codex uses OPENAI_API_KEY env var or its own login system.
fn check_codex_auth(command: &ResolvedCommand) -> Option<bool> {
    // Check env var first
    if std::env::var("OPENAI_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
    {
        return Some(true);
    }
    let mut process = command.command(["login", "status"]);
    configure_probe_command(&mut process);
    let output =
        crate::process::run_bounded(&mut process, PROBE_TIMEOUT, PROBE_OUTPUT_LIMIT).ok()?;
    if output.stdout_truncated || output.stderr_truncated {
        return None;
    }
    if !output.status.success() {
        let diagnostic = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_ascii_lowercase();
        if diagnostic.contains("unrecognized")
            || diagnostic.contains("unexpected argument")
            || diagnostic.contains("unknown command")
        {
            return None;
        }
    }
    Some(output.status.success())
}

/// Check if Gemini CLI has credentials available.
/// Gemini uses GEMINI_API_KEY or GOOGLE_API_KEY env vars, or Google auth.
fn check_gemini_auth() -> Option<bool> {
    // Check env vars first
    if std::env::var("GEMINI_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
    {
        return Some(true);
    }
    if std::env::var("GOOGLE_API_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false)
    {
        return Some(true);
    }
    // Gemini CLI currently has no stable, noninteractive auth-status command.
    // A settings-file substring is not evidence of a readable, unexpired
    // credential, so report this state as unknown rather than green.
    None
}

fn cli_auth_status(found: bool, authenticated: Option<bool>) -> Option<CliAuthStatus> {
    found.then_some(match authenticated {
        Some(true) => CliAuthStatus::SignedIn,
        Some(false) => CliAuthStatus::SignedOut,
        None => CliAuthStatus::Unknown,
    })
}

/// Parse "http(s)://host[:port]/..." into (host, port) for a TCP probe.
/// Returns None for URLs we can't parse; the probe then reports unreachable.
fn parse_host_port(base_url: &str) -> Option<(String, u16)> {
    let rest = base_url
        .trim()
        .strip_prefix("http://")
        .or_else(|| base_url.trim().strip_prefix("https://"))?;
    let default_port = if base_url.trim_start().starts_with("https://") {
        443
    } else {
        80
    };
    let authority = rest.split('/').next()?;
    if authority.is_empty() {
        return None;
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port.parse::<u16>().ok()?;
            if host.is_empty() {
                return None;
            }
            Some((host.to_string(), port))
        }
        None => Some((authority.to_string(), default_port)),
    }
}

/// Protocol-level probe for the local OpenAI-compatible server. A listener is
/// ready only if `/v1/models` returns a bounded OpenAI-style model list.
fn probe_local_server(base_url: &str, api_key: &str) -> (bool, String) {
    let Some((host, port)) = parse_host_port(base_url) else {
        return (false, String::new());
    };
    let desc = format!("{host}:{port}");
    let models_url = if base_url.trim_end_matches('/').ends_with("/v1") {
        format!("{}/models", base_url.trim_end_matches('/'))
    } else {
        format!("{}/v1/models", base_url.trim_end_matches('/'))
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return (false, desc),
    };
    let reachable = runtime.block_on(async {
        let mut request = crate::pipeline::api_common::HTTP_CLIENT
            .get(models_url)
            .timeout(std::time::Duration::from_secs(3));
        if !api_key.is_empty() {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await.ok()?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > 1024 * 1024)
        {
            return None;
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if chunk.len() > 1024 * 1024usize.saturating_sub(body.len()) {
                return None;
            }
            body.extend_from_slice(&chunk);
        }
        let value: serde_json::Value = serde_json::from_slice(&body).ok()?;
        value
            .get("data")
            .and_then(|data| data.as_array())
            .map(|_| ())
    });
    (reachable.is_some(), desc)
}

/// Run all dependency checks in parallel. Provider CLIs are still probed when
/// a direct-API key is configured so their own sign-in state can be reported
/// separately. Uses existence checks instead of --version/--help for optional
/// tools.
pub fn check_all() -> DepsReport {
    let settings = crate::settings::load_persisted();
    let provider = settings.preferred_provider.clone();
    let has_anthropic_key = !settings.anthropic_api_key.is_empty();
    let has_openai_key = !settings.openai_api_key.is_empty();
    let has_google_key = !settings.google_api_key.is_empty();
    let local_base_url = settings.local_base_url.clone();
    let local_api_key = settings.local_api_key.clone();

    // Run all independent probes in parallel.
    std::thread::scope(|s| {
        // Probe each CLI and its own sign-in state even if a direct API key
        // independently makes that provider ready.
        let claude_h = s.spawn(move || {
            let ProbeResult {
                command,
                version,
                path,
            } = probe("claude", &["--version"]);
            let auth = command.as_ref().and_then(check_claude_auth);
            (command.is_some(), version, path, auth)
        });

        let codex_h = s.spawn(move || {
            let ProbeResult {
                command,
                version,
                path,
            } = probe("codex", &["--version"]);
            let auth = command.as_ref().and_then(check_codex_auth);
            (command.is_some(), version, path, auth)
        });

        let gemini_h = s.spawn(move || {
            let ProbeResult {
                command,
                version,
                path,
            } = probe("gemini", &["--version"]);
            let auth = command.as_ref().and_then(|_| check_gemini_auth());
            (command.is_some(), version, path, auth)
        });

        // pdftoppm: used by Claude Code's Read tool to render PDF pages.
        // Normally bundled; falls back to system poppler if present.
        let pdftoppm_h = s.spawn(|| find_on_path("pdftoppm"));

        // pdftotext: just check existence (no subprocess needed)
        let pdftotext_h = s.spawn(|| find_on_path("pdftotext"));

        // marker_single: just check existence (--help would spawn Python, very
        // slow). A managed install (~/.pipeline/bin) wins over PATH, matching
        // the resolution order in extract.rs.
        let marker_h = s.spawn(|| {
            crate::engines::find_managed("marker_single").or_else(|| find_on_path("marker_single"))
        });

        // Local OpenAI-compatible server: TCP reachability of the configured
        // base URL (fast, no HTTP parse — a listener there is a good signal).
        let local_url = local_base_url.clone();
        let local_h = s.spawn(move || probe_local_server(&local_url, &local_api_key));

        // Collect results
        let (found, ver, path, claude_auth) = claude_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let claude_hint = if has_anthropic_key {
            "API key configured — CLI not required."
        } else if found && claude_auth == Some(false) {
            "Claude CLI is installed but not signed in. Run `claude auth login` to authenticate."
        } else if found && claude_auth.is_none() {
            "Claude CLI is installed, but authentication status could not be verified. Run `claude auth status`."
        } else {
            "Install Claude Code: npm install -g @anthropic-ai/claude-code"
        };
        let claude = DepStatus {
            name: "Claude CLI".into(),
            found: found || has_anthropic_key,
            version: if has_anthropic_key && !found {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: provider == "claude",
            hint: claude_hint.into(),
            authenticated: if has_anthropic_key {
                Some(true)
            } else {
                claude_auth
            },
            cli_auth_status: cli_auth_status(found, claude_auth),
        };

        let (found, ver, path, codex_auth) = codex_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let codex_hint = if has_openai_key {
            "API key configured — CLI not required."
        } else if found && codex_auth == Some(false) {
            "Codex CLI is installed but not authenticated. Run `codex login` or set OPENAI_API_KEY."
        } else if found && codex_auth.is_none() {
            "Codex CLI is installed, but authentication status could not be verified. Run `codex login status`."
        } else {
            "Install Codex CLI: npm install -g @openai/codex"
        };
        let codex = DepStatus {
            name: "Codex CLI".into(),
            found: found || has_openai_key,
            version: if has_openai_key && !found {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: provider == "codex",
            hint: codex_hint.into(),
            authenticated: if has_openai_key {
                Some(true)
            } else {
                codex_auth
            },
            cli_auth_status: cli_auth_status(found, codex_auth),
        };

        let (found, ver, path, gemini_auth) = gemini_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let gemini_hint = if has_google_key {
            "API key configured — CLI not required."
        } else if found && gemini_auth == Some(false) {
            "Gemini CLI is installed but not authenticated. Set GEMINI_API_KEY or run `gemini` to log in."
        } else if found && gemini_auth.is_none() {
            "Gemini CLI is installed, but authentication cannot be verified noninteractively. Set GEMINI_API_KEY/GOOGLE_API_KEY or verify login before running."
        } else {
            "Install Gemini CLI: npm install -g @google/gemini-cli"
        };
        let gemini = DepStatus {
            name: "Gemini CLI".into(),
            found: found || has_google_key,
            version: if has_google_key && !found {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: provider == "gemini",
            hint: gemini_hint.into(),
            authenticated: if has_google_key {
                Some(true)
            } else {
                gemini_auth
            },
            cli_auth_status: cli_auth_status(found, gemini_auth),
        };

        // Classify a found binary as bundled (under our resource dir) or system.
        let bundled_dir = env::bundled_poppler_dir();
        let classify = |p: &PathBuf| -> &'static str {
            match bundled_dir {
                Some(dir) if p.starts_with(dir) => "bundled",
                _ => "system",
            }
        };
        let install_hint = if cfg!(target_os = "macos") {
            "brew install poppler"
        } else if cfg!(target_os = "windows") {
            "Install poppler: scoop install poppler"
        } else {
            "Install poppler-utils via your package manager"
        };

        let pdftoppm_path = pdftoppm_h.join().unwrap_or(None);
        let pdftoppm = DepStatus {
            name: "pdftoppm".into(),
            found: pdftoppm_path.is_some(),
            version: pdftoppm_path
                .as_ref()
                .map(|p| classify(p).into())
                .unwrap_or_default(),
            path: pdftoppm_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            required: false,
            hint: if pdftoppm_path.is_some() {
                "Used by Claude Code to read PDFs. Bundled with Pipeline.".into()
            } else {
                format!("{install_hint} — needed for PDF support in the LLM Read tool.")
            },
            authenticated: None,
            cli_auth_status: None,
        };

        let pdftotext_path = pdftotext_h.join().unwrap_or(None);
        let pdftotext = DepStatus {
            name: "pdftotext".into(),
            found: pdftotext_path.is_some(),
            version: pdftotext_path
                .as_ref()
                .map(|p| classify(p).into())
                .unwrap_or_default(),
            path: pdftotext_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            required: false,
            hint: if pdftotext_path.is_some() {
                "Native PDF text fallback. Bundled with Pipeline.".into()
            } else {
                format!("{install_hint} — needed for the pdftotext extraction fallback.")
            },
            authenticated: None,
            cli_auth_status: None,
        };

        let marker_path = marker_h.join().unwrap_or(None);
        let marker_managed = marker_path
            .as_ref()
            .zip(crate::engines::managed_bin_dir())
            .map(|(p, dir)| p.starts_with(&dir))
            .unwrap_or(false);
        let marker_hint = if marker_managed {
            "Managed install (~/.pipeline) — this copy is used for extraction."
        } else if marker_path.is_some() {
            "System install on PATH — used only because no managed install exists. \
             Installing from Settings → Text Extraction takes precedence."
        } else {
            "Optional local PDF equation extraction. Install from Settings → Text Extraction."
        };
        let marker = DepStatus {
            name: "marker-pdf".into(),
            found: marker_path.is_some(),
            version: match (&marker_path, marker_managed) {
                (Some(_), true) => "managed".into(),
                (Some(_), false) => "system".into(),
                (None, _) => String::new(),
            },
            path: marker_path
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            required: false,
            hint: marker_hint.into(),
            authenticated: None,
            cli_auth_status: None,
        };

        let (local_reachable, local_desc) = local_h.join().unwrap_or((false, String::new()));
        let local = DepStatus {
            name: "Local LLM server".into(),
            found: local_reachable,
            version: if local_reachable {
                "reachable".into()
            } else {
                String::new()
            },
            path: local_desc,
            required: provider == "local",
            hint: if local_reachable {
                "OpenAI-compatible server responding at the configured URL.".into()
            } else {
                "No server at the configured URL. Install Ollama (ollama.com), run it, \
                 and pull a model (e.g. `ollama pull llama3.3`), or point Settings → Models \
                 at another OpenAI-compatible server."
                    .into()
            },
            authenticated: None,
            cli_auth_status: None,
        };

        let deps = vec![claude, codex, gemini, local, pdftoppm, pdftotext, marker];
        let ready = deps.iter().all(|d| {
            if !d.required {
                return true;
            }
            if !d.found {
                return false;
            }
            d.authenticated
                .map_or(d.name == "Local LLM server", |authenticated| authenticated)
        });
        DepsReport { deps, ready }
    })
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{check_claude_auth, probe_resolved};
    use super::{
        cli_auth_status, parse_host_port, resolve_command_in, windows_pathexts, CliAuthStatus,
    };
    use std::ffi::{OsStr, OsString};
    use std::path::Path;

    fn write_fixture(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn assert_windows_path_eq(actual: &Path, expected: &Path) {
        assert!(
            actual
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.to_string_lossy()),
            "{} != {}",
            actual.display(),
            expected.display()
        );
    }

    fn write_node_shim(path: &Path, package_entrypoint: &str) {
        write_fixture(
            path,
            &format!(
                r#"@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
IF EXIST "%dp0%\node.exe" (
  SET "_prog=%dp0%\node.exe"
) ELSE (
  SET "_prog=node"
  SET PATHEXT=%PATHEXT:;.JS;=;%
)
endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & "%_prog%"  "%dp0%\{package_entrypoint}" %*
"#
            ),
        );
    }

    #[test]
    fn parse_ollama_default() {
        assert_eq!(
            parse_host_port("http://localhost:11434/v1"),
            Some(("localhost".into(), 11434))
        );
    }

    #[test]
    fn parse_no_port_defaults_by_scheme() {
        assert_eq!(
            parse_host_port("http://myhost/v1"),
            Some(("myhost".into(), 80))
        );
        assert_eq!(
            parse_host_port("https://myhost/v1"),
            Some(("myhost".into(), 443))
        );
    }

    #[test]
    fn parse_no_path() {
        assert_eq!(
            parse_host_port("http://127.0.0.1:1234"),
            Some(("127.0.0.1".into(), 1234))
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert_eq!(parse_host_port(""), None);
        assert_eq!(parse_host_port("localhost:11434"), None); // no scheme
        assert_eq!(parse_host_port("http://"), None);
        assert_eq!(parse_host_port("http://host:notaport/v1"), None);
    }

    #[test]
    fn windows_js_npm_shim_resolves_to_node_and_preserves_untrusted_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let npm_dir = temp.path().join("npm global with spaces");
        let node_dir = temp.path().join("Node Runtime");
        let shim = npm_dir.join("codex.CMD");
        let entrypoint = npm_dir
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin")
            .join("codex.js");
        let node = node_dir.join("node.EXE");
        write_fixture(&npm_dir.join("codex"), "#!/bin/sh\nexit 99\n");
        write_fixture(&entrypoint, "// fixture");
        write_fixture(&node, "fixture");
        write_node_shim(&shim, r"node_modules\@openai\codex\bin\codex.js");

        let directories = vec![npm_dir, node_dir];
        let resolved =
            resolve_command_in("codex", &directories, true, Some(OsStr::new(".cmd;.EXE"))).unwrap();

        assert_windows_path_eq(&resolved.discovered_path, &shim);
        assert_windows_path_eq(&resolved.program, &node);
        assert_eq!(
            resolved.prefix_args,
            vec![entrypoint.clone().into_os_string()]
        );

        // These strings would be shell syntax if concatenated into cmd /C.
        // The resolved representation keeps each one as an opaque argument.
        let caller_args = vec![
            "hello & whoami | more".to_string(),
            "a quoted \"value\"\nand a second line".to_string(),
            "%PATH% !PROMPT! ^ <input >output".to_string(),
        ];
        let command = resolved.command(&caller_args);
        assert_windows_path_eq(Path::new(command.get_program()), &node);
        let actual_args: Vec<OsString> = command.get_args().map(OsString::from).collect();
        let mut expected_args = vec![entrypoint.into_os_string()];
        expected_args.extend(caller_args.into_iter().map(OsString::from));
        assert_eq!(actual_args, expected_args);
    }

    #[test]
    fn windows_pathext_order_and_case_are_honored() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("Provider Bin");
        let shim = bin.join("gemini.CMD");
        let executable = bin.join("gemini.EXE");
        let entrypoint = bin
            .join("node_modules")
            .join("@google")
            .join("gemini-cli")
            .join("dist")
            .join("index.js");
        let node = bin.join("node.exe");
        write_fixture(&executable, "fixture");
        write_fixture(&entrypoint, "// fixture");
        write_fixture(&node, "fixture");
        write_node_shim(&shim, r"node_modules\@google\gemini-cli\dist\index.js");
        let directories = vec![bin];

        let exe_first =
            resolve_command_in("gemini", &directories, true, Some(OsStr::new(".exe;.CMD")))
                .unwrap();
        assert_windows_path_eq(&exe_first.discovered_path, &executable);
        assert_windows_path_eq(&exe_first.program, &executable);

        let cmd_first =
            resolve_command_in("gemini", &directories, true, Some(OsStr::new(".cmd;.EXE")))
                .unwrap();
        assert_windows_path_eq(&cmd_first.discovered_path, &shim);
        assert_windows_path_eq(&cmd_first.program, &node);
    }

    #[test]
    fn windows_native_npm_shim_launches_target_without_node() {
        let temp = tempfile::tempdir().unwrap();
        let npm_dir = temp.path().join("npm global");
        let shim = npm_dir.join("claude.cmd");
        let executable = npm_dir
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("bin")
            .join("claude.exe");
        write_fixture(&executable, "native fixture");
        write_fixture(
            &shim,
            r#"@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
"%dp0%\node_modules\@anthropic-ai\claude-code\bin\claude.exe"   %*
"#,
        );

        let resolved =
            resolve_command_in("claude", &[npm_dir], true, Some(OsStr::new(".CMD"))).unwrap();
        assert_windows_path_eq(&resolved.discovered_path, &shim);
        assert_windows_path_eq(&resolved.program, &executable);
        assert!(resolved.prefix_args.is_empty());
    }

    #[test]
    fn windows_old_npm_shim_template_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let npm_dir = temp.path().join("legacy npm");
        let shim = npm_dir.join("gemini.cmd");
        let node = npm_dir.join("node.exe");
        let entrypoint = npm_dir
            .join("node_modules")
            .join("@google")
            .join("gemini-cli")
            .join("dist")
            .join("index.js");
        write_fixture(&node, "fixture");
        write_fixture(&entrypoint, "// fixture");
        write_fixture(
            &shim,
            r#"@IF EXIST "%~dp0\node.exe" (
  "%~dp0\node.exe" "%~dp0\node_modules\@google\gemini-cli\dist\index.js" %*
) ELSE (
  node "%~dp0\node_modules\@google\gemini-cli\dist\index.js" %*
)
"#,
        );

        let resolved =
            resolve_command_in("gemini", &[npm_dir], true, Some(OsStr::new(".CMD;.EXE"))).unwrap();
        assert_windows_path_eq(&resolved.discovered_path, &shim);
        assert_windows_path_eq(&resolved.program, &node);
        assert_eq!(resolved.prefix_args, vec![entrypoint.into_os_string()]);
    }

    #[test]
    fn windows_rejects_unrecognized_batch_shims() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        write_fixture(&bin.join("claude.cmd"), "@echo off\necho %*\n");
        assert!(resolve_command_in("claude", &[bin], true, Some(OsStr::new(".CMD")),).is_none());
    }

    #[test]
    fn windows_pathext_defaults_match_create_process_conventions() {
        assert_eq!(windows_pathexts(None), vec![".COM", ".EXE", ".BAT", ".CMD"]);
        assert_eq!(
            windows_pathexts(Some(OsStr::new("cmd; .EXE; .cmd"))),
            vec![".cmd", ".EXE"]
        );
    }

    #[test]
    #[cfg(unix)]
    fn version_and_auth_probes_fail_closed() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin with spaces");
        let failing = bin.join("failing-provider");
        write_fixture(&failing, "#!/bin/sh\nexit 7\n");
        std::fs::set_permissions(&failing, std::fs::Permissions::from_mode(0o755)).unwrap();
        let command =
            resolve_command_in("failing-provider", std::slice::from_ref(&bin), false, None)
                .unwrap();
        assert_eq!(probe_resolved(&command, &["--version"]), None);

        let empty_version = bin.join("empty-version");
        write_fixture(&empty_version, "#!/bin/sh\nexit 0\n");
        std::fs::set_permissions(&empty_version, std::fs::Permissions::from_mode(0o755)).unwrap();
        let command =
            resolve_command_in("empty-version", std::slice::from_ref(&bin), false, None).unwrap();
        assert_eq!(probe_resolved(&command, &["--version"]), None);

        let malformed_auth = bin.join("malformed-auth");
        write_fixture(&malformed_auth, "#!/bin/sh\nprintf 'not-json\\n'\n");
        std::fs::set_permissions(&malformed_auth, std::fs::Permissions::from_mode(0o755)).unwrap();
        let command = resolve_command_in("malformed-auth", &[bin], false, None).unwrap();
        assert_eq!(check_claude_auth(&command), None);
    }

    #[test]
    #[cfg(unix)]
    fn claude_auth_uses_json_state_even_when_signed_out_exits_nonzero() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        let signed_out = bin.join("signed-out");
        write_fixture(
            &signed_out,
            "#!/bin/sh\nprintf '{\"loggedIn\":false}\\n'\nexit 1\n",
        );
        std::fs::set_permissions(&signed_out, std::fs::Permissions::from_mode(0o755)).unwrap();
        let command = resolve_command_in("signed-out", &[bin], false, None).unwrap();
        assert_eq!(check_claude_auth(&command), Some(false));
    }

    #[test]
    fn cli_auth_status_is_tri_state_and_only_applies_to_installed_clis() {
        assert_eq!(cli_auth_status(false, None), None);
        assert_eq!(
            cli_auth_status(true, Some(true)),
            Some(CliAuthStatus::SignedIn)
        );
        assert_eq!(
            cli_auth_status(true, Some(false)),
            Some(CliAuthStatus::SignedOut)
        );
        assert_eq!(cli_auth_status(true, None), Some(CliAuthStatus::Unknown));
    }
}
