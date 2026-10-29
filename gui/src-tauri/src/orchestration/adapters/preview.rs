//! Bounded task result presentation and terminal Workspace classification.
use serde_json::{json, Value};

/// History rows keep small previews. The full immutable response is loaded only
/// when requested; bindings use the latest full value for each step.
pub fn output_preview(value: &Value) -> Value {
    fn compact(v: &Value, depth: usize) -> Value {
        if depth > 5 {
            return json!("[Open output for nested content]");
        }
        match v {
            Value::String(s) if s.len() > 2048 => {
                let mut n = 2048;
                while !s.is_char_boundary(n) {
                    n -= 1;
                }
                json!(format!("{}… [Open output for the full response]", &s[..n]))
            }
            Value::Array(a) => {
                Value::Array(a.iter().take(12).map(|v| compact(v, depth + 1)).collect())
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .take(32)
                    .map(|(k, v)| (k.clone(), compact(v, depth + 1)))
                    .collect(),
            ),
            _ => v.clone(),
        }
    }
    compact(value, 0)
}

/// A retained terminal receipt is distinct from an ambiguous submission error.
pub(in crate::orchestration) fn terminal_failure(value: &Value) -> bool {
    matches!(
        value["state"].as_str(),
        Some("failed" | "interrupted" | "cancelled")
    )
}
