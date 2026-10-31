//! Connection identity shared by all callers. Legacy settings and saved runtime
//! IDs are decoded here; callers supply their own configuration snapshot.
//! This module never chooses an active runtime or reads ambient run state.
use crate::settings::Settings;
use serde::{Deserialize, Serialize};

mod api;
pub mod capabilities;
pub mod models;
pub mod thinking;
pub use api::{ApiConnection, ApiProvider};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionId {
    #[serde(rename = "codex")]
    Chatgpt,
    #[serde(rename = "claude")]
    ClaudeCode,
    #[serde(rename = "openai-api")]
    OpenAiApi,
    #[serde(rename = "anthropic-api")]
    AnthropicApi,
    #[serde(rename = "google-api")]
    GeminiApi,
    #[serde(rename = "local")]
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    CodexAppServer,
    CodexCli,
    ClaudeCode,
    OpenAiChat,
    AnthropicMessages,
    Gemini,
    CompatibleChat,
}

impl ConnectionId {
    pub fn transport(self, settings: &Settings) -> Transport {
        match self {
            Self::Chatgpt if settings.codex_backend == "legacy_cli" => Transport::CodexCli,
            Self::Chatgpt => Transport::CodexAppServer,
            Self::ClaudeCode => Transport::ClaudeCode,
            Self::OpenAiApi => Transport::OpenAiChat,
            Self::AnthropicApi => Transport::AnthropicMessages,
            Self::GeminiApi => Transport::Gemini,
            Self::Local => Transport::CompatibleChat,
        }
    }

    pub fn from_runtime(value: &str) -> Result<Self, String> {
        match value {
            "codex" => Ok(Self::Chatgpt),
            "claude" => Ok(Self::ClaudeCode),
            "openai-api" => Ok(Self::OpenAiApi),
            "anthropic-api" => Ok(Self::AnthropicApi),
            "google-api" => Ok(Self::GeminiApi),
            "local" => Ok(Self::Local),
            _ => Err("Choose a connection in Settings.".into()),
        }
    }

    pub fn for_review(provider: &str, settings: &Settings) -> Result<Self, String> {
        let api =
            |mode: &str, key: &str| mode == "api" || (mode.is_empty() && !key.trim().is_empty());
        match provider {
            "claude" | "" if api(&settings.claude_access_mode, &settings.anthropic_api_key) => {
                Ok(Self::AnthropicApi)
            }
            "claude" | "" => Ok(Self::ClaudeCode),
            "codex" if api(&settings.codex_access_mode, &settings.openai_api_key) => {
                Ok(Self::OpenAiApi)
            }
            "codex" => Ok(Self::Chatgpt),
            "antigravity" => Ok(Self::GeminiApi),
            "local" => Ok(Self::Local),
            _ => Err("Choose a connection in Settings.".into()),
        }
    }

    pub fn api(self) -> Option<ApiProvider> {
        match self {
            Self::OpenAiApi => Some(ApiProvider::OpenAi),
            Self::AnthropicApi => Some(ApiProvider::Anthropic),
            Self::GeminiApi => Some(ApiProvider::Google),
            Self::Local => Some(ApiProvider::Local),
            _ => None,
        }
    }

    pub fn catalog_provider(self) -> &'static str {
        match self {
            Self::Chatgpt | Self::OpenAiApi => "codex",
            Self::ClaudeCode | Self::AnthropicApi => "claude",
            Self::GeminiApi => "antigravity",
            Self::Local => "local",
        }
    }

    pub fn legacy_transport(self) -> &'static str {
        if self.api().is_some() {
            "api"
        } else {
            "cli"
        }
    }
}

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
pub struct Description {
    pub id: ConnectionId,
    pub label: String,
    pub family: String,
    pub access: String,
}
#[derive(Deserialize)]
struct Registry {
    connections: Vec<Description>,
}

pub fn descriptions() -> &'static [Description] {
    static REGISTRY: std::sync::LazyLock<Registry> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../connections.json"))
            .expect("valid connection registry")
    });
    &REGISTRY.connections
}
