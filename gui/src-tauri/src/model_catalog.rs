//! Dynamic provider model discovery and durable selection resolution.
//!
//! Availability comes from the installed CLI or authenticated API. Role,
//! pricing, and deprecation policy is bundled with the signed application so
//! mutable remote content cannot change model selection behavior.

use crate::pipeline::claude::build_provider_command;
use crate::settings::{ModelSelection, Settings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_SCHEMA_VERSION: u32 = 3;
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_DISCOVERY_VERSION_BYTES: usize = 64 * 1024;
const MAX_DISCOVERY_LINE_BYTES: usize = 1024 * 1024;
const BUNDLED_POLICY: &str = include_str!("../../../model-policy.json");

static DISCOVERY_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub supported_efforts: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default)]
    pub input_price_per_million: Option<f64>,
    #[serde(default)]
    pub output_price_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRole {
    pub id: String,
    pub label: String,
    pub description: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCatalog {
    pub provider: String,
    pub transport: String,
    pub source: String,
    pub source_version: String,
    pub fetched_at: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub recommended_model: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelCatalogEntry>,
    #[serde(default)]
    pub roles: Vec<ModelRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedModel {
    pub selection: ModelSelection,
    /// Omitted for CLI Automatic, preserving the CLI's own default selection.
    pub command_model: Option<String>,
    pub resolved_model: String,
    pub transport: String,
    pub source: String,
    pub catalog_updated_at: String,
    #[serde(default)]
    pub supported_efforts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CacheEnvelope {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    policy_fingerprint: String,
    #[serde(default)]
    credential_fingerprint: String,
    saved_at: u64,
    catalog: ModelCatalog,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Policy {
    schema_version: u32,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    transports: HashMap<String, TransportPolicy>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TransportPolicy {
    #[serde(default)]
    roles: HashMap<String, String>,
    #[serde(default)]
    models: Vec<PolicyModel>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PolicyModel {
    id: String,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    replacement: Option<String>,
    #[serde(default)]
    input_price_per_million: Option<f64>,
    #[serde(default)]
    output_price_per_million: Option<f64>,
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn cache_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    Ok(home.join(".pipeline").join("cache").join("models"))
}

fn safe_key(provider: &str, transport: &str) -> String {
    format!("{provider}-{transport}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn cache_path(provider: &str, transport: &str) -> Result<PathBuf, String> {
    Ok(cache_dir()?.join(format!("{}.json", safe_key(provider, transport))))
}

fn read_file_limited(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path).ok()?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn read_cache(provider: &str, transport: &str, settings: &Settings) -> Option<CacheEnvelope> {
    let path = cache_path(provider, transport).ok()?;
    let bytes = read_file_limited(&path, MAX_CATALOG_BYTES)?;
    let envelope = serde_json::from_slice::<CacheEnvelope>(&bytes).ok()?;
    cache_matches_context(&envelope, provider, transport, settings).then_some(envelope)
}

fn catalog_credential_fingerprint(provider: &str, transport: &str, settings: &Settings) -> String {
    use sha2::{Digest as _, Sha256};
    let credential = if transport == "api" {
        match provider {
            "claude" => settings.anthropic_api_key.as_str(),
            "codex" => settings.openai_api_key.as_str(),
            "gemini" => settings.google_api_key.as_str(),
            "local" => settings.local_api_key.as_str(),
            _ => "",
        }
    } else {
        "installed-cli-account"
    };
    let mut digest = Sha256::new();
    digest.update(b"pipeline model catalog credential v1\0");
    digest.update(provider.as_bytes());
    digest.update(b"\0");
    digest.update(transport.as_bytes());
    digest.update(b"\0");
    digest.update(credential.as_bytes());
    if provider == "local" {
        digest.update(b"\0");
        digest.update(settings.local_base_url.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn cache_matches_context(
    envelope: &CacheEnvelope,
    provider: &str,
    transport: &str,
    settings: &Settings,
) -> bool {
    envelope.schema_version == CACHE_SCHEMA_VERSION
        && envelope.policy_fingerprint == bundled_policy_fingerprint()
        && envelope.credential_fingerprint
            == catalog_credential_fingerprint(provider, transport, settings)
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), String> {
    use std::io::Write as _;
    let parent = path.parent().ok_or("Cache path has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create model cache: {e}"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Failed to create model cache file: {e}"))?;
    serde_json::to_writer_pretty(&mut temp, value)
        .map_err(|e| format!("Failed to serialize model cache: {e}"))?;
    temp.flush()
        .map_err(|e| format!("Failed to flush model cache: {e}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Failed to sync model cache: {e}"))?;
    temp.persist(path)
        .map_err(|e| format!("Failed to persist model cache: {}", e.error))?;
    Ok(())
}

fn bundled_policy() -> Policy {
    let policy = serde_json::from_str::<Policy>(BUNDLED_POLICY).unwrap_or_default();
    if policy.schema_version == 1 {
        policy
    } else {
        Policy::default()
    }
}

fn bundled_policy_fingerprint() -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(BUNDLED_POLICY.as_bytes()))
}

async fn response_bytes_limited(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(format!("Response exceeded the {limit} byte safety limit"));
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(format!("Response exceeded the {limit} byte safety limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn response_json_limited(
    response: reqwest::Response,
    label: &str,
) -> Result<serde_json::Value, String> {
    let bytes = response_bytes_limited(response, MAX_CATALOG_BYTES)
        .await
        .map_err(|e| format!("{label} model discovery failed: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid {label} model list: {e}"))
}

fn policy_for<'a>(
    policy: &'a Policy,
    provider: &str,
    transport: &str,
) -> Option<&'a TransportPolicy> {
    policy.transports.get(&format!("{provider}:{transport}"))
}

fn display_name(id: &str) -> String {
    id.replace('-', " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn entry(id: impl Into<String>) -> ModelCatalogEntry {
    let id = id.into();
    ModelCatalogEntry {
        display_name: display_name(&id),
        id,
        ..Default::default()
    }
}

fn infer_roles(provider: &str, models: &[ModelCatalogEntry]) -> Vec<ModelRole> {
    let stable = |id: &str| {
        let s = id.to_ascii_lowercase();
        !s.contains("preview") && !s.contains("experimental") && !s.contains("-exp")
    };
    let pick = |needles: &[&str]| {
        models
            .iter()
            .find(|m| {
                stable(&m.id)
                    && needles
                        .iter()
                        .all(|n| m.id.to_ascii_lowercase().contains(n))
            })
            .or_else(|| {
                models.iter().find(|m| {
                    needles
                        .iter()
                        .all(|n| m.id.to_ascii_lowercase().contains(n))
                })
            })
            .map(|m| m.id.clone())
    };
    let mut roles = Vec::new();
    let mut add = |id: &str, label: &str, description: &str, model: Option<String>| {
        if let Some(model) = model {
            roles.push(ModelRole {
                id: id.into(),
                label: label.into(),
                description: description.into(),
                model,
            });
        }
    };
    match provider {
        "claude" => {
            add(
                "quality",
                "Quality",
                "Best available quality tier",
                pick(&["opus"]),
            );
            add(
                "balanced",
                "Balanced",
                "Default balance of quality and speed",
                pick(&["sonnet"]),
            );
            add("fast", "Fast", "Lowest-latency tier", pick(&["haiku"]));
        }
        "gemini" => {
            add(
                "quality",
                "Quality",
                "Best available quality tier",
                pick(&["pro"]),
            );
            add(
                "balanced",
                "Balanced",
                "Default balance of quality and speed",
                pick(&["flash"]).filter(|m| !m.contains("lite")),
            );
            add(
                "fast",
                "Fast",
                "Lowest-latency tier",
                pick(&["flash", "lite"]),
            );
        }
        "codex" => {
            let quality = models
                .iter()
                .find(|m| !m.id.contains("mini") && !m.id.contains("nano"))
                .map(|m| m.id.clone());
            let fast = models
                .iter()
                .find(|m| m.id.contains("mini") || m.id.contains("nano"))
                .map(|m| m.id.clone());
            add(
                "quality",
                "Quality",
                "Best available quality tier",
                quality.clone(),
            );
            add(
                "balanced",
                "Balanced",
                "Provider-recommended general model",
                quality,
            );
            add("fast", "Fast", "Lower-latency model", fast);
        }
        _ => {}
    }
    roles
}

fn apply_policy(catalog: &mut ModelCatalog, policy: &Policy) {
    if catalog.source_version.is_empty() {
        catalog.source_version = policy.updated_at.clone();
    }
    let Some(transport_policy) = policy_for(policy, &catalog.provider, &catalog.transport) else {
        return;
    };
    for model in &mut catalog.models {
        if let Some(meta) = transport_policy.models.iter().find(|m| m.id == model.id) {
            model.deprecated = meta.deprecated;
            model.replacement.clone_from(&meta.replacement);
            model.input_price_per_million = meta.input_price_per_million;
            model.output_price_per_million = meta.output_price_per_million;
        }
    }
    if catalog.roles.is_empty() {
        catalog.roles = infer_roles(&catalog.provider, &catalog.models);
    }
    for (id, model) in &transport_policy.roles {
        if catalog.models.is_empty()
            || catalog.models.iter().any(|m| m.id == *model)
            || catalog.transport == "cli"
        {
            let role = ModelRole {
                id: id.clone(),
                label: display_name(id),
                description: format!("Provider {id} tier"),
                model: model.clone(),
            };
            if let Some(existing) = catalog.roles.iter_mut().find(|role| role.id == *id) {
                *existing = role;
            } else {
                catalog.roles.push(role);
            }
        }
    }
    if catalog.recommended_model.is_none() {
        catalog.recommended_model = catalog
            .roles
            .iter()
            .find(|r| r.id == "balanced")
            .map(|r| r.model.clone())
            .or_else(|| catalog.default_model.clone())
            .or_else(|| catalog.models.first().map(|m| m.id.clone()));
    }
}

fn base_catalog(provider: &str, transport: &str, source: &str) -> ModelCatalog {
    ModelCatalog {
        provider: provider.into(),
        transport: transport.into(),
        source: source.into(),
        fetched_at: now_iso(),
        ..Default::default()
    }
}

/// The direct OpenAI transport uses Chat Completions with function tools.
/// `/v1/models` also returns embeddings, media, realtime, search, and
/// Responses-only coding models, so endpoint compatibility must be an
/// allowlist plus explicit specialty exclusions rather than a broad denylist.
fn is_openai_chat_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    if id.starts_with("ft:") {
        return false;
    }
    let general_family = id.starts_with("gpt-")
        || id.starts_with("o1")
        || id.starts_with("o3")
        || id.starts_with("o4");
    let specialty = [
        "embedding",
        "moderation",
        "whisper",
        "tts",
        "dall-e",
        "realtime",
        "transcribe",
        "audio",
        "image",
        "sora",
        "search",
        "computer-use",
        "deep-research",
        "codex",
        "babbage",
        "davinci",
    ];
    general_family && !specialty.iter().any(|word| id.contains(word))
}

async fn api_catalog(provider: &str, settings: &Settings) -> Result<ModelCatalog, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to create model catalog client: {e}"))?;
    let mut catalog = base_catalog(provider, "api", "provider_api");
    match provider {
        "claude" => {
            let response = client
                .get("https://api.anthropic.com/v1/models?limit=1000")
                .header("x-api-key", &settings.anthropic_api_key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await
                .map_err(|e| format!("Anthropic model discovery failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("Anthropic model discovery failed: {e}"))?;
            let value = response_json_limited(response, "Anthropic").await?;
            for item in value["data"].as_array().into_iter().flatten() {
                if let Some(id) = item["id"].as_str() {
                    catalog.models.push(entry(id));
                }
            }
        }
        "codex" => {
            let response = client
                .get("https://api.openai.com/v1/models")
                .bearer_auth(&settings.openai_api_key)
                .send()
                .await
                .map_err(|e| format!("OpenAI model discovery failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("OpenAI model discovery failed: {e}"))?;
            let value = response_json_limited(response, "OpenAI").await?;
            let mut discovered = Vec::new();
            for item in value["data"].as_array().into_iter().flatten() {
                if let Some(id) = item["id"].as_str() {
                    if is_openai_chat_model(id) {
                        discovered.push((item["created"].as_u64().unwrap_or(0), entry(id)));
                    }
                }
            }
            // Recency only orders endpoint-compatible general models. The
            // policy's explicit role mapping selects Automatic/role defaults.
            discovered.sort_by_key(|(created, _)| std::cmp::Reverse(*created));
            catalog.models = discovered.into_iter().map(|(_, model)| model).collect();
        }
        "gemini" => {
            let response = client
                .get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000")
                .header("x-goog-api-key", &settings.google_api_key)
                .send()
                .await
                .map_err(|e| format!("Google model discovery failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("Google model discovery failed: {e}"))?;
            let value = response_json_limited(response, "Google").await?;
            for item in value["models"].as_array().into_iter().flatten() {
                let supports_generate = item["supportedGenerationMethods"]
                    .as_array()
                    .is_none_or(|methods| methods.iter().any(|m| m == "generateContent"));
                if supports_generate {
                    if let Some(id) = item["name"]
                        .as_str()
                        .and_then(|n| n.strip_prefix("models/"))
                    {
                        let mut model = entry(id);
                        model.display_name = item["displayName"].as_str().unwrap_or(id).to_string();
                        model.description = item["description"].as_str().unwrap_or("").to_string();
                        catalog.models.push(model);
                    }
                }
            }
        }
        "local" => {
            crate::settings::validate_local_base_url(&settings.local_base_url)?;
            let url = format!("{}/models", settings.local_base_url.trim_end_matches('/'));
            let mut request =
                crate::pipeline::api_common::custom_endpoint_client(&settings.local_base_url)
                    .get(url)
                    .timeout(Duration::from_secs(15));
            if !settings.local_api_key.trim().is_empty() {
                request = request.bearer_auth(&settings.local_api_key);
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Local model discovery failed: {e}"))?;
            if !response.status().is_success() {
                return Err(format!(
                    "Local model discovery failed: HTTP {}",
                    response.status()
                ));
            }
            let value = response_json_limited(response, "Local").await?;
            for item in value["data"].as_array().into_iter().flatten() {
                if let Some(id) = item["id"].as_str() {
                    catalog.models.push(entry(id));
                }
            }
            // Ollama's native endpoint is a common fallback.
            if catalog.models.is_empty() {
                for item in value["models"].as_array().into_iter().flatten() {
                    if let Some(id) = item["name"].as_str() {
                        catalog.models.push(entry(id));
                    }
                }
            }
        }
        _ => return Err(format!("Unknown model provider: {provider}")),
    }
    catalog.roles = infer_roles(provider, &catalog.models);
    Ok(catalog)
}

async fn read_discovery_output<R: AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut kept = Vec::with_capacity(limit.min(16 * 1024));
    let mut overflowed = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|e| format!("Failed to read provider discovery output: {e}"))?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        let take = remaining.min(count);
        kept.extend_from_slice(&chunk[..take]);
        overflowed |= take < count;
    }
    Ok((kept, overflowed))
}

struct DiscoveryProcessGuard {
    pid: u32,
    armed: bool,
}

impl DiscoveryProcessGuard {
    fn register(pid: u32) -> Self {
        if pid > 0 {
            crate::commands::register_child_pid(pid);
        }
        Self {
            pid,
            armed: pid > 0,
        }
    }

    fn unregister(&mut self) {
        if self.armed {
            crate::commands::unregister_child_pid(self.pid);
            self.armed = false;
        }
    }
}

impl Drop for DiscoveryProcessGuard {
    fn drop(&mut self) {
        if self.armed {
            crate::commands::kill_process(self.pid);
            crate::commands::unregister_child_pid(self.pid);
        }
    }
}

async fn stop_discovery_child(
    child: &mut tokio::process::Child,
    guard: &mut DiscoveryProcessGuard,
) {
    if guard.pid > 0 {
        crate::commands::kill_process(guard.pid);
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    guard.unregister();
}

async fn cli_version(program: &str) -> String {
    let args = vec!["--version".to_string()];
    let Ok(mut command) = build_provider_command(program, None, &args) else {
        return String::new();
    };
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return String::new();
    };
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let Some(stdout) = child.stdout.take() else {
        stop_discovery_child(&mut child, &mut guard).await;
        return String::new();
    };
    let reader = tokio::spawn(read_discovery_output(stdout, MAX_DISCOVERY_VERSION_BYTES));
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    if !matches!(status, Ok(Ok(status)) if status.success()) {
        stop_discovery_child(&mut child, &mut guard).await;
        let _ = reader.await;
        return String::new();
    }
    guard.unregister();
    match reader.await {
        Ok(Ok((bytes, false))) => String::from_utf8_lossy(&bytes).trim().to_string(),
        _ => String::new(),
    }
}

pub(crate) async fn rpc_exchange(
    program: &str,
    args: &[String],
    requests: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>, String> {
    let mut command = build_provider_command(program, None, args)?;
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start {program} model discovery: {e}"))?;
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let result = async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Provider discovery has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Provider discovery has no stdout")?;
        let mut reader = BufReader::new(stdout);
        let mut results = Vec::new();
        for request in requests {
            let id = request["id"].clone();
            let response = tokio::time::timeout(Duration::from_secs(12), async {
                stdin
                    .write_all(
                        serde_json::to_string(request)
                            .map_err(|e| e.to_string())?
                            .as_bytes(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
                stdin.flush().await.map_err(|e| e.to_string())?;
                while let Some(record) = crate::pipeline::logging::next_bounded_line(
                    &mut reader,
                    MAX_DISCOVERY_LINE_BYTES,
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    if record.truncated {
                        return Err(format!(
                            "{program} model discovery emitted a response line larger than {} MB",
                            MAX_DISCOVERY_LINE_BYTES / 1024 / 1024
                        ));
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&record.text) {
                        if value.get("id") == Some(&id) {
                            return Ok(value);
                        }
                    }
                }
                Err("Provider discovery ended before replying".to_string())
            })
            .await
            .map_err(|_| format!("{program} model discovery timed out"))??;
            if response.get("error").is_some() {
                return Err(format!(
                    "{program} discovery returned {}",
                    response["error"]
                ));
            }
            results.push(response);
            if program == "codex" && id == serde_json::json!(1) {
                tokio::time::timeout(Duration::from_secs(12), async {
                    stdin
                        .write_all(
                            b"{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}\n",
                        )
                        .await
                        .map_err(|e| e.to_string())?;
                    stdin.flush().await.map_err(|e| e.to_string())
                })
                .await
                .map_err(|_| format!("{program} model discovery timed out"))??;
            }
        }
        Ok(results)
    }
    .await;
    stop_discovery_child(&mut child, &mut guard).await;
    result
}

/// Ask Claude Code for the same account-aware model list exposed by its
/// interactive `/model` picker. This is the documented Agent SDK initialize
/// handshake: it does not submit a user prompt or make a model call.
async fn claude_sdk_initialize() -> Result<serde_json::Value, String> {
    let args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--safe-mode",
        "--permission-mode",
        "dontAsk",
        "--tools",
        "",
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    let mut command = build_provider_command("claude", None, &args)?;
    command
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to start Claude model discovery: {e}"))?;
    let mut guard = DiscoveryProcessGuard::register(child.id().unwrap_or(0));
    let result = async {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Claude model discovery has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Claude model discovery has no stdout")?;
        let request_id = "pipeline-model-list";
        let request = serde_json::json!({
            "type": "control_request",
            "request_id": request_id,
            "request": {
                "subtype": "initialize",
                "hooks": {}
            }
        });
        stdin
            .write_all(
                serde_json::to_string(&request)
                    .map_err(|e| e.to_string())?
                    .as_bytes(),
            )
            .await
            .map_err(|e| e.to_string())?;
        stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stdout);
        tokio::time::timeout(Duration::from_secs(20), async {
            while let Some(record) =
                crate::pipeline::logging::next_bounded_line(&mut reader, MAX_DISCOVERY_LINE_BYTES)
                    .await
                    .map_err(|e| e.to_string())?
            {
                if record.truncated {
                    return Err(format!(
                        "Claude model discovery emitted a response line larger than {} MB",
                        MAX_DISCOVERY_LINE_BYTES / 1024 / 1024
                    ));
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&record.text) else {
                    continue;
                };
                if value["type"] != "control_response"
                    || value["response"]["request_id"] != request_id
                {
                    continue;
                }
                if value["response"]["subtype"] == "error" {
                    return Err(format!(
                        "Claude model discovery returned {}",
                        value["response"]["error"]
                    ));
                }
                return value["response"]["response"]
                    .as_object()
                    .map(|_| value["response"]["response"].clone())
                    .ok_or_else(|| {
                        "Claude model discovery returned no initialization data".to_string()
                    });
            }
            Err("Claude model discovery ended before replying".to_string())
        })
        .await
        .map_err(|_| "Claude model discovery timed out".to_string())?
    }
    .await;
    stop_discovery_child(&mut child, &mut guard).await;
    result
}

fn populate_claude_models(
    catalog: &mut ModelCatalog,
    initialization: &serde_json::Value,
) -> Result<(), String> {
    let models = initialization["models"]
        .as_array()
        .ok_or("Claude model discovery returned no model list")?;
    let default_resolved = models
        .iter()
        .find(|model| model["value"] == "default")
        .and_then(|model| model["resolvedModel"].as_str())
        .map(str::to_string);
    catalog.default_model.clone_from(&default_resolved);

    for item in models {
        let Some(id) = item["value"].as_str() else {
            continue;
        };
        // Pipeline already provides an Automatic option which deliberately
        // omits --model, so do not duplicate Claude's `default` sentinel as a
        // pinnable exact model.
        if id == "default" {
            continue;
        }
        let mut model = entry(id);
        model.display_name = item["displayName"].as_str().unwrap_or(id).to_string();
        model.description = item["description"].as_str().unwrap_or("").to_string();
        model.supported_efforts = item["supportedEffortLevels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_string)
            .collect();
        model.is_default = default_resolved
            .as_deref()
            .is_some_and(|default| item["resolvedModel"].as_str() == Some(default));
        catalog.models.push(model);
    }
    if catalog.models.is_empty() {
        return Err("Claude model discovery returned an empty model list".into());
    }
    Ok(())
}

async fn cli_catalog(provider: &str) -> Result<ModelCatalog, String> {
    let program = match provider {
        "claude" => "claude",
        "codex" => "codex",
        "gemini" => "gemini",
        _ => return Err(format!("Unknown CLI provider: {provider}")),
    };
    let mut catalog = base_catalog(provider, "cli", "installed_cli");
    catalog.source_version = cli_version(program).await;
    match provider {
        "codex" => {
            let requests = [
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"pipeline","title":"Pipeline","version":env!("CARGO_PKG_VERSION")}}}),
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"model/list","params":{"limit":100}}),
            ];
            let replies = rpc_exchange(
                "codex",
                &["app-server".into(), "--listen".into(), "stdio://".into()],
                &requests,
            )
            .await?;
            let result = &replies[1]["result"];
            for item in result["data"].as_array().into_iter().flatten() {
                let Some(id) = item["model"].as_str().or_else(|| item["id"].as_str()) else {
                    continue;
                };
                let mut model = entry(id);
                model.display_name = item["displayName"].as_str().unwrap_or(id).to_string();
                model.description = item["description"].as_str().unwrap_or("").to_string();
                model.is_default = item["isDefault"].as_bool().unwrap_or(false);
                model.supported_efforts = item["supportedReasoningEfforts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|effort| {
                        effort["reasoningEffort"]
                            .as_str()
                            .or_else(|| effort.as_str())
                    })
                    .map(str::to_string)
                    .collect();
                if model.is_default {
                    catalog.default_model = Some(id.to_string());
                }
                catalog.models.push(model);
            }
        }
        "gemini" => {
            let cwd = std::env::temp_dir().to_string_lossy().to_string();
            let requests = [
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{},"clientInfo":{"name":"pipeline","version":env!("CARGO_PKG_VERSION")}}}),
                serde_json::json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":cwd,"mcpServers":[]}}),
            ];
            let replies = rpc_exchange("gemini", &["--acp".into()], &requests).await?;
            let models = &replies[1]["result"]["models"];
            catalog.default_model = models["currentModelId"].as_str().map(str::to_string);
            for item in models["availableModels"].as_array().into_iter().flatten() {
                let Some(id) = item["modelId"].as_str().or_else(|| item["id"].as_str()) else {
                    continue;
                };
                let mut model = entry(id);
                model.display_name = item["name"].as_str().unwrap_or(id).to_string();
                model.description = item["description"].as_str().unwrap_or("").to_string();
                model.is_default = catalog.default_model.as_deref() == Some(id);
                catalog.models.push(model);
            }
        }
        "claude" => {
            let initialization = claude_sdk_initialize().await?;
            populate_claude_models(&mut catalog, &initialization)?;
        }
        _ => unreachable!(),
    }
    catalog.roles = infer_roles(provider, &catalog.models);
    Ok(catalog)
}

pub async fn discover(
    provider: &str,
    settings: &Settings,
    refresh: bool,
) -> Result<ModelCatalog, String> {
    // A parallel wave can reach resolution simultaneously. Serialize the
    // initial refresh so only one CLI/API probe runs; followers consume the
    // cache written by the leader.
    let _guard = DISCOVERY_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let provider = if provider.is_empty() {
        "claude"
    } else {
        provider
    };
    let transport = settings.model_transport(provider);
    // API credentials have a stable, non-secret fingerprint. CLI account
    // identity does not, so never reuse an installed-CLI catalog across calls:
    // the user may have switched accounts since it was written.
    let cacheable = provider != "local" && transport == "api";
    if !refresh && cacheable {
        if let Some(cache) = read_cache(provider, transport, settings) {
            // Builds before Claude Agent SDK discovery cached only the three
            // policy roles and an empty model list. Refresh those legacy
            // envelopes immediately after upgrading instead of preserving
            // them for the remainder of the 24-hour TTL.
            let legacy_claude_cli_cache = provider == "claude"
                && transport == "cli"
                && cache.catalog.source == "installed_cli"
                && cache.catalog.models.is_empty();
            if !legacy_claude_cli_cache
                && now_epoch().saturating_sub(cache.saved_at) < CACHE_TTL.as_secs()
            {
                return Ok(cache.catalog);
            }
        }
    }
    let policy = bundled_policy();
    let live = if transport == "cli" {
        cli_catalog(provider).await
    } else {
        api_catalog(provider, settings).await
    };
    match live {
        Ok(mut catalog) => {
            apply_policy(&mut catalog, &policy);
            let envelope = CacheEnvelope {
                schema_version: CACHE_SCHEMA_VERSION,
                policy_fingerprint: bundled_policy_fingerprint(),
                credential_fingerprint: catalog_credential_fingerprint(
                    provider, transport, settings,
                ),
                saved_at: now_epoch(),
                catalog: catalog.clone(),
            };
            if cacheable {
                let _ = write_json_atomic(&cache_path(provider, transport)?, &envelope);
            }
            Ok(catalog)
        }
        Err(error) => {
            if cacheable {
                if let Some(mut cache) = read_cache(provider, transport, settings) {
                    cache.catalog.stale = true;
                    cache.catalog.warning = Some(format!(
                        "Live discovery failed; using the last known catalog. {error}"
                    ));
                    return Ok(cache.catalog);
                }
            }
            let mut catalog = base_catalog(provider, transport, "bundled_policy");
            catalog.stale = true;
            catalog.warning = Some(format!(
                "Live discovery failed. Automatic still uses the provider default. {error}"
            ));
            if let Some(transport_policy) = policy_for(&policy, provider, transport) {
                catalog.models = transport_policy
                    .models
                    .iter()
                    .map(|model| {
                        let mut entry = entry(&model.id);
                        entry.deprecated = model.deprecated;
                        entry.replacement.clone_from(&model.replacement);
                        entry.input_price_per_million = model.input_price_per_million;
                        entry.output_price_per_million = model.output_price_per_million;
                        entry
                    })
                    .collect();
            }
            apply_policy(&mut catalog, &policy);
            Ok(catalog)
        }
    }
}

pub async fn resolve(
    provider: &str,
    settings: &Settings,
    step_selection: Option<&ModelSelection>,
) -> Result<ResolvedModel, String> {
    let provider = if provider.is_empty() {
        "claude"
    } else {
        provider
    };
    let transport = settings.model_transport(provider).to_string();
    let selection = step_selection
        .cloned()
        .unwrap_or_else(|| settings.model_selection(provider));

    // CLI Automatic is deliberately zero-probe at run time: omitting the flag
    // lets the installed CLI choose whatever that installation supports.
    if transport == "cli" && selection == ModelSelection::Automatic {
        return Ok(ResolvedModel {
            selection,
            command_model: None,
            resolved_model: "Provider default".to_string(),
            transport,
            source: "provider_default".to_string(),
            catalog_updated_at: String::new(),
            supported_efforts: Vec::new(),
        });
    }

    let catalog = discover(provider, settings, false).await?;
    let command_model = match &selection {
        ModelSelection::Automatic => catalog
            .recommended_model
            .clone()
            .or_else(|| catalog.default_model.clone())
            .or_else(|| catalog.models.first().map(|m| m.id.clone())),
        ModelSelection::Role { role } => {
            let normalized = match (provider, role.as_str()) {
                ("claude", "sonnet") => "balanced",
                ("claude", "opus" | "fable") => "quality",
                ("claude", "haiku") => "fast",
                ("gemini", "pro") => "quality",
                ("gemini", "flash") => "balanced",
                ("gemini", "flash-lite") => "fast",
                _ => role,
            };
            catalog
                .roles
                .iter()
                .find(|r| r.id == normalized || r.model == *role)
                .map(|r| r.model.clone())
        }
        ModelSelection::Pinned { model } => {
            if !catalog.stale
                && !catalog.models.is_empty()
                && !catalog.models.iter().any(|m| m.id == *model)
            {
                return Err(format!("Pinned model '{model}' is not available for {provider} {}. Choose another model or Automatic.", catalog.transport));
            }
            Some(model.clone())
        }
    };
    let command_model = command_model.ok_or_else(|| {
        format!(
            "Could not resolve {} for {provider} {}",
            selection.label(),
            catalog.transport
        )
    })?;
    let supported_efforts = catalog
        .models
        .iter()
        .find(|m| m.id == command_model)
        .map(|m| m.supported_efforts.clone())
        .unwrap_or_default();
    Ok(ResolvedModel {
        selection,
        resolved_model: command_model.clone(),
        command_model: Some(command_model),
        transport: catalog.transport,
        source: catalog.source,
        catalog_updated_at: catalog.fetched_at,
        supported_efforts,
    })
}

/// Pricing remains available synchronously to report rendering and is tied to
/// the policy embedded in the signed application build.
pub fn price_for_model(model: &str) -> Option<(f64, f64)> {
    let model = model.to_ascii_lowercase();
    let policy = bundled_policy();
    policy
        .transports
        .values()
        .flat_map(|p| p.models.iter())
        .filter(|m| {
            model == m.id.to_ascii_lowercase() || model.contains(&m.id.to_ascii_lowercase())
        })
        // A full model id can contain another valid id (for example,
        // `gpt-4.1-mini` contains `gpt-4.1`). Prefer the most specific match
        // instead of depending on policy-file ordering.
        .filter_map(|m| {
            Some((
                m.id.len(),
                m.input_price_per_million?,
                m.output_price_per_million?,
            ))
        })
        .max_by_key(|(specificity, _, _)| *specificity)
        .map(|(_, input, output)| (input, output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_policy_is_valid_and_versioned() {
        let policy = bundled_policy();
        assert_eq!(policy.schema_version, 1);
        assert!(!policy.updated_at.is_empty());
    }

    #[test]
    fn pricing_comes_from_policy() {
        assert_eq!(price_for_model("claude-sonnet-4-6"), Some((3.0, 15.0)));
        assert_eq!(price_for_model("gpt-4.1-mini"), Some((0.4, 1.6)));
        assert_eq!(price_for_model("gpt-5.6-terra"), Some((2.5, 15.0)));
    }

    #[test]
    fn openai_chat_filter_excludes_specialty_and_responses_only_models() {
        for model in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-4.1", "o3", "o4-mini"] {
            assert!(is_openai_chat_model(model), "{model}");
        }
        for model in [
            "text-embedding-3-large",
            "gpt-image-2",
            "gpt-realtime-2",
            "gpt-5.3-codex",
            "gpt-4o-search-preview",
            "omni-moderation-latest",
            "ft:gpt-4.1:org:custom",
        ] {
            assert!(!is_openai_chat_model(model), "{model}");
        }
    }

    #[test]
    fn cache_reader_is_bounded() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"123456789").unwrap();
        assert_eq!(read_file_limited(temp.path(), 9).unwrap(), b"123456789");
        assert!(read_file_limited(temp.path(), 8).is_none());
    }

    #[test]
    fn legacy_or_different_policy_caches_are_rejected() {
        let settings = Settings {
            openai_api_key: "account-one-secret".to_string(),
            ..Default::default()
        };
        let legacy = CacheEnvelope {
            saved_at: now_epoch(),
            ..Default::default()
        };
        assert!(!cache_matches_context(&legacy, "codex", "api", &settings));

        let current = CacheEnvelope {
            schema_version: CACHE_SCHEMA_VERSION,
            policy_fingerprint: bundled_policy_fingerprint(),
            credential_fingerprint: catalog_credential_fingerprint("codex", "api", &settings),
            saved_at: now_epoch(),
            ..Default::default()
        };
        assert!(cache_matches_context(&current, "codex", "api", &settings));

        let other_account = Settings {
            openai_api_key: "account-two-secret".to_string(),
            ..Default::default()
        };
        assert!(!cache_matches_context(
            &current,
            "codex",
            "api",
            &other_account
        ));
        assert!(!serde_json::to_string(&current)
            .unwrap()
            .contains("account-one-secret"));
    }

    #[test]
    fn discovery_output_is_bounded_but_fully_drained() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (mut writer, reader) = tokio::io::duplex(8);
                let writing = tokio::spawn(async move {
                    writer.write_all(b"123456789").await.unwrap();
                });
                let (bytes, overflowed) = read_discovery_output(reader, 4).await.unwrap();
                writing.await.unwrap();
                assert_eq!(bytes, b"1234");
                assert!(overflowed);
            });
    }

    #[test]
    fn claude_sdk_models_are_account_aware_and_skip_default_sentinel() {
        let initialization = serde_json::json!({
            "models": [
                {
                    "value": "default",
                    "resolvedModel": "claude-opus-4-8[1m]",
                    "displayName": "Default (recommended)",
                    "description": "Account default"
                },
                {
                    "value": "opus[1m]",
                    "resolvedModel": "claude-opus-4-8[1m]",
                    "displayName": "Opus",
                    "description": "Everyday complex tasks",
                    "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"]
                },
                {
                    "value": "claude-fable-5[1m]",
                    "resolvedModel": "claude-fable-5",
                    "displayName": "Fable",
                    "description": "Hardest and longest-running tasks",
                    "supportedEffortLevels": ["low", "medium", "high", "xhigh", "max"]
                }
            ]
        });
        let mut catalog = base_catalog("claude", "cli", "installed_cli");
        populate_claude_models(&mut catalog, &initialization).unwrap();

        assert_eq!(
            catalog.default_model.as_deref(),
            Some("claude-opus-4-8[1m]")
        );
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[0].id, "opus[1m]");
        assert!(catalog.models[0].is_default);
        assert_eq!(catalog.models[1].id, "claude-fable-5[1m]");
        assert_eq!(catalog.models[1].display_name, "Fable");
        assert_eq!(
            catalog.models[1].supported_efforts,
            ["low", "medium", "high", "xhigh", "max"]
        );
    }
}
