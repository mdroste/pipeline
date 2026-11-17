//! Portable records shared by the built-in and host acceptance boundary.
use serde_json::{json, Value};

fn text() -> Value {
    json!({"type":"string", "minLength":1})
}
fn strings(max: usize) -> Value {
    json!({"type":"array", "maxItems":max, "uniqueItems":true, "items":text()})
}
fn object(properties: Value) -> Value {
    let required: Vec<_> = properties.as_object().unwrap().keys().cloned().collect();
    json!({"type":"object", "required":required, "properties":properties})
}
fn anchor() -> Value {
    object(json!({
        "original":text(),
        "prefix":{"type":"string", "description":"Exact text immediately before original; empty if original is unique."},
        "suffix":{"type":"string", "description":"Exact text immediately after original; enough context to select one occurrence."}
    }))
}
fn coverage() -> Value {
    object(
        json!({"reviewed":strings(200), "limitations":strings(100), "incomplete":{"type":"boolean"}}),
    )
}
fn meaning() -> Value {
    json!({"type":"array", "minItems":1, "maxItems":60, "items":object(json!({
        "source_claim":text(), "preserved_as":text()
    }))})
}
fn unresolved() -> Value {
    json!({"type":"array", "maxItems":60, "items":object(json!({
        "location":text(), "original":text(), "issue":text()
    }))})
}

pub(crate) fn candidates(consolidated: bool) -> Value {
    let stage = if consolidated {
        super::STAGES[1]
    } else {
        super::STAGES[0]
    };
    let mut schema = object(json!({
        "stage":{"type":"string", "enum":[stage]},
        "source_hash":text(), "coverage":coverage(), "unresolved":unresolved(),
        "edits":{"type":"array", "maxItems":60, "description":"A ceiling, not a target. Zero edits is valid.", "items":object(json!({
            "id":text(), "location":text(), "anchor":anchor(),
            "replacement":{"type":"string"},
            "operation":{"type":"string", "enum":["replace","delete"]},
            "reason":text(), "priority":{"type":"string", "enum":["recommended","optional"]},
            "meaning":meaning(), "referee_ids":strings(3), "source_ids":strings(180),
            "requires":strings(60),
            "deletion_basis":{"type":"string", "enum":["not_deletion","empty_framing","redundant"]},
            "retained_at":{"type":"array", "maxItems":20, "items":anchor()}
        }))}
    }));
    schema[super::STAGE_KEY] = json!(stage);
    schema
}

pub(crate) fn verification() -> Value {
    let mut schema = object(json!({
        "stage":{"type":"string", "enum":[super::STAGES[2]]},
        "source_hash":text(), "coverage":coverage(), "unresolved":unresolved(),
        "decisions":{"type":"array", "maxItems":60, "items":object(json!({
            "candidate_id":text(), "decision":{"type":"string", "enum":["accept","reject"]},
            "reason":text(), "meaning":meaning(), "interaction_check":text()
        }))}
    }));
    schema[super::STAGE_KEY] = json!(super::STAGES[2]);
    schema
}
