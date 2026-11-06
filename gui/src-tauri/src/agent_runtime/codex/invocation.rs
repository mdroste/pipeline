//! Shared native invocation mechanics. Callers own policy, account leases,
//! persistence, event collection, cancellation, and ambiguous-outcome recovery.

use super::{AppServerClient, RequestError, DEFAULT_REQUEST_TIMEOUT};
use serde::Serialize;
use serde_json::Value;

/// Both owners supply explicit authority. No mode-specific defaults or ambient
/// settings belong at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeThreadStart<'a> {
    pub cwd: &'a std::path::Path,
    pub runtime_workspace_roots: &'a [String],
    pub permissions: &'a str,
    pub approval_policy: &'a str,
    pub developer_instructions: Option<&'a str>,
    pub base_instructions: Option<&'a str>,
    #[serde(serialize_with = "serialize_dynamic_tools")]
    pub dynamic_tools: &'a [Value],
    pub model: Option<&'a str>,
    pub reasoning_effort: Option<&'a str>,
    pub ephemeral: bool,
    pub service_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

// Provider-neutral catalogs and saved harness snapshots can omit the function
// tag. Codex requires the whole list to use one format, so normalize only the
// outgoing request and preserve canonical functions and namespaces verbatim.
fn serialize_dynamic_tools<S: serde::Serializer>(
    tools: &[Value],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut tools = tools.to_vec();
    for tool in &mut tools {
        tool.as_object_mut()
            .ok_or_else(|| serde::ser::Error::custom("Dynamic tool must be an object"))?
            .entry("type")
            .or_insert_with(|| "function".into());
    }
    tools.serialize(serializer)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextTurnStart<'a> {
    pub thread_id: &'a str,
    #[serde(skip)]
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_user_message_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<&'a Value>,
}

impl AppServerClient {
    pub(crate) async fn start_native_thread(
        &self,
        request: NativeThreadStart<'_>,
    ) -> Result<Value, RequestError> {
        let mut params = serde_json::to_value(request)
            .map_err(|error| RequestError::invalid(error.to_string()))?;
        params["modelProvider"] = "openai".into();
        params["allowProviderModelFallback"] = false.into();
        params["approvalsReviewer"] = "user".into();
        self.request("thread/start", params, DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    /// Exactly one submission. Return the original RPC error unchanged so each
    /// owner can distinguish rejection from an ambiguously acknowledged turn.
    pub(crate) async fn start_text_turn(
        &self,
        request: TextTurnStart<'_>,
    ) -> Result<Value, RequestError> {
        let mut params = serde_json::to_value(&request)
            .map_err(|error| RequestError::invalid(error.to_string()))?;
        params["input"] = serde_json::json!([{"type":"text", "text":request.text}]);
        self.request("turn/start", params, DEFAULT_REQUEST_TIMEOUT)
            .await
    }
}

#[cfg(test)]
mod tests;
