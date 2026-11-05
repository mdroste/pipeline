//! The designer's schema check, held to the backend's.
//!
//! `outputSchemaError` keeps the schema editor responsive while a draft is
//! typed, and only Rust decides what saves, so the two share this fixture.
//! `contracts` must equal what the backend really publishes.

use crate::pipeline::structured::{resolve_schema_reference, SCHEMA_REFERENCE_KEY};
use serde_json::{json, Value};

const FIXTURE: &str = include_str!("../../../../tests/fixtures/workflow/schema-dialect.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn published_contracts() -> Value {
    let grant = super::super::builtins::grant_review_profile();
    let schema_of = |id: &str| {
        grant
            .steps
            .iter()
            .find(|step| step.id == id)
            .and_then(|step| step.output_schema.clone())
            .unwrap()
    };
    json!({
        "findings": crate::findings::output_schema(),
        "grant_synthesis": schema_of("grant_synthesis"),
        "grant_validate": schema_of("grant_validate"),
    })
}

#[test]
fn the_fixture_holds_the_contracts_the_backend_publishes() {
    assert_eq!(fixture()["contracts"], published_contracts());
}

#[test]
fn accepted_and_refused_schemas_match_the_backend() {
    let fixture = fixture();
    for (name, schema) in fixture["contracts"].as_object().unwrap() {
        resolve_schema_reference(schema).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    for case in fixture["accepted"].as_array().unwrap() {
        resolve_schema_reference(&case["schema"]).unwrap_or_else(|error| panic!("{case}: {error}"));
    }
    for case in fixture["refused"].as_array().unwrap() {
        assert!(
            resolve_schema_reference(&case["schema"]).is_err(),
            "{} should be refused",
            case["name"]
        );
    }
}

#[test]
fn the_reference_preview_is_what_the_structure_view_shows() {
    let fixture = fixture();
    let reference = &fixture["reference"];
    assert!(reference["schema"].get(SCHEMA_REFERENCE_KEY).is_some());
    let preview = tauri::async_runtime::block_on(crate::commands::config::preview_provider_schema(
        reference["schema"].clone(),
    ))
    .unwrap();
    assert_eq!(preview.projected, reference["projected"]);
}
