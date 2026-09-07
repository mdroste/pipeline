//! Shared account, model and quota operations. No product state or credentials.
use super::transport::{AppServerClient, RequestError, DEFAULT_REQUEST_TIMEOUT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
pub(crate) const MODEL_PAGE_SIZE: u32 = 100;
const MAX_MODEL_PAGES: usize = 32;
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AccountStatus {
    SignedOut,
    Chatgpt,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    pub status: AccountStatus,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub unsupported_account_type: Option<String>,
    pub requires_openai_auth: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginStart {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningEffortOption {
    pub reasoning_effort: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceModel {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub is_default: bool,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<ReasoningEffortOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub models: Vec<WorkspaceModel>,
}

impl ModelCatalog {
    pub fn validate_exact(
        &self,
        model: &str,
        effort: Option<&str>,
    ) -> Result<WorkspaceModel, RequestError> {
        validate_identifier("model id", model)?;
        let selected = self
            .models
            .iter()
            .find(|candidate| candidate.model == model)
            .ok_or_else(|| {
                RequestError::invalid(format!(
                    "Model {model} is not available to the selected ChatGPT account"
                ))
            })?;
        if let Some(effort) = effort {
            validate_identifier("reasoning effort", effort)?;
            if !selected
                .supported_reasoning_efforts
                .iter()
                .any(|candidate| candidate.reasoning_effort == effort)
            {
                return Err(RequestError::invalid(format!(
                    "Model {model} does not advertise reasoning effort {effort}"
                )));
            }
        }
        Ok(selected.clone())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitWindow {
    pub used_percent: i32,
    pub remaining_percent: i32,
    pub window_duration_mins: Option<i64>,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitBucket {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub primary: Option<RateLimitWindow>,
    pub secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RateLimitSource {
    PerBucket,
    Legacy,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimits {
    pub source: RateLimitSource,
    pub buckets: Vec<RateLimitBucket>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelListPage {
    data: Vec<WorkspaceModel>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRateLimitWindow {
    used_percent: i32,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRateLimitBucket {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<WireRateLimitWindow>,
    secondary: Option<WireRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRateLimits {
    rate_limits: WireRateLimitBucket,
    rate_limits_by_limit_id: Option<HashMap<String, WireRateLimitBucket>>,
}

impl AppServerClient {
    pub async fn account_state(&self, refresh_token: bool) -> Result<AccountState, RequestError> {
        let value = self
            .request(
                "account/read",
                json!({"refreshToken": refresh_token}),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        parse_account_state(value)
    }

    pub async fn login_start(&self) -> Result<LoginStart, RequestError> {
        let value = self
            .request(
                "account/login/start",
                json!({
                    "type": "chatgpt",
                    "appBrand": "chatgpt",
                    "useHostedLoginSuccessPage": true
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        if value.get("type").and_then(Value::as_str) != Some("chatgpt") {
            return Err(RequestError::unavailable(
                "Codex App Server returned an unsupported login method",
            ));
        }
        Ok(LoginStart {
            login_id: required_string(&value, &["loginId"], "account/login/start response")?,
            auth_url: required_string(&value, &["authUrl"], "account/login/start response")?,
        })
    }

    pub async fn login_cancel(&self, login_id: &str) -> Result<bool, RequestError> {
        validate_identifier("login id", login_id)?;
        let value = self
            .request(
                "account/login/cancel",
                json!({"loginId": login_id}),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        match value.get("status").and_then(Value::as_str) {
            Some("canceled") => Ok(true),
            Some("notFound") => Ok(false),
            _ => Err(RequestError::unavailable(
                "Codex App Server returned an invalid login cancellation status",
            )),
        }
    }

    pub async fn logout(&self) -> Result<(), RequestError> {
        self.request("account/logout", Value::Null, DEFAULT_REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    pub async fn model_catalog(&self) -> Result<ModelCatalog, RequestError> {
        let mut cursor: Option<String> = None;
        let mut seen_cursors = HashSet::new();
        let mut seen_models = HashSet::new();
        let mut models = Vec::new();
        for _ in 0..MAX_MODEL_PAGES {
            let value = self
                .request(
                    "model/list",
                    json!({
                        "cursor": cursor,
                        "includeHidden": false,
                        "limit": MODEL_PAGE_SIZE
                    }),
                    DEFAULT_REQUEST_TIMEOUT,
                )
                .await?;
            let page: ModelListPage = serde_json::from_value(value).map_err(|error| {
                RequestError::unavailable(format!("Invalid model/list response: {error}"))
            })?;
            for model in page.data {
                if model.id.is_empty() || model.model.is_empty() {
                    return Err(RequestError::unavailable(
                        "model/list returned a model without an identity",
                    ));
                }
                if !seen_models.insert(model.id.clone()) {
                    return Err(RequestError::unavailable(format!(
                        "model/list returned duplicate model id {}",
                        model.id
                    )));
                }
                models.push(model);
            }
            let Some(next_cursor) = page.next_cursor else {
                return Ok(ModelCatalog { models });
            };
            if next_cursor.is_empty() || !seen_cursors.insert(next_cursor.clone()) {
                return Err(RequestError::unavailable(
                    "model/list returned an invalid pagination cursor",
                ));
            }
            cursor = Some(next_cursor);
        }
        Err(RequestError::unavailable(format!(
            "model/list exceeded the {MAX_MODEL_PAGES}-page safety limit"
        )))
    }

    pub async fn validate_model_selection(
        &self,
        model: &str,
        effort: Option<&str>,
    ) -> Result<WorkspaceModel, RequestError> {
        self.model_catalog().await?.validate_exact(model, effort)
    }

    pub async fn rate_limits(&self) -> Result<RateLimits, RequestError> {
        let value = self
            .request(
                "account/rateLimits/read",
                Value::Null,
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        parse_rate_limits(value)
    }
}
fn validate_identifier(label: &str, value: &str) -> Result<(), RequestError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|character| character.is_control())
    {
        return Err(RequestError::invalid(format!("Invalid {label}")));
    }
    Ok(())
}

pub(crate) fn parse_account_state(value: Value) -> Result<AccountState, RequestError> {
    let requires_openai_auth = value
        .get("requiresOpenaiAuth")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            RequestError::unavailable("account/read response omitted boolean requiresOpenaiAuth")
        })?;
    let Some(account) = value.get("account").filter(|account| !account.is_null()) else {
        return Ok(AccountState {
            status: AccountStatus::SignedOut,
            email: None,
            plan_type: None,
            unsupported_account_type: None,
            requires_openai_auth,
        });
    };
    let account_type = required_string(account, &["type"], "account/read response")?;
    if account_type == "chatgpt" {
        return Ok(AccountState {
            status: AccountStatus::Chatgpt,
            email: account
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_string),
            plan_type: Some(required_string(
                account,
                &["planType"],
                "account/read response",
            )?),
            unsupported_account_type: None,
            requires_openai_auth,
        });
    }
    Ok(AccountState {
        status: AccountStatus::Unsupported,
        email: None,
        plan_type: None,
        unsupported_account_type: Some(account_type),
        requires_openai_auth,
    })
}

pub(crate) fn parse_rate_limits(value: Value) -> Result<RateLimits, RequestError> {
    let wire: WireRateLimits = serde_json::from_value(value).map_err(|error| {
        RequestError::unavailable(format!("Invalid account/rateLimits/read response: {error}"))
    })?;
    if let Some(per_bucket) = wire
        .rate_limits_by_limit_id
        .filter(|per_bucket| !per_bucket.is_empty())
    {
        let mut buckets: Vec<RateLimitBucket> = per_bucket
            .into_iter()
            .map(|(limit_id, mut bucket)| {
                if bucket.limit_id.is_none() {
                    bucket.limit_id = Some(limit_id);
                }
                bucket.into()
            })
            .collect();
        buckets.sort_by(|left, right| left.limit_id.cmp(&right.limit_id));
        return Ok(RateLimits {
            source: RateLimitSource::PerBucket,
            buckets,
        });
    }
    Ok(RateLimits {
        source: RateLimitSource::Legacy,
        buckets: vec![wire.rate_limits.into()],
    })
}

impl From<WireRateLimitWindow> for RateLimitWindow {
    fn from(window: WireRateLimitWindow) -> Self {
        Self {
            used_percent: window.used_percent,
            remaining_percent: (100 - window.used_percent).clamp(0, 100),
            window_duration_mins: window.window_duration_mins,
            resets_at: window.resets_at,
        }
    }
}

impl From<WireRateLimitBucket> for RateLimitBucket {
    fn from(bucket: WireRateLimitBucket) -> Self {
        Self {
            limit_id: bucket.limit_id,
            limit_name: bucket.limit_name,
            plan_type: bucket.plan_type,
            primary: bucket.primary.map(Into::into),
            secondary: bucket.secondary.map(Into::into),
        }
    }
}

fn required_string(value: &Value, path: &[&str], context: &str) -> Result<String, RequestError> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(key).ok_or_else(|| {
            RequestError::unavailable(format!("{context} omitted {}", path.join(".")))
        })?;
    }
    cursor.as_str().map(str::to_string).ok_or_else(|| {
        RequestError::unavailable(format!(
            "{context} field {} was not a string",
            path.join(".")
        ))
    })
}

pub(crate) fn is_safe_auth_url(url: &str) -> bool {
    if url.len() > 8 * 1024
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let Some(remainder) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.contains(['@', '\\'])
}
