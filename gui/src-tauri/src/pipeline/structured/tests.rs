use super::*;

#[test]
fn extract_plain_json() {
    assert_eq!(extract_json("{\"a\":1}"), Some(serde_json::json!({"a":1})));
    assert_eq!(extract_json("[1,2,3]"), Some(serde_json::json!([1, 2, 3])));
}

#[test]
fn extract_from_fence_and_prose() {
    let fenced = "Here you go:\n```json\n{\"a\": 1}\n```\nDone.";
    assert_eq!(extract_json(fenced), Some(serde_json::json!({"a":1})));

    let prose = "The result is {\"ok\": true} as requested.";
    assert_eq!(extract_json(prose), Some(serde_json::json!({"ok":true})));
}

#[test]
fn extract_balanced_span_ignores_braces_in_strings() {
    let text = "prefix {\"note\": \"a } brace in a string\"} suffix";
    assert_eq!(
        extract_json(text),
        Some(serde_json::json!({"note": "a } brace in a string"}))
    );
}

#[test]
fn extract_skips_malformed_candidates_and_fences() {
    let text = "broken [not json]\n```json\n{still broken}\n```\nresult: [{\"ok\":true}]";
    assert_eq!(extract_json(text), Some(serde_json::json!([{"ok": true}])));
}

#[test]
fn extract_handles_mixed_nested_delimiters() {
    let text = r#"prefix {"items":[{"text":"} ]"}]} suffix"#;
    assert_eq!(
        extract_json(text),
        Some(serde_json::json!({"items": [{"text": "} ]"}]}))
    );
}

#[test]
fn extract_returns_none_for_non_json() {
    assert!(extract_json("just some prose, no json here").is_none());
}

#[test]
fn validate_object_required_and_types() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["issues"],
        "properties": { "issues": { "type": "array" } }
    });
    assert!(validate(&schema, &serde_json::json!({"issues": []})).is_ok());
    let err = validate(&schema, &serde_json::json!({"other": 1})).unwrap_err();
    assert!(err.contains("missing required property 'issues'"));
    let err2 = validate(&schema, &serde_json::json!({"issues": "nope"})).unwrap_err();
    assert!(err2.contains("expected type array"));
}

#[test]
fn validate_array_items_required_keys() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["id", "severity"]
                }
            }
        }
    });
    let good = serde_json::json!({"items": [{"id": "1", "severity": "high"}]});
    assert!(validate(&schema, &good).is_ok());
    let bad = serde_json::json!({"items": [{"id": "1"}]});
    assert!(validate(&schema, &bad).unwrap_err().contains("severity"));
}

#[test]
fn validates_enum_and_bounded_unique_arrays() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["methods"],
        "properties": {
            "methods": {
                "type": "array",
                "minItems": 2,
                "maxItems": 3,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["proofs", "statistics", "computation"]
                }
            }
        }
    });
    assert!(validate(
        &schema,
        &serde_json::json!({"methods": ["proofs", "statistics"]})
    )
    .is_ok());
    assert!(validate(&schema, &serde_json::json!({"methods": ["proofs"]})).is_err());
    assert!(validate(
        &schema,
        &serde_json::json!({"methods": ["proofs", "proofs"]})
    )
    .is_err());
    assert!(validate(
        &schema,
        &serde_json::json!({"methods": ["proofs", "unknown"]})
    )
    .is_err());
}

#[test]
fn check_end_to_end() {
    let schema = serde_json::json!({ "type": "object", "required": ["ok"] });
    assert!(check(&schema, "```json\n{\"ok\": true}\n```").is_ok());
    assert!(check(&schema, "not json").is_err());
    assert!(check(&schema, "{\"nope\": 1}").is_err());
}

#[test]
fn canonicalize_rejects_prose_and_owns_serialization() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["ok"],
        "properties": {"ok": {"type": "boolean"}}
    });
    assert_eq!(
        canonicalize(&schema, "{\"ok\":true}").unwrap(),
        "{\n  \"ok\": true\n}"
    );
    assert!(canonicalize(&schema, "Here: {\"ok\":true}").is_err());
    assert!(canonicalize(&schema, "```json\n{\"ok\":true}\n```").is_err());
}

