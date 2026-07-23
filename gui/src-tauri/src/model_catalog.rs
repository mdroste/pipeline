//! Dynamic provider model discovery and durable selection resolution.
//!
//! Availability comes from the installed CLI or authenticated API. The small
//! remote policy file only adds role mappings and price/deprecation metadata;
//! it is never allowed to invent availability when a live catalog succeeded.

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
const MAX_POLICY_BYTES: usize = 1024 * 1024;
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_DISCOVERY_VERSION_BYTES: usize = 64 * 1024;
const MAX_DISCOVERY_LINE_BYTES: usize = 1024 * 1024;
const POLICY_URL: &str =
    "https://raw.githubusercontent.com/mdroste/pipeline/main/model-policy.json";
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

fn read_cache(provider: &str, transport: &str) -> Option<CacheEnvelope> {
    let path = cache_path(provider, transport).ok()?;
    let bytes = read_file_limited(&path, MAX_CATALOG_BYTES)?;
    serde_json::from_slice(&bytes).ok()
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
    serde_json::from_str(BUNDLED_POLICY).unwrap_or_default()
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

async fn current_policy(refresh: bool) -> Policy {
    let path = cache_dir().ok().map(|dir| dir.join("policy.json"));
    if !refresh {
        if let Some(path) = &path {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.modified().ok().and_then(|m| m.elapsed().ok()) < Some(CACHE_TTL) {
                    if let Some(bytes) = read_file_limited(path, MAX_POLICY_BYTES) {
                        if let Ok(policy) = serde_json::from_slice::<Policy>(&bytes) {
                            if policy.schema_version == 1 {
                                return policy;
                            }
                        }
                    }
                }
            }
        }
    }

    let fetched = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok();
    if let Some(client) = fetched {
        if let Ok(response) = client.get(POLICY_URL).send().await {
            if let Ok(response) = response.error_for_status() {
                if let Ok(body) = response_bytes_limited(response, MAX_POLICY_BYTES).await {
                    if let Ok(policy) = serde_json::from_slice::<Policy>(&body) {
                        if policy.schema_version == 1 {
                            if let Some(path) = path {
                                let _ = std::fs::create_dir_all(
                                    path.parent().unwrap_or(Path::new(".")),
                                );
                                let _ = std::fs::write(path, &body);
                            }
                            return policy;
                        }
                    }
                }
            }
        }
    }
    bundled_policy()
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

async fn api_catalog(provider: &str, settings: &Settings) -> Result<ModelCatalog, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
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
            let excluded = [
                "embedding",
                "moderation",
                "whisper",
                "tts",
                "dall-e",
                "realtime",
                "transcribe",
                "audio",
                "image",
                "ft:",
                "babbage",
                "davinci",
            ];
            let mut discovered = Vec::new();
            for item in value["data"].as_array().into_iter().flatten() {
                if let Some(id) = item["id"].as_str() {
                    if !excluded.iter().any(|word| id.contains(word)) {
                        discovered.push((item["created"].as_u64().unwrap_or(0), entry(id)));
                    }
                }
            }
            // OpenAI's list has no default marker. Prefer recently created
            // generations for inferred Automatic/role choices while the
            // remote policy remains free to supply a more precise mapping.
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
            let url = format!("{}/models", settings.local_base_url.trim_end_matches('/'));
            let mut request = client.get(url);
            if !settings.local_api_key.trim().is_empty() {
                request = request.bearer_auth(&settings.local_api_key);
            }
            let response = request
                .send()
                .await
                .map_err(|e| format!("Local model discovery failed: {e}"))?
                .error_for_status()
                .map_err(|e| format!("Local model discovery failed: {e}"))?;
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

async fn rpc_exchange(
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
        // Claude Code exposes durable tier aliases but no supported machine
        // model-list endpoint. Keep Automatic plus the policy roles.
        "claude" => {}
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
    if !refresh && provider != "local" {
        if let Some(cache) = read_cache(provider, transport) {
            if now_epoch().saturating_sub(cache.saved_at) < CACHE_TTL.as_secs() {
                return Ok(cache.catalog);
            }
        }
    }
    let policy = current_policy(refresh).await;
    let live = if transport == "cli" {
        cli_catalog(provider).await
    } else {
        api_catalog(provider, settings).await
    };
    match live {
        Ok(mut catalog) => {
            apply_policy(&mut catalog, &policy);
            let envelope = CacheEnvelope {
                saved_at: now_epoch(),
                catalog: catalog.clone(),
            };
            if provider != "local" {
                let _ = write_json_atomic(&cache_path(provider, transport)?, &envelope);
            }
            Ok(catalog)
        }
        Err(error) => {
            if provider != "local" {
                if let Some(mut cache) = read_cache(provider, transport) {
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
        let cached = read_cache(provider, &transport).map(|c| c.catalog);
        let resolved_model = cached
            .as_ref()
            .and_then(|c| {
                c.default_model
                    .clone()
                    .or_else(|| c.recommended_model.clone())
            })
            .unwrap_or_else(|| "Provider default".to_string());
        return Ok(ResolvedModel {
            selection,
            command_model: None,
            resolved_model,
            transport,
            source: cached
                .as_ref()
                .map(|c| c.source.clone())
                .unwrap_or_else(|| "provider_default".into()),
            catalog_updated_at: cached.map(|c| c.fetched_at).unwrap_or_default(),
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

/// Pricing remains available synchronously to report rendering. The bundled
/// policy is intentionally conservative; live remote metadata can extend this
/// in future without coupling run completion to the network.
pub fn price_for_model(model: &str) -> Option<(f64, f64)> {
    let model = model.to_ascii_lowercase();
    let policy = cache_dir()
        .ok()
        .and_then(|dir| read_file_limited(&dir.join("policy.json"), MAX_POLICY_BYTES))
        .and_then(|bytes| serde_json::from_slice::<Policy>(&bytes).ok())
        .filter(|policy| policy.schema_version == 1)
        .unwrap_or_else(bundled_policy);
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
    }

    #[test]
    fn cache_reader_is_bounded() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"123456789").unwrap();
        assert_eq!(read_file_limited(temp.path(), 9).unwrap(), b"123456789");
        assert!(read_file_limited(temp.path(), 8).is_none());
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
}
