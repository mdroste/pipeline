//! Dependency detection for the GUI startup check.

use crate::env;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct DepStatus {
    pub name: String,
    pub found: bool,
    pub version: String,
    pub path: String,
    pub required: bool,
    pub hint: String,
    /// Whether the CLI is authenticated / signed in.
    /// None = not applicable or not checked (e.g. binary not found).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authenticated: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepsReport {
    pub deps: Vec<DepStatus>,
    pub ready: bool,
}

fn cmd(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env("PATH", env::full_path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}

/// Find a binary on PATH by scanning directories directly (no subprocess).
fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = env::full_path();
    let sep = if cfg!(windows) { ';' } else { ':' };
    for dir in path_var.split(sep) {
        if dir.is_empty() {
            continue;
        }
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        // On Windows, also check common extensions
        #[cfg(windows)]
        for ext in &[".exe", ".cmd", ".bat", ".com"] {
            let with_ext = PathBuf::from(format!("{}{}", candidate.display(), ext));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

/// Probe a command: find it on PATH (no subprocess), then run version command.
/// Returns (found, version_string, path).
fn probe(name: &str, version_args: &[&str]) -> (bool, String, String) {
    let bin_path = match find_on_path(name) {
        Some(p) => p,
        None => return (false, String::new(), String::new()),
    };
    let path_str = bin_path.to_string_lossy().to_string();
    match cmd(name).args(version_args).output() {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            let ver = if out.is_empty() { err } else { out };
            (true, ver.lines().next().unwrap_or("").to_string(), path_str)
        }
        // Binary exists but version command failed — still report as found
        _ => (true, String::new(), path_str),
    }
}

/// Check if Claude CLI is authenticated via `claude auth status`.
fn check_claude_auth() -> Option<bool> {
    let output = cmd("claude").args(["auth", "status"]).output().ok()?;
    if !output.status.success() {
        return Some(false);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse JSON response: { "loggedIn": true, ... }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout.trim()) {
        return val.get("loggedIn").and_then(|v| v.as_bool());
    }
    None
}

/// Check if Codex CLI has credentials available.
/// Codex uses OPENAI_API_KEY env var or its own login system.
fn check_codex_auth() -> Option<bool> {
    // Check env var first
    if std::env::var("OPENAI_API_KEY").map(|k| !k.is_empty()).unwrap_or(false) {
        return Some(true);
    }
    // Check for codex config directory with stored credentials
    if let Some(home) = dirs::home_dir() {
        let config = home.join(".codex").join("auth.json");
        if config.exists() {
            return Some(true);
        }
    }
    // Neither the env var nor the standard credential file is present, so
    // report "not authenticated" — that drives the "run `codex login`" hint
    // in the deps dialog. (This is only reached when the CLI is installed;
    // the caller passes None for the not-installed case.)
    Some(false)
}

/// Check if Gemini CLI has credentials available.
/// Gemini uses GEMINI_API_KEY or GOOGLE_API_KEY env vars, or Google auth.
fn check_gemini_auth() -> Option<bool> {
    // Check env vars first
    if std::env::var("GEMINI_API_KEY").map(|k| !k.is_empty()).unwrap_or(false) {
        return Some(true);
    }
    if std::env::var("GOOGLE_API_KEY").map(|k| !k.is_empty()).unwrap_or(false) {
        return Some(true);
    }
    // Check for Gemini config with stored credentials
    if let Some(home) = dirs::home_dir() {
        let settings = home.join(".gemini").join("settings.json");
        if settings.exists() {
            if let Ok(content) = std::fs::read_to_string(&settings) {
                // If settings exist and contain an API key or auth config, consider authenticated
                if content.contains("apiKey") || content.contains("oauth") {
                    return Some(true);
                }
            }
        }
    }
    Some(false)
}

/// Run all dependency checks in parallel.
/// Skips subprocess probes when an API key already covers a provider.
/// Uses existence checks (which) instead of --version/--help for optional tools.
pub fn check_all() -> DepsReport {
    let settings = crate::settings::load();
    let provider = settings.preferred_provider.clone();
    let has_anthropic_key = !settings.anthropic_api_key.is_empty();
    let has_openai_key = !settings.openai_api_key.is_empty();
    let has_google_key = !settings.google_api_key.is_empty();

    // Run all probes in parallel, skipping unnecessary work
    std::thread::scope(|s| {
        // Claude: probe CLI, skip auth check if API key covers it
        let claude_h = s.spawn(move || {
            let (found, ver, path) = probe("claude", &["--version"]);
            let auth = if found { check_claude_auth() } else { None };
            (found, ver, path, auth)
        });

        // Codex: probe CLI, skip auth check if API key covers it
        let codex_h = s.spawn(move || {
            let (found, ver, path) = probe("codex", &["--version"]);
            let auth = if found { check_codex_auth() } else { None };
            (found, ver, path, auth)
        });

        // Gemini: probe CLI, skip auth check if API key covers it
        let gemini_h = s.spawn(move || {
            let (found, ver, path) = probe("gemini", &["--version"]);
            let auth = if found { check_gemini_auth() } else { None };
            (found, ver, path, auth)
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

        // Collect results
        let (found, ver, path, claude_auth) = claude_h.join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let claude_hint = if has_anthropic_key {
            "API key configured — CLI not required."
        } else if found && claude_auth == Some(false) {
            "Claude CLI is installed but not signed in. Run `claude auth login` to authenticate."
        } else {
            "Install Claude Code: npm install -g @anthropic-ai/claude-code"
        };
        let claude = DepStatus {
            name: "Claude CLI".into(),
            found: found || has_anthropic_key,
            version: if has_anthropic_key && !found { "direct API".into() } else { ver },
            path,
            required: provider == "claude",
            hint: claude_hint.into(),
            authenticated: if has_anthropic_key { Some(true) } else { claude_auth },
        };

        let (found, ver, path, codex_auth) = codex_h.join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let codex_hint = if has_openai_key {
            "API key configured — CLI not required."
        } else if found && codex_auth == Some(false) {
            "Codex CLI is installed but not authenticated. Run `codex login` or set OPENAI_API_KEY."
        } else {
            "Install Codex CLI: npm install -g @openai/codex"
        };
        let codex = DepStatus {
            name: "Codex CLI".into(),
            found: found || has_openai_key,
            version: if has_openai_key && !found { "direct API".into() } else { ver },
            path,
            required: provider == "codex",
            hint: codex_hint.into(),
            authenticated: if has_openai_key { Some(true) } else { codex_auth },
        };

        let (found, ver, path, gemini_auth) = gemini_h.join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let gemini_hint = if has_google_key {
            "API key configured — CLI not required."
        } else if found && gemini_auth == Some(false) {
            "Gemini CLI is installed but not authenticated. Set GEMINI_API_KEY or run `gemini` to log in."
        } else {
            "Install Gemini CLI: npm install -g @google/gemini-cli"
        };
        let gemini = DepStatus {
            name: "Gemini CLI".into(),
            found: found || has_google_key,
            version: if has_google_key && !found { "direct API".into() } else { ver },
            path,
            required: provider == "gemini",
            hint: gemini_hint.into(),
            authenticated: if has_google_key { Some(true) } else { gemini_auth },
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
            version: pdftoppm_path.as_ref().map(|p| classify(p).into()).unwrap_or_default(),
            path: pdftoppm_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            required: false,
            hint: if pdftoppm_path.is_some() {
                "Used by Claude Code to read PDFs. Bundled with Pipeline.".into()
            } else {
                format!("{install_hint} — needed for PDF support in the LLM Read tool.")
            },
            authenticated: None,
        };

        let pdftotext_path = pdftotext_h.join().unwrap_or(None);
        let pdftotext = DepStatus {
            name: "pdftotext".into(),
            found: pdftotext_path.is_some(),
            version: pdftotext_path.as_ref().map(|p| classify(p).into()).unwrap_or_default(),
            path: pdftotext_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            required: false,
            hint: if pdftotext_path.is_some() {
                "Native PDF text fallback. Bundled with Pipeline.".into()
            } else {
                format!("{install_hint} — needed for the pdftotext extraction fallback.")
            },
            authenticated: None,
        };

        let marker_path = marker_h.join().unwrap_or(None);
        let marker_managed = marker_path
            .as_ref()
            .zip(crate::engines::managed_bin_dir())
            .map(|(p, dir)| p.starts_with(&dir))
            .unwrap_or(false);
        let marker = DepStatus {
            name: "marker-pdf".into(),
            found: marker_path.is_some(),
            version: match (&marker_path, marker_managed) {
                (Some(_), true) => "managed".into(),
                (Some(_), false) => "system".into(),
                (None, _) => String::new(),
            },
            path: marker_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            required: false,
            hint: "Optional local PDF equation extraction. Install from Settings, or pip install marker-pdf.".into(),
            authenticated: None,
        };

        let deps = vec![claude, codex, gemini, pdftoppm, pdftotext, marker];
        let ready = deps.iter().all(|d| {
            if !d.required { return true; }
            if !d.found { return false; }
            d.authenticated != Some(false)
        });
        DepsReport { deps, ready }
    })
}