#[test]
fn provider_schema_projects_only_the_portable_structural_core() {
    let schema = serde_json::json!({
        "x-pipeline-contract": crate::auto_review::AUTO_REVIEW_CONTRACT,
        "type": "object",
        "required": ["findings"],
        "properties": {
            "findings": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {"type": "string", "description": "Stable id"}
                    }
                }
            }
        }
    });
    let projected = provider_schema(&schema).unwrap();
    assert_eq!(projected["type"], "object");
    assert_eq!(projected["properties"]["findings"]["maxItems"], 3);
    assert_eq!(
        projected["properties"]["findings"]["items"]["required"],
        serde_json::json!(["id"])
    );
    assert!(projected["properties"]["findings"]
        .get("uniqueItems")
        .is_none());
    assert!(projected.get("x-pipeline-contract").is_none());
}

#[test]
fn malformed_and_unknown_schemas_are_rejected() {
    assert!(validate_schema(&serde_json::json!([])).is_err());
    assert!(validate_schema(&serde_json::json!({"type": "date"}))
        .unwrap_err()
        .contains("unsupported type"));
    assert!(
        validate_schema(&serde_json::json!({"type": "object", "required": ["id", 2]})).is_err()
    );
    assert!(validate_schema(&serde_json::json!({"type": "object", "properties": {"bad": {"type": "string", "items": {}}}})).is_err());
    assert!(validate_schema(
        &serde_json::json!({"type": "object", "properties": {"bad": {"type": "array", "minItems": 3, "maxItems": 2}}})
    )
    .is_err());
    assert!(validate_schema(&serde_json::json!({"type": "object", "properties": {"bad": {"type": "string", "enum": []}}})).is_err());
    assert!(
        validate_schema(&serde_json::json!({"type": "object", "properties": {"bad": {"type": "array", "uniqueItems": "yes"}}})).is_err()
    );
    assert!(
        validate_schema(&serde_json::json!({"type": "array", "items": {"type": "string"}}))
            .unwrap_err()
            .contains("root must explicitly be 'object'")
    );
    assert!(validate_schema(&serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"name": {"type": "string", "minLength": 2}}
    }))
    .unwrap_err()
    .contains("unsupported keyword"));
    assert!(validate_schema(&serde_json::json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "x-pipeline-contract": crate::auto_review::AUTO_REVIEW_CONTRACT}
        }
    }))
    .unwrap_err()
    .contains("only valid at the schema root"));
}

#[test]
fn codex_strict_projection_satisfies_the_strict_responses_dialect() {
    // The exact failure seen in the field: the resolved Auto Review
    // orientation schema must reach Codex with additionalProperties:false
    // on every object and every property required.
    let resolved =
        crate::auto_review::resolve_schema_catalogs(&crate::auto_review::orientation_schema())
            .unwrap();
    let strict = strictify(&provider_schema(&resolved).unwrap()).unwrap();
    fn assert_strict(value: &serde_json::Value, path: &str) {
        let object = value.as_object().unwrap();
        match object.get("type").and_then(serde_json::Value::as_str) {
            Some("object") => {
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&serde_json::Value::Bool(false)),
                    "{path} lacks additionalProperties:false"
                );
                let properties = object["properties"].as_object().unwrap();
                let required = object["required"].as_array().unwrap();
                assert_eq!(required.len(), properties.len(), "{path} required set");
                for (key, child) in properties {
                    assert_strict(child, &format!("{path}.{key}"));
                }
            }
            Some("array") => assert_strict(&object["items"], &format!("{path}[]")),
            _ => {}
        }
    }
    assert_strict(&strict, "$");
}

