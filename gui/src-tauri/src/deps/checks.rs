use super::*;

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

/// Minimum agy release whose headless mode honors `--mode`, `--model`, and
/// `--effort` and whose print envelope carries usage accounting. Older
/// releases silently ignored those flags, so they fail the dependency check.
const ANTIGRAVITY_MIN_VERSION: [u64; 3] = [1, 1, 12];

/// Signed-in `agy models` fetches the list over the network, so it gets a
/// longer bound than the local version probes.
const ANTIGRAVITY_AUTH_PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Parse agy's bare `X.Y.Z` version output and gate on the minimum release.
/// Unparseable output is unknown, not evidence either way.
pub(super) fn antigravity_version_supported(version: &str) -> Option<bool> {
    let mut parts = version.trim().split('.');
    let mut parsed = [0u64; 3];
    for slot in parsed.iter_mut() {
        *slot = parts.next()?.trim().parse().ok()?;
    }
    Some(parsed >= ANTIGRAVITY_MIN_VERSION)
}

/// Check if Claude CLI is authenticated via `claude auth status`. Claude
/// returns JSON even when the user is signed out (with a non-zero exit code),
/// so parse the payload before considering process status. Execution or parse
/// failures are unknown, not evidence that the user signed out.
pub(super) fn check_claude_auth(command: &ResolvedCommand) -> Option<bool> {
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

/// Check the Antigravity CLI's own sign-in state. `agy models` is free and
/// deterministic: signed in it lists models and exits 0; signed out it fails
/// fast with a "please sign in" diagnostic. A `-p` probe would instead start
/// an interactive OAuth wait (~60s stall printing a login URL), so print mode
/// is never used for probing.
pub(super) fn check_antigravity_auth(command: &ResolvedCommand) -> Option<bool> {
    let mut process = command.command(["models"]);
    configure_probe_command(&mut process);
    let output = crate::process::run_bounded(
        &mut process,
        ANTIGRAVITY_AUTH_PROBE_TIMEOUT,
        PROBE_OUTPUT_LIMIT,
    )
    .ok()?;
    if output.stdout_truncated || output.stderr_truncated {
        return None;
    }
    if output.status.success() {
        return Some(true);
    }
    let diagnostic = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    if diagnostic.contains("sign in") || diagnostic.contains("authentication") {
        return Some(false);
    }
    None
}

pub(super) fn cli_auth_status(found: bool, authenticated: Option<bool>) -> Option<CliAuthStatus> {
    found.then_some(match authenticated {
        Some(true) => CliAuthStatus::SignedIn,
        Some(false) => CliAuthStatus::SignedOut,
        None => CliAuthStatus::Unknown,
    })
}

/// Parse "http(s)://host[:port]/..." into (host, port) for a TCP probe.
/// Returns None for URLs we can't parse; the probe then reports unreachable.
pub(super) fn parse_host_port(base_url: &str) -> Option<(String, u16)> {
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
    if crate::settings::validate_local_base_url(base_url).is_err() {
        return (false, String::new());
    }
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
        let mut request = crate::pipeline::api_common::custom_endpoint_client(base_url)
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
            if chunk.len() > (1024 * 1024usize).saturating_sub(body.len()) {
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

fn effective_pdf_extractor<'a>(
    settings: &'a crate::settings::Settings,
    config: &'a crate::pipeline_config::PipelineConfig,
) -> &'a str {
    let method = config.extraction.method.trim();
    let configured = if method.is_empty() || method == "auto" {
        settings.pdf_extractor.as_str()
    } else {
        method
    };
    crate::settings::resolve_pdf_extractor(configured)
}

fn document_path_may_need_pdf(path: &str) -> bool {
    let path = std::path::Path::new(path);
    if path.is_dir() {
        // A primary directory is ingested as a folder. Named document inputs
        // normally come from a file picker; treating a directory as possibly
        // PDF-backed is the conservative choice for direct IPC callers.
        return true;
    }
    !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("tex") || extension.eq_ignore_ascii_case("docx")
        })
}

