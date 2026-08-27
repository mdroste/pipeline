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
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_SCHEMA_VERSION: u32 = 3;
const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_DISCOVERY_VERSION_BYTES: usize = 64 * 1024;
const MAX_DISCOVERY_LINE_BYTES: usize = 1024 * 1024;
const BUNDLED_POLICY: &str = include_str!("../../../model-policy.json");

const MAX_DISCOVERY_FLIGHTS: usize = 64;
const DISCOVERY_FLIGHT_TTL: Duration = Duration::from_secs(15 * 60);

/// A non-secret identity for one discovery context. Credentials and endpoints
/// are reduced to one-way fingerprints before they can reach this process-wide
/// registry, so the registry can neither retain nor accidentally log secrets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DiscoveryKey {
    provider: String,
    transport: String,
    endpoint_fingerprint: String,
    credential_fingerprint: String,
    catalog_revision: String,
}

struct DiscoveryFlight {
    result: Mutex<Option<Result<ModelCatalog, String>>>,
    completed: tokio::sync::Notify,
    started_at: std::time::Instant,
}

static DISCOVERY_FLIGHTS: OnceLock<Mutex<HashMap<DiscoveryKey, Arc<DiscoveryFlight>>>> =
    OnceLock::new();

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
            "antigravity" => settings.google_api_key.as_str(),
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
        "antigravity" => {
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
        "antigravity" => {
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

mod discovery;
mod resolution;

use discovery::cli_catalog;
pub(crate) use discovery::rpc_exchange;
pub use resolution::{discover, price_for_model, resolve};

#[cfg(test)]
use discovery::{
    populate_antigravity_models, populate_claude_models, provider_discovery_error,
    read_discovery_output,
};
#[cfg(test)]
use resolution::{discovery_key, single_flight_discovery};

#[cfg(test)]
mod tests;
