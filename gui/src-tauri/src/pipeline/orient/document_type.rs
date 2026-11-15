//! The automatic reviews can decline an input in the same call that would
//! otherwise build the orientation map. The response wrapper is for the model
//! call only. Saved workflows and downstream steps use the original map.

use serde_json::{json, Value};

pub(super) fn expected_document(schema: &Value) -> Option<&'static str> {
    if schema["x-pipeline-contract"].as_str() != Some(crate::auto_review::AUTO_REVIEW_CONTRACT) {
        return None;
    }
    let slide_genre = json!([crate::auto_review::SLIDE_DECK_GENRE]);
    if schema.pointer("/properties/review_plan/properties/genre/enum") == Some(&slide_genre) {
        Some("a research slide deck")
    } else {
        Some("an academic paper")
    }
}

pub(super) fn response_schema(mut orientation: Value) -> Value {
    // These root-only annotations belong to the saved orientation contract.
    orientation
        .as_object_mut()
        .expect("validated orientation schema has an object root")
        .retain(|key, _| !key.starts_with("x-pipeline-"));
    json!({
        "type": "object",
        "properties": {
            "orientation": orientation,
            "rejection_reason": {
                "type": "string",
                "description": "One plain-language sentence of at most 300 characters explaining why the input could not be identified as the expected document type."
            }
        }
    })
}

pub(super) fn instruction(expected: &str) -> String {
    format!(
        "First decide whether the supplied content can be identified as {expected}. \
         Judge the content itself, including any extraction notes. Recognize drafts and unconventional formats \
         when the content supports that classification. Uncertainty about a research field or method is not a reason to stop.\n\n\
         If the document fits, put the complete map and review plan in orientation and leave rejection_reason absent or null. \
         Otherwise, leave orientation absent or null and put one plain sentence in rejection_reason. \
         Say what the document appears to contain, or what is unreadable or missing if you cannot tell. \
         For example: It appears to be a restaurant menu listing dishes and prices. \
         Do not select reviewers or invent a map for a rejected document.\n\n\
         The instructions below for the map and plan apply only when the document fits.\n\n"
    )
}

pub(super) enum Response {
    Oriented(Value),
    Rejected(String),
}

pub(super) fn decode(mut value: Value, expected: &str) -> Result<Response, String> {
    let fields = value.as_object_mut().expect("response has an object root");
    let orientation = fields.remove("orientation").unwrap_or(Value::Null);
    let reason = fields.remove("rejection_reason").unwrap_or(Value::Null);
    match (orientation, reason) {
        (orientation, Value::Null) if fields.is_empty() && orientation.is_object() => {
            Ok(Response::Oriented(orientation))
        }
        (Value::Null, Value::String(reason)) if fields.is_empty() => {
            let reason = reason.split_whitespace().collect::<Vec<_>>().join(" ");
            if reason.is_empty() || reason.chars().count() > 300 {
                return Err("rejection_reason must contain 1 to 300 characters".into());
            }
            Ok(Response::Rejected(format!(
                "Review stopped. The document couldn’t be identified as {expected}. {reason}"
            )))
        }
        _ => Err("Return exactly one of orientation or rejection_reason.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auto_review, pipeline::structured};

    #[test]
    fn automatic_review_responses_support_both_outcomes_in_strict_output() {
        for (schema, expected) in [
            (auto_review::orientation_schema(), "an academic paper"),
            (auto_review::quick_orientation_schema(), "an academic paper"),
            (
                auto_review::slide_orientation_schema(),
                "a research slide deck",
            ),
        ] {
            let schema = auto_review::resolve_schema_catalogs(&schema).unwrap();
            assert_eq!(expected_document(&schema), Some(expected));
            let strict = structured::codex_schema(&response_schema(schema))
                .unwrap()
                .unwrap();
            assert_eq!(
                strict["properties"]["orientation"]["type"],
                json!(["object", "null"])
            );
            assert_eq!(
                strict["properties"]["rejection_reason"]["type"],
                json!(["string", "null"])
            );
        }
        assert_eq!(
            expected_document(&crate::orientation_contract::generic_schema()),
            None
        );
    }

    #[test]
    fn a_recognized_document_returns_its_original_map() {
        let map = json!({"metadata": {"title": "Research draft"}, "review_plan": {}});
        let response = json!({"orientation": map, "rejection_reason": null});
        let Response::Oriented(orientation) = decode(response, "an academic paper").unwrap() else {
            panic!("expected the orientation map");
        };
        assert_eq!(orientation, map);
    }

    #[test]
    fn missing_or_conflicting_outcomes_are_validation_errors() {
        for response in [
            json!({}),
            json!({"rejection_reason": "  "}),
            json!({"rejection_reason": 17}),
            json!({"orientation": {}, "rejection_reason": "Looks like a menu."}),
        ] {
            assert!(decode(response, "an academic paper").is_err());
        }
    }
}
