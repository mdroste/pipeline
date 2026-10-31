//! API connection resolution from a caller-owned settings snapshot.
//! Credentials stay in memory and are never serialized with connection metadata.
use serde_json::{json, Value};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiProvider {
    OpenAi,
    Anthropic,
    Google,
    Local,
}
impl ApiProvider {
    pub fn connection(self) -> super::ConnectionId {
        match self {
            Self::OpenAi => super::ConnectionId::OpenAiApi,
            Self::Anthropic => super::ConnectionId::AnthropicApi,
            Self::Google => super::ConnectionId::GeminiApi,
            Self::Local => super::ConnectionId::Local,
        }
    }
    pub fn runtime(self) -> &'static str {
        match self {
            Self::OpenAi => "openai-api",
            Self::Anthropic => "anthropic-api",
            Self::Google => "google-api",
            Self::Local => "local",
        }
    }
    pub fn catalog_provider(self) -> &'static str {
        match self {
            Self::OpenAi => "codex",
            Self::Anthropic => "claude",
            Self::Google => "antigravity",
            Self::Local => "local",
        }
    }
}

#[derive(Clone)]
pub struct ApiConnection {
    pub provider: ApiProvider,
    pub endpoint: String,
    pub key: String,
    pub model: String,
    pub effort: Option<String>,
    pub thinking: Option<Value>,
}

impl ApiConnection {
    pub async fn from_settings(
        provider: ApiProvider,
        model: Option<&str>,
        effort: Option<&str>,
        settings: &crate::settings::Settings,
    ) -> Result<Self, String> {
        if provider == ApiProvider::Local {
            crate::settings::validate_local_base_url(&settings.local_base_url)?;
        }
        let key = provider.key(settings)?.to_owned();
        let model = match model.filter(|m| !m.trim().is_empty()) {
            Some(model) => model.trim().to_owned(),
            None if provider == ApiProvider::Local && !settings.local_model.trim().is_empty() => {
                settings.local_model.trim().to_owned()
            }
            None => {
                let catalog = crate::model_catalog::discover_connection(
                    provider.connection(),
                    settings,
                    false,
                )
                .await?;
                catalog
                    .recommended_model
                    .or(catalog.default_model)
                    .or_else(|| catalog.models.first().map(|m| m.id.clone()))
                    .ok_or("Choose a model for this connection")?
            }
        };
        if model.len() > 200 || model.chars().any(|c| c.is_control()) {
            return Err("Invalid model name".into());
        }
        let effort = effort.filter(|e| !e.is_empty()).map(str::to_owned);
        let mut thinking = None;
        if let Some(effort) = effort.as_deref() {
            if !matches!(
                effort,
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            ) {
                return Err("Choose an available thinking level.".into());
            }
            if provider == ApiProvider::Google {
                thinking = super::thinking::google_config(&model, effort)?;
            } else if provider == ApiProvider::Anthropic {
                let catalog = crate::model_catalog::discover_connection(
                    provider.connection(),
                    settings,
                    false,
                )
                .await?;
                let metadata = catalog
                    .models
                    .iter()
                    .find(|m| m.id == model)
                    .ok_or("Choose a discovered model to configure thinking")?;
                if super::capabilities::effort(metadata, effort)
                    != super::capabilities::Support::Supported
                {
                    return Err("Choose a thinking level listed for this model.".into());
                }
                if super::capabilities::feature(metadata, "adaptive_thinking")
                    == super::capabilities::Support::Supported
                {
                    thinking = Some(json!({"type":"adaptive"}));
                }
            }
        }
        let endpoint = provider.endpoint(settings, &model)?;
        Ok(Self {
            provider,
            endpoint,
            key,
            model,
            effort,
            thinking,
        })
    }
}

impl ApiProvider {
    pub fn key(self, settings: &crate::settings::Settings) -> Result<&str, String> {
        let key = match self {
            Self::OpenAi => &settings.openai_api_key,
            Self::Anthropic => &settings.anthropic_api_key,
            Self::Google => &settings.google_api_key,
            Self::Local => &settings.local_api_key,
        };
        if self != Self::Local && key.trim().is_empty() {
            return Err("Add an API key in Settings → Connections.".into());
        }
        Ok(key)
    }
    pub fn endpoint(
        self,
        settings: &crate::settings::Settings,
        model: &str,
    ) -> Result<String, String> {
        if self == Self::Local {
            crate::settings::validate_local_base_url(&settings.local_base_url)?;
        }
        Ok(match self {
            Self::OpenAi => "https://api.openai.com/v1/chat/completions".into(),
            Self::Anthropic => "https://api.anthropic.com/v1/messages".into(),
            Self::Google => {
                let mut url =
                    reqwest::Url::parse("https://generativelanguage.googleapis.com/v1beta/models/")
                        .unwrap();
                url.path_segments_mut()
                    .unwrap()
                    .pop_if_empty()
                    .push(&format!(
                        "{}:generateContent",
                        model.trim_start_matches("models/")
                    ));
                url.to_string()
            }
            Self::Local => format!(
                "{}/chat/completions",
                settings.local_base_url.trim().trim_end_matches('/')
            ),
        })
    }
}
