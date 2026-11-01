//! What a sender's statuses may become in the importing store.
//!
//! Acceptance, human confirmation, and passed checks are decisions made on the
//! sender's computer, so a package can never make them here. Imported records
//! arrive as proposals, and the sender's status stays visible in the record's
//! origin or assessor (`research::imported_status` reads it). Statuses that grant
//! nothing, such as rejected or retired, travel unchanged. `.pwrx` restore brings
//! back the researcher's own store and never uses this policy.
use super::{str_of, text};
use crate::workbench::research::{imported_mark, split_imported_mark};
use serde_json::{json, Value};

/// A note's local state and origin.
pub(super) fn note_status(value: &Value) -> (String, String) {
    let state = str_of(value, "state").unwrap_or_else(|| "proposed".into());
    let origin = text(value, "origin");
    if state == "accepted" {
        ("proposed".into(), imported_mark(&state, &origin))
    } else {
        (state, origin)
    }
}

/// A claim's local state, and the sender's status when it was held back.
pub(super) fn claim_status(value: &Value) -> (String, Option<String>) {
    let state = str_of(value, "workflowState").unwrap_or_else(|| "proposed".into());
    if state == "accepted" {
        ("proposed".into(), Some(state))
    } else {
        (state, None)
    }
}

/// An evidence link's local assessment and assessor.
pub(super) fn evidence_status(value: &Value) -> (String, String) {
    let assessment = text(value, "assessment");
    let assessor = text(value, "assessor");
    if matches!(assessment.as_str(), "human_confirmed" | "check_passed") {
        ("not_checked".into(), imported_mark(&assessment, &assessor))
    } else {
        (assessment, assessor)
    }
}

/// Marks a receipt or check record as the sender's, whatever the package claims.
pub(super) fn stamp(value: &Value, namespace: &str) -> Value {
    let mut stamped = match value {
        Value::Object(_) => value.clone(),
        Value::Null => json!({}),
        other => json!({"senderValue": other}),
    };
    stamped["importedFrom"] = json!(namespace);
    stamped
}

fn restore(value: &mut Value, marked: &str, status: &str, held: &str) {
    let Some((sender_status, original)) = value
        .get(marked)
        .and_then(Value::as_str)
        .and_then(split_imported_mark)
        .map(|(status, original)| (status.to_string(), original.to_string()))
    else {
        return;
    };
    value[marked] = json!(original);
    if value.get(status).and_then(Value::as_str) == Some(held) {
        value[status] = json!(sender_status);
    }
}

/// The record as its package stated it, so holding a status back never reads
/// as a change, while a decision made here still does.
pub(super) fn as_sent(kind: &str, value: &mut Value) {
    match kind {
        "note" => restore(value, "origin", "state", "proposed"),
        "evidence" => restore(value, "assessor", "assessment", "not_checked"),
        "claim" => {
            restore(value, "origin", "workflowState", "proposed");
            let versions = value.get_mut("versions").and_then(Value::as_array_mut);
            for version in versions.into_iter().flatten() {
                let original = version
                    .get("origin")
                    .and_then(Value::as_str)
                    .and_then(split_imported_mark)
                    .map(|(_, original)| original.to_string());
                if let Some(original) = original {
                    version["origin"] = json!(original);
                }
            }
        }
        _ => {}
    }
}