pub(super) fn pdf_extraction_may_run(
    config: &crate::pipeline_config::PipelineConfig,
    input_path: Option<&str>,
    extra_inputs: Option<&std::collections::HashMap<String, String>>,
) -> bool {
    let primary_may_need_pdf = input_path
        .map(|path| {
            crate::pipeline::extract::effective_input_mode(&config.extraction.input_mode, path)
                == "document"
                && document_path_may_need_pdf(path)
        })
        .unwrap_or_else(|| !matches!(config.extraction.input_mode.trim(), "folder" | "none"));
    if primary_may_need_pdf {
        return true;
    }

    let Some(extra_inputs) = extra_inputs else {
        return false;
    };
    config.extraction.extra_inputs.iter().any(|slot| {
        slot.mode != "folder"
            && extra_inputs
                .get(&slot.key)
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .is_some_and(document_path_may_need_pdf)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PdfDependencyRequirements {
    pub(super) extraction: bool,
    pub(super) pdftotext: bool,
    pub(super) pdftoppm: bool,
    pub(super) paddle_full: bool,
    pub(super) configuration_ready: bool,
}

pub(super) fn pdf_dependency_requirements(
    settings: &crate::settings::Settings,
    config: Option<&crate::pipeline_config::PipelineConfig>,
    input_path: Option<&str>,
    extra_inputs: Option<&std::collections::HashMap<String, String>>,
) -> PdfDependencyRequirements {
    let Some(config) = config else {
        return PdfDependencyRequirements {
            extraction: false,
            pdftotext: false,
            pdftoppm: false,
            paddle_full: false,
            configuration_ready: true,
        };
    };
    let extraction = pdf_extraction_may_run(config, input_path, extra_inputs);
    let method = effective_pdf_extractor(settings, config);
    PdfDependencyRequirements {
        extraction,
        // LLM extraction treats the pdftotext page map as a mandatory
        // completeness check rather than a best-effort enhancement.
        pdftotext: extraction && matches!(method, "llm" | "pdftotext"),
        pdftoppm: false,
        paddle_full: extraction && method == "paddleocr-vl-full",
        configuration_ready: !extraction
            || matches!(method, "llm" | "paddleocr-vl-full" | "pdftotext")
                && !(method == "llm" && settings.preferred_provider == "local"),
    }
}

pub(super) fn required_providers(
    settings: &crate::settings::Settings,
    config: Option<&crate::pipeline_config::PipelineConfig>,
    diff: bool,
    input_path: Option<&str>,
    extra_inputs: Option<&std::collections::HashMap<String, String>>,
) -> std::collections::HashSet<String> {
    let mut required = std::collections::HashSet::new();
    let preferred = settings.preferred_provider.clone();
    let Some(config) = config else {
        required.insert(preferred);
        return required;
    };

    let extraction_uses_preferred = pdf_extraction_may_run(config, input_path, extra_inputs)
        && effective_pdf_extractor(settings, config) == "llm";

    required.insert(settings.orientation_agent().to_string());
    if settings.auto_revision_reconciliation || diff || extraction_uses_preferred {
        required.insert(preferred.clone());
    }

    let enabled = config.steps.iter().filter(|step| step.enabled);
    let mut needs_merge = false;
    for step in enabled {
        if step.agents.is_empty() {
            required.insert(preferred.clone());
        } else {
            required.extend(step.agents.iter().cloned());
            needs_merge |= step.agents.len() > 1;
        }
    }
    if config.merge.enabled && needs_merge {
        required.insert(
            config
                .merge
                .agents
                .first()
                .cloned()
                .unwrap_or_else(|| settings.sequential_agent().to_string()),
        );
    }
    required
}

/// Run all dependency checks against one already-loaded settings/profile
/// snapshot. Provider CLIs are still probed in API mode so their own sign-in
/// state can be reported separately if the user later switches transports.
fn check_all_for(
    settings: &crate::settings::Settings,
    config: Option<&crate::pipeline_config::PipelineConfig>,
    diff: bool,
    input_path: Option<&str>,
    extra_inputs: Option<&std::collections::HashMap<String, String>>,
) -> DepsReport {
    let required_providers = required_providers(settings, config, diff, input_path, extra_inputs);
    let pdf_requirements = pdf_dependency_requirements(settings, config, input_path, extra_inputs);
    let effective_extractor = config.map(|config| effective_pdf_extractor(settings, config));
    let has_anthropic_key = !settings.anthropic_api_key.trim().is_empty();
    let has_openai_key = !settings.openai_api_key.trim().is_empty();
    let has_google_key = !settings.google_api_key.trim().is_empty();
    let claude_api_mode = settings.model_transport("claude") == "api";
    let codex_api_mode = settings.model_transport("codex") == "api";
    let antigravity_api_mode = settings.model_transport("antigravity") == "api";
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

        let antigravity_h = s.spawn(move || {
            let ProbeResult {
                command,
                version,
                path,
            } = probe("agy", &["--version"]);
            let auth = command.as_ref().and_then(check_antigravity_auth);
            let version_supported = command
                .is_some()
                .then(|| antigravity_version_supported(&version))
                .flatten();
            (command.is_some(), version, path, auth, version_supported)
        });

        // Poppler binaries are normally bundled, with a system fallback.
        let pdftoppm_h = s.spawn(|| find_on_path("pdftoppm"));

        // pdftotext: just check existence (no subprocess needed)
        let pdftotext_h = s.spawn(|| find_on_path("pdftotext"));

        // The Full Parser is one managed extractor whose implementation spans
        // the native recognition server and the isolated Paddle layout client.
        // Startup readiness uses the bounded commit-marker probe; extraction
        // performs the full multi-GB integrity verification before execution.
        let paddle_full_h = s.spawn(crate::engines::paddle_full_parser_status);

        // Local OpenAI-compatible server: TCP reachability of the configured
        // base URL (fast, no HTTP parse — a listener there is a good signal).
        let local_url = local_base_url.clone();
        let local_h = s.spawn(move || probe_local_server(&local_url, &local_api_key));

        // Collect results
        let (found, ver, path, claude_auth) = claude_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let claude_setup = cli_setup_recommendation(
            "Claude Code",
            "npm install -g @anthropic-ai/claude-code",
            false,
            cfg!(target_os = "windows"),
        );
        let (claude_hint, claude_help_url) = if claude_api_mode && has_anthropic_key {
            ("API key configured — CLI not required.".to_string(), None)
        } else if claude_api_mode {
            (
                "Claude API mode is selected, but no Anthropic API key is configured. Add one in Settings → API Keys or switch to Subscription mode."
                    .to_string(),
                None,
            )
        } else if found && claude_auth == Some(false) {
            (
                "Claude CLI is installed but not signed in. Run `claude auth login` to authenticate."
                    .to_string(),
                None,
            )
        } else if found && claude_auth.is_none() {
            (
                "Claude CLI is installed, but authentication status could not be verified. Run `claude auth status`."
                    .to_string(),
                None,
            )
        } else {
            claude_setup
        };
        let claude = DepStatus {
            name: "Claude CLI".into(),
            found: if claude_api_mode {
                has_anthropic_key
            } else {
                found
            },
            version: if claude_api_mode {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: required_providers.contains("claude"),
            hint: claude_hint,
            help_url: claude_help_url,
            authenticated: if claude_api_mode {
                Some(has_anthropic_key)
            } else {
                claude_auth
            },
            cli_auth_status: cli_auth_status(found, claude_auth),
        };

        let (found, ver, path, codex_auth) = codex_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None));
        let codex_setup = cli_setup_recommendation(
            "Codex CLI",
            "npm install -g @openai/codex",
            false,
            cfg!(target_os = "windows"),
        );
        let (codex_hint, codex_help_url) = if codex_api_mode && has_openai_key {
            ("API key configured — CLI not required.".to_string(), None)
        } else if codex_api_mode {
            (
                "ChatGPT API mode is selected, but no OpenAI API key is configured. Add one in Settings → API Keys or switch to Subscription mode."
                    .to_string(),
                None,
            )
        } else if found && codex_auth == Some(false) {
            (
                "Codex CLI is installed but not authenticated. Run `codex login` or set OPENAI_API_KEY."
                    .to_string(),
                None,
            )
        } else if found && codex_auth.is_none() {
            (
                "Codex CLI is installed, but authentication status could not be verified. Run `codex login status`."
                    .to_string(),
                None,
            )
        } else {
            codex_setup
        };
        let codex = DepStatus {
            name: "Codex CLI".into(),
            found: if codex_api_mode {
                has_openai_key
            } else {
                found
            },
            version: if codex_api_mode {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: required_providers.contains("codex"),
            hint: codex_hint,
            help_url: codex_help_url,
            authenticated: if codex_api_mode {
                Some(has_openai_key)
            } else {
                codex_auth
            },
            cli_auth_status: cli_auth_status(found, codex_auth),
        };

        let (found, ver, path, antigravity_auth, antigravity_version_ok) = antigravity_h
            .join()
            .unwrap_or_else(|_| (false, String::new(), String::new(), None, None));
        let antigravity_cli_usable = found && antigravity_version_ok == Some(true);
        let antigravity_install = cli_setup_recommendation(
            "Antigravity CLI",
            "curl -fsSL https://antigravity.google/cli/install.sh | bash",
            false,
            cfg!(target_os = "windows"),
        );
        let (antigravity_hint, antigravity_help_url) = if antigravity_api_mode && has_google_key {
            ("API key configured — CLI not required.".to_string(), None)
        } else if antigravity_api_mode {
            (
                "Antigravity API mode is selected, but no Google AI API key is configured. Add one in Settings → API Keys or switch to Subscription mode."
                    .to_string(),
                None,
            )
        } else if found && antigravity_version_ok == Some(false) {
            (
                "The installed Antigravity CLI predates the required 1.1.12 release. Run `agy update`, then run the dependency check again."
                    .to_string(),
                None,
            )
        } else if found && antigravity_version_ok.is_none() {
            (
                "Could not verify the Antigravity CLI version. Run `agy update`, then run the dependency check again."
                    .to_string(),
                None,
            )
        } else if found && antigravity_auth == Some(false) {
            (
                "Antigravity CLI is installed but not signed in. Run `agy` in a terminal to sign in, then refresh this check."
                    .to_string(),
                None,
            )
        } else if found && antigravity_auth.is_none() {
            (
                "Antigravity CLI is installed, but sign-in could not be verified. Run `agy` in a terminal to sign in, then refresh this check."
                    .to_string(),
                None,
            )
        } else {
            // A fresh install lands in ~/.local/bin, which the cached
            // login-shell PATH may not include until the app restarts.
            (
                format!(
                    "{} Then restart Pipeline so the new `agy` command is visible on its PATH.",
                    antigravity_install.0
                ),
                antigravity_install.1,
            )
        };
        let antigravity = DepStatus {
            name: "Antigravity CLI".into(),
            found: if antigravity_api_mode {
                has_google_key
            } else {
                antigravity_cli_usable
            },
            version: if antigravity_api_mode {
                "direct API".into()
            } else {
                ver
            },
            path,
            required: required_providers.contains("antigravity"),
            hint: antigravity_hint,
            help_url: antigravity_help_url,
            authenticated: if antigravity_api_mode {
                Some(has_google_key)
            } else {
                antigravity_auth
            },
            cli_auth_status: cli_auth_status(found, antigravity_auth),
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
            required: pdf_requirements.pdftoppm,
            hint: if pdftoppm_path.is_some() {
                "Renders PDF pages for PaddleOCR-VL and run artifacts. Bundled with Pipeline."
                    .into()
            } else if pdf_requirements.pdftoppm {
                format!("{install_hint} — required by the selected PaddleOCR-VL extraction.")
            } else {
                format!("{install_hint} — enables PaddleOCR-VL and PDF page artifacts.")
            },
            help_url: None,
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
            required: pdf_requirements.pdftotext,
            hint: if pdftotext_path.is_some() {
                "PDF text extraction and LLM completeness verification. Bundled with Pipeline."
                    .into()
            } else if pdf_requirements.pdftotext {
                format!("{install_hint} — required by the selected PDF extraction workflow.")
            } else {
                format!("{install_hint} — enables pdftotext and LLM PDF extraction.")
            },
            help_url: None,
            authenticated: None,
            cli_auth_status: None,
        };

        let paddle_full_paths = paddle_full_h.join().ok().and_then(Result::ok);
        let paddle_full_found = paddle_full_paths.is_some();
        let paddle_full = DepStatus {
            name: "PaddleOCR-VL Full Parser".into(),
            found: paddle_full_found,
            version: paddle_full_paths
                .as_ref()
                .map(|paths| format!("managed {}", paths.release))
                .unwrap_or_default(),
            path: paddle_full_paths
                .as_ref()
                .map(|paths| paths.script.to_string_lossy().to_string())
                .unwrap_or_default(),
            required: pdf_requirements.paddle_full,
            hint: if paddle_full_found {
                "Managed PaddleOCR layout client, recognition runtime, and models are installed."
                    .into()
            } else if pdf_requirements.paddle_full {
                "Install from Settings → PDF Extraction, or run `pipeline-cli engines install paddle` from a source build."
                    .into()
            } else {
                "Install PaddleOCR-VL Full Parser from Settings, or run `pipeline-cli engines install paddle` from a source build."
                    .into()
            },
            help_url: None,
            authenticated: None,
            cli_auth_status: None,
        };

        let extractor = DepStatus {
            name: "PDF extractor configuration".into(),
            found: pdf_requirements.configuration_ready,
            version: effective_extractor.unwrap_or("not needed").to_string(),
            path: String::new(),
            required: pdf_requirements.extraction,
            hint: match effective_extractor {
                Some("llm") if settings.preferred_provider == "local" => {
                    "LLM PDF extraction is unavailable for local OpenAI-compatible servers. Choose PaddleOCR-VL or pdftotext extraction."
                        .into()
                }
                Some("llm" | "paddleocr-vl-full" | "pdftotext") | None => {
                    "The selected PDF extraction method is supported.".into()
                }
                Some(_) => {
                    "Choose LLM, PaddleOCR-VL, or pdftotext extraction in Settings.".into()
                }
            },
            help_url: None,
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
            required: required_providers.contains("local"),
            hint: if local_reachable {
                "OpenAI-compatible server responding at the configured URL.".into()
            } else {
                "No server at the configured URL. Install Ollama (ollama.com), run it, \
                 and pull a model (e.g. `ollama pull llama3.3`), or point Settings → Models \
                 at another OpenAI-compatible server."
                    .into()
            },
            help_url: None,
            authenticated: None,
            cli_auth_status: None,
        };

        // Every provider the active workflow actually dispatches to must be
        // usable (its `required` flag comes from the profile's agents plus
        // the role defaults). An unrelated available provider — say, a local
        // server answering /v1/models — must not mask a missing required
        // CLI, or this preflight's actionable hint degrades into a mid-run
        // "No launchable … CLI" failure. PDF parsing requirements remain
        // independently enforced.
        let model_access_ready = [&claude, &codex, &antigravity, &local]
            .into_iter()
            .all(dependency_ready);
        let pdf_dependencies_ready = [&pdftoppm, &pdftotext, &paddle_full, &extractor]
            .into_iter()
            .all(dependency_ready);
        let ready = model_access_ready && pdf_dependencies_ready;
        let deps = vec![
            claude,
            codex,
            antigravity,
            local,
            pdftoppm,
            pdftotext,
            paddle_full,
            extractor,
        ];
        DepsReport { deps, ready }
    })
}

pub fn check_snapshot(
    settings: crate::settings::Settings,
    config: crate::pipeline_config::PipelineConfig,
    diff: bool,
    input_path: Option<String>,
    extra_inputs: std::collections::HashMap<String, String>,
) -> DepsReport {
    check_all_for(
        &settings,
        Some(&config),
        diff,
        input_path.as_deref(),
        Some(&extra_inputs),
    )
}
