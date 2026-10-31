//! API protocol mechanics shared by independently owned runtimes.
pub mod http;
pub mod records;
pub(crate) mod streaming;
pub(crate) mod wire;

use crate::connections::ApiProvider;
use serde_json::{json, Value};

pub(crate) fn authorize(
    provider: ApiProvider,
    request: reqwest::RequestBuilder,
    key: &str,
) -> reqwest::RequestBuilder {
    match provider {
        ApiProvider::Anthropic => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        ApiProvider::Google => request.header("x-goog-api-key", key),
        _ if !key.is_empty() => request.bearer_auth(key),
        _ => request,
    }
}

pub(crate) fn function_tool(
    provider: ApiProvider,
    name: &str,
    description: &str,
    schema: &Value,
) -> Value {
    match provider {
        ApiProvider::Anthropic => {
            json!({"name":name,"description":description,"input_schema":schema})
        }
        ApiProvider::Google => {
            json!({"name":name,"description":description,"parametersJsonSchema":schema})
        }
        _ => {
            json!({"type":"function","function":{"name":name,"description":description,"parameters":schema}})
        }
    }
}
pub mod tool_output;