#[test]
fn optional_fields_widen_to_null_and_strip_back_out() {
    let schema = crate::findings::output_schema();
    let strict = strictify(&provider_schema(&schema).unwrap()).unwrap();
    let item = &strict["properties"]["findings"]["items"];
    // Optional fields become required-but-nullable in the strict form.
    assert!(item["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key == "source_key"));
    assert_eq!(
        item["properties"]["source_key"]["type"],
        serde_json::json!(["string", "null"])
    );

    // A strict-transport response spelling absence as null canonicalizes
    // cleanly against the original contract, with the nulls removed.
    let response = serde_json::json!({
      "schema_version": 2, "taxonomy": crate::findings::CATEGORIES,
      "findings": [{
        "rank": 1, "id": "a", "source_key": null,
        "reviewer_ids": ["reviewer"], "source_call_ids": null,
        "title": "T", "category": crate::findings::CATEGORIES[0],
        "severity": "high", "confidence": "high", "verification_status": "unverified",
        "problem": "Problem", "consequence": "Consequence", "recommended_action": "Fix",
        "evidence": [{"evidence_type": "document", "verification_status": "unverified",
                      "page": 3, "line_start": null, "line_end": null,
                      "node_id": null, "asset_id": null, "artifact_path": null,
                      "source_path": null, "source_hash": null,
                      "url": null, "doi": null, "publisher": null, "accessed_at": null,
                      "query_id": null, "call_id": null,
                      "description": "page 3", "quote": null}]
    }]});
    let canonical = canonicalize(&schema, &response.to_string()).unwrap();
    assert!(!canonical.contains("null"));
    assert!(canonical.contains("\"page\": 3"));
    // A required field may not hide behind null.
    let mut broken = response;
    broken["findings"][0]["id"] = serde_json::Value::Null;
    assert!(canonicalize(&schema, &broken.to_string()).is_err());
}

#[test]
fn inexpressible_schemas_skip_the_codex_constraint_instead_of_failing() {
    // The deliberately schema-light object-root contract cannot be made
    // strict; the call proceeds without a native constraint.
    assert!(
        prepare_codex_schema_file(&serde_json::json!({"type": "object"}))
            .unwrap()
            .is_none()
    );
    let concrete = prepare_codex_schema_file(&crate::output::text_artifact_schema())
        .unwrap()
        .unwrap();
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&concrete.path).unwrap()).unwrap();
    assert_eq!(written["additionalProperties"], false);
}

#[test]
fn schema_references_resolve_to_live_contracts_and_carry_markers() {
    let marker = serde_json::json!({
        "type": "object",
        SCHEMA_REFERENCE_KEY: "findings-v1",
        PRESERVE_FINDINGS_KEY: "auto_synthesis",
    });
    validate_schema(&marker).unwrap();
    let resolved = resolve_schema_reference(&marker).unwrap();
    assert!(resolved.get("properties").is_some());
    assert_eq!(
        resolved.get(PRESERVE_FINDINGS_KEY),
        Some(&serde_json::json!("auto_synthesis"))
    );
    assert!(resolved.get(SCHEMA_REFERENCE_KEY).is_none());
    provider_schema(&resolved).unwrap();

    assert!(resolve_schema_reference(&serde_json::json!({
        "type": "object", SCHEMA_REFERENCE_KEY: "unknown-v9"
    }))
    .unwrap_err()
    .contains("unknown Pipeline schema reference"));

    // An unresolved reference must never reach a provider, and structural
    // keywords beside a reference are rejected rather than discarded.
    assert!(provider_schema(&marker)
        .unwrap_err()
        .contains("must be resolved"));
    assert!(validate_schema(&serde_json::json!({
        "type": "object",
        SCHEMA_REFERENCE_KEY: "findings-v1",
        "properties": {"x": {"type": "string"}}
    }))
    .unwrap_err()
    .contains("replaces the whole contract"));
    assert!(resolve_schema_reference(&serde_json::json!({
        "type": "object",
        SCHEMA_REFERENCE_KEY: "findings-v1",
        "properties": {"x": {"type": "string"}}
    }))
    .unwrap_err()
    .contains("replaces the whole contract"));
}

#[test]
fn min_length_is_host_checked_and_not_projected() {
    let schema = serde_json::json!({
        "type": "object",
        "required": ["id"],
        "properties": {"id": {"type": "string", "minLength": 1}}
    });
    validate_schema(&schema).unwrap();
    assert!(validate(&schema, &serde_json::json!({"id": "x"})).is_ok());
    assert!(validate(&schema, &serde_json::json!({"id": ""}))
        .unwrap_err()
        .contains("at least 1 characters"));
    let projected = provider_schema(&schema).unwrap();
    assert!(projected["properties"]["id"].get("minLength").is_none());
    assert!(validate_schema(
        &serde_json::json!({"type": "object", "properties": {"n": {"type": "integer", "minLength": 1}}})
    )
    .unwrap_err()
    .contains("only valid for a string schema"));
}

#[test]
fn provider_schema_rejects_payloads_too_large_for_portable_cli_transport() {
    let schema = serde_json::json!({
        "type": "object",
        "description": "x".repeat(MAX_PROVIDER_SCHEMA_BYTES),
    });
    assert!(provider_schema(&schema)
        .unwrap_err()
        .contains("portable limit"));
}

#[test]
fn provider_schema_rejects_enums_outside_the_common_provider_limits() {
    let values = (0..=MAX_PROVIDER_ENUM_VALUES)
        .map(|index| format!("value_{index}"))
        .collect::<Vec<_>>();
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"value": {"type": "string", "enum": values}}
    });
    assert!(provider_schema(&schema)
        .unwrap_err()
        .contains("enum values"));
}
