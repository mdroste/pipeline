//! Host-owned shape checks for the Auto Review orientation schema.
//!
//! The saved schema names the subject, method and genre catalogs by reference
//! instead of listing every ID, so adding a role never requires a schema edit.
//! Before the provider call, the host expands those references to plain enums and
//! validates the response against that same expanded schema. A schema that drops
//! a required field or catalog reference is rejected before any call.

use super::catalog::{GENRES, RESEARCH_ARTICLE_GENRE};
use super::{
    CATALOG_POLICY_KEY, CATALOG_REFERENCE_KEY, GENRE_CATALOG, METHOD_CATALOG, SUBJECT_CATALOG,
};

fn schema_node<'a>(
    schema: &'a serde_json::Value,
    pointer: &str,
) -> Result<&'a serde_json::Value, String> {
    schema
        .pointer(pointer)
        .ok_or_else(|| format!("Auto Review schema is missing {pointer}"))
}

fn require_schema_type(
    schema: &serde_json::Value,
    pointer: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = schema_node(schema, pointer)?
        .get("type")
        .and_then(serde_json::Value::as_str);
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(format!(
            "Auto Review schema {pointer}.type must be '{expected}'"
        ))
    }
}

fn require_schema_required(
    schema: &serde_json::Value,
    pointer: &str,
    expected: &[&str],
) -> Result<(), String> {
    let required = schema_node(schema, pointer)?
        .get("required")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("Auto Review schema {pointer}.required must be an array"))?;
    for key in expected {
        if !required
            .iter()
            .any(|candidate| candidate.as_str() == Some(key))
        {
            return Err(format!(
                "Auto Review schema {pointer}.required must include '{key}'"
            ));
        }
    }
    Ok(())
}

fn require_catalog_reference(
    schema: &serde_json::Value,
    pointer: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = schema_node(schema, pointer)?
        .get(CATALOG_REFERENCE_KEY)
        .and_then(serde_json::Value::as_str);
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(format!(
            "Auto Review schema {pointer}.{CATALOG_REFERENCE_KEY} must be '{expected}'"
        ))
    }
}

/// `genre` is normally backed by the live genre catalog. A profile may instead
/// pin it to an explicit subset of that catalog — Slide Review pins the
/// slide-deck genre — so the matching context reaches every reviewer without
/// depending on the router's classification.
fn require_genre_values(schema: &serde_json::Value, pointer: &str) -> Result<(), String> {
    let node = schema_node(schema, pointer)?;
    if node.get(CATALOG_REFERENCE_KEY).is_some() {
        return require_catalog_reference(schema, pointer, GENRE_CATALOG);
    }
    let values = node
        .get("enum")
        .and_then(serde_json::Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| {
            format!(
                "Auto Review schema {pointer} must set {CATALOG_REFERENCE_KEY} to \
                 '{GENRE_CATALOG}' or enumerate catalog genres"
            )
        })?;
    for value in values {
        let genre = value.as_str().unwrap_or_default();
        if genre != RESEARCH_ARTICLE_GENRE && !GENRES.iter().any(|entry| entry.id == genre) {
            return Err(format!(
                "Auto Review schema {pointer}.enum names unknown genre {value}"
            ));
        }
    }
    Ok(())
}

/// Protect the host-owned Auto Review materialization contract while leaving
/// array bounds and descriptions editable. A profile that removes the marker
/// becomes an ordinary custom workflow; one that retains it must remain a
/// complete Auto Review router schema.
pub(super) fn validate_auto_review_schema_contract(
    schema: &serde_json::Value,
) -> Result<(), String> {
    require_schema_type(schema, "", "object")?;
    if schema
        .get(CATALOG_POLICY_KEY)
        .and_then(serde_json::Value::as_str)
        != Some("live")
    {
        return Err(format!(
            "Auto Review schema {CATALOG_POLICY_KEY} must be 'live'"
        ));
    }
    require_schema_required(
        schema,
        "",
        &[
            "metadata",
            "review_plan",
            "sections",
            "formal_results",
            "tables_figures",
            "notation",
            "stated_contribution",
            "key_references",
            "extraction_quality_notes",
        ],
    )?;
    for (property, expected_type) in [
        ("metadata", "object"),
        ("review_plan", "object"),
        ("sections", "array"),
        ("formal_results", "array"),
        ("tables_figures", "array"),
        ("notation", "array"),
        ("stated_contribution", "string"),
        ("key_references", "array"),
        ("extraction_quality_notes", "array"),
    ] {
        require_schema_type(schema, &format!("/properties/{property}"), expected_type)?;
    }

    let metadata = "/properties/metadata";
    require_schema_required(
        schema,
        metadata,
        &[
            "title",
            "authors",
            "paper_type",
            "has_appendix",
            "has_online_appendix",
        ],
    )?;
    for (property, expected_type) in [
        ("title", "string"),
        ("authors", "array"),
        ("paper_type", "string"),
        ("has_appendix", "boolean"),
        ("has_online_appendix", "boolean"),
    ] {
        require_schema_type(
            schema,
            &format!("{metadata}/properties/{property}"),
            expected_type,
        )?;
    }

    let plan = "/properties/review_plan";
    require_schema_required(
        schema,
        plan,
        &[
            "primary_domain",
            "subject",
            "paper_forms",
            "subject_specialist_ids",
            "method_specialist_ids",
            "genre",
            "selection_notes",
            "routing_uncertainty",
        ],
    )?;
    for (property, expected_type) in [
        ("primary_domain", "string"),
        ("subject", "string"),
        ("paper_forms", "array"),
        ("subject_specialist_ids", "array"),
        ("method_specialist_ids", "array"),
        ("genre", "string"),
        ("selection_notes", "array"),
        ("routing_uncertainty", "array"),
    ] {
        require_schema_type(
            schema,
            &format!("{plan}/properties/{property}"),
            expected_type,
        )?;
    }
    require_catalog_reference(
        schema,
        &format!("{plan}/properties/subject_specialist_ids/items"),
        SUBJECT_CATALOG,
    )?;
    require_catalog_reference(
        schema,
        &format!("{plan}/properties/method_specialist_ids/items"),
        METHOD_CATALOG,
    )?;
    require_genre_values(schema, &format!("{plan}/properties/genre"))?;
    let note = format!("{plan}/properties/selection_notes/items");
    require_schema_type(schema, &note, "object")?;
    require_schema_required(schema, &note, &["id", "reason"])?;
    require_schema_type(schema, &format!("{note}/properties/id"), "string")?;
    require_schema_type(schema, &format!("{note}/properties/reason"), "string")?;
    Ok(())
}
