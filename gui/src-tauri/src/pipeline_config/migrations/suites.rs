//! Exact stock-suite, orientation-contract, and rendered-prompt upgrades.

use super::super::{
    auto_review_profile, grant_review_profile, quick_auto_review_profile, read_profile_file,
    restore_profile_bytes, validate_profile_data, ProfileData, GRANT_REVIEW_ID,
    PAPER_REVIEW_FAST_ID, PAPER_REVIEW_ID,
};
use super::prompts::prompt_digest;
use super::retirement::archive_retired_profile;
use super::validated_for_migration;
use std::path::Path;

/// Replace a legacy built-in Grant Review suite with its typed, validated
/// successor. The archived copy keeps any user edits recoverable.
pub(in crate::pipeline_config) fn refresh_stale_grant_review_suite(
    profiles: &Path,
) -> Result<(), String> {
    let id = GRANT_REVIEW_ID;
    let path = profiles.join(format!("{id}.json"));
    if !path.exists() {
        return Ok(());
    }
    let content = read_profile_file(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let profile: ProfileData = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
    // The free-form suite that shipped (0.9.x: grant_aims, grant_feasibility,
    // grant_clarity, grant_consistency, grant_synthesis) has a synthesis step
    // and no typed validation step; the earlier key set never shipped.
    let has_legacy_skeleton = profile
        .steps
        .iter()
        .any(|step| step.id == "grant_synthesis")
        && !profile.steps.iter().any(|step| step.id == "grant_validate");
    let is_current = profile.steps.iter().any(|step| step.id == "grant_validate")
        && profile
            .steps
            .iter()
            .find(|step| step.id == "grant_validate")
            .and_then(|step| step.output_schema.as_ref())
            .and_then(|schema| schema.get(crate::pipeline::structured::SCHEMA_REFERENCE_KEY))
            .and_then(serde_json::Value::as_str)
            == Some("findings-v2-validation");
    if !has_legacy_skeleton || is_current {
        return Ok(());
    }

    archive_retired_profile(profiles, id)?;
    let replacement = grant_review_profile();
    validate_profile_data(&replacement)?;
    let json = serde_json::to_string_pretty(&replacement)
        .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
    restore_profile_bytes(&path, json.as_bytes())
        .map_err(|error| format!("Failed to write '{}': {error}", path.display()))
}

/// The v2 Auto Review contract changes the whole step suite (structured
/// specialist output, consolidated `sources`, host-enforced id lineage), not
/// just the orientation pair, so a stale built-in profile cannot be upgraded
/// field-by-field. Archive the user's copy beside the other retired profiles
/// and write the current definition; the archived file preserves any edits.
pub(in crate::pipeline_config) fn refresh_stale_auto_review_suites(
    profiles: &Path,
) -> Result<(), String> {
    for (id, replacement) in [
        (PAPER_REVIEW_ID, auto_review_profile()),
        (PAPER_REVIEW_FAST_ID, quick_auto_review_profile()),
    ] {
        let path = profiles.join(format!("{id}.json"));
        if !path.exists() {
            continue;
        }
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        let has_auto_review_skeleton = profile.steps.iter().any(|step| {
            matches!(
                step.id.as_str(),
                "auto_contribution"
                    | "auto_consistency"
                    | "auto_exposition"
                    | "auto_synthesis"
                    | "auto_validate"
            )
        });
        let current_contract = profile.orientation_schema.as_ref().is_some_and(|schema| {
            schema
                .get("x-pipeline-contract")
                .and_then(serde_json::Value::as_str)
                == Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
        });
        if !has_auto_review_skeleton || current_contract {
            continue;
        }
        archive_retired_profile(profiles, id)?;
        let json = serde_json::to_string_pretty(&replacement)
            .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
        restore_profile_bytes(&path, json.as_bytes())
            .map_err(|error| format!("Failed to write '{}': {error}", path.display()))?;
    }
    Ok(())
}

pub(in crate::pipeline_config) fn refresh_builtin_auto_review_contracts(
    profiles: &Path,
) -> Result<(), String> {
    for (id, replacement) in [
        (PAPER_REVIEW_ID, auto_review_profile()),
        (PAPER_REVIEW_FAST_ID, quick_auto_review_profile()),
    ] {
        let path = profiles.join(format!("{id}.json"));
        if !path.exists() {
            continue;
        }
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let mut profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        let has_auto_review_skeleton = profile.steps.iter().any(|step| {
            matches!(
                step.id.as_str(),
                "auto_contribution"
                    | "auto_consistency"
                    | "auto_exposition"
                    | "auto_synthesis"
                    | "auto_validate"
            )
        });
        if !has_auto_review_skeleton {
            continue;
        }
        let current_contract = profile.orientation_schema.as_ref().is_some_and(|schema| {
            schema
                .get("x-pipeline-contract")
                .and_then(serde_json::Value::as_str)
                == Some(crate::auto_review::AUTO_REVIEW_CONTRACT)
                && crate::auto_review::validate_schema_settings(schema).is_ok()
        });
        if current_contract {
            continue;
        }

        profile.orientation_prompt = replacement.orientation_prompt;
        profile.orientation_schema = replacement.orientation_schema;
        if !validated_for_migration(&path, &profile, "structured-contract") {
            continue;
        }
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
        restore_profile_bytes(&path, json.as_bytes())
            .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
    }
    Ok(())
}

pub(in crate::pipeline_config) fn compact_builtin_auto_review_prompts(
    profiles: &Path,
) -> Result<(), String> {
    let template = crate::auto_review::orientation_prompt();
    let expanded = crate::auto_review::expand_orientation_prompt(&template);
    for id in [PAPER_REVIEW_ID, PAPER_REVIEW_FAST_ID] {
        let path = profiles.join(format!("{id}.json"));
        if !path.exists() {
            continue;
        }
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let mut profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        let current_contract = profile
            .orientation_schema
            .as_ref()
            .and_then(|schema| schema.get("x-pipeline-contract"))
            .and_then(serde_json::Value::as_str)
            == Some(crate::auto_review::AUTO_REVIEW_CONTRACT);
        if !current_contract || profile.orientation_prompt != expanded {
            continue;
        }
        profile.orientation_prompt = template.clone();
        if !validated_for_migration(&path, &profile, "compact-prompt") {
            continue;
        }
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
        restore_profile_bytes(&path, json.as_bytes())
            .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
    }
    Ok(())
}

/// Refresh the prompt-bearing fields changed by the prompt-parsimony audit.
/// Digest matching is intentionally per field: a user may customize other
/// parts of a built-in workflow while retaining a stock prompt.
pub(in crate::pipeline_config) fn refresh_parsimonious_prompt_defaults(
    profiles: &Path,
) -> Result<(), String> {
    const OLD_AUTO_ORIENTATION: &str =
        "e0b2d3d953d5d78930916600797e1774b0d581e499225d4fc105be826cb58436";
    const OLD_AUTO_CONTRIBUTION: &str =
        "6853ad3d426a84b59a16db96b9b5ad1cfe57e1ba4276bfdb6a2d97fd3de23e10";
    const OLD_AUTO_CONSISTENCY: &str =
        "b3e3f19325e773e221ea5552d472dfdf3f75833ed68f29f0066d71d94a5245f5";
    const OLD_AUTO_EXPOSITION: &str =
        "2f06472360fe849b7a7a9839a5243ef2195afb1f0d6f19e2bf0a7e1616691b4b";
    const OLD_AUTO_VALIDATE: &str =
        "054e15ab830b0bce611fcef0d64fe84d325889a69a32e7424f306ad2c75b67ba";
    const OLD_GENERIC_ORIENTATION: &str =
        "eb32d99a757c30776d0171cde277fdbc71cd1188625c27ab84a94208b30cc972";
    const OLD_GRANT_CLARITY: &str =
        "5247ba38f4fb752ff31311f81167f13c7e97a4d9d61c9c123fad371113027cce";
    const OLD_GRANT_CONSISTENCY: &str =
        "efa397ef674b57f0bb609e675d65e1e0220827923662f63d46550be3df671ac5";

    for (id, replacement) in [
        (PAPER_REVIEW_ID, auto_review_profile()),
        (PAPER_REVIEW_FAST_ID, quick_auto_review_profile()),
    ] {
        let path = profiles.join(format!("{id}.json"));
        if !path.exists() {
            continue;
        }
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let mut profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        let mut changed = false;

        if prompt_digest(&profile.orientation_prompt) == OLD_AUTO_ORIENTATION {
            profile.orientation_prompt = replacement.orientation_prompt.clone();
            changed = true;
        }
        for step in &mut profile.steps {
            let old_digest = match step.id.as_str() {
                "auto_contribution" => Some(OLD_AUTO_CONTRIBUTION),
                "auto_consistency" => Some(OLD_AUTO_CONSISTENCY),
                "auto_exposition" => Some(OLD_AUTO_EXPOSITION),
                "auto_validate" => Some(OLD_AUTO_VALIDATE),
                _ => None,
            };
            if old_digest.is_some_and(|digest| prompt_digest(&step.prompt) == digest) {
                if let Some(new_step) = replacement
                    .steps
                    .iter()
                    .find(|candidate| candidate.id == step.id)
                {
                    step.prompt = new_step.prompt.clone();
                    changed = true;
                }
            }
        }
        if changed && validated_for_migration(&path, &profile, "prompt-parsimony") {
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
    }

    let path = profiles.join(format!("{GRANT_REVIEW_ID}.json"));
    if path.exists() {
        let content = read_profile_file(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        let mut profile: ProfileData = serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
        let replacement = grant_review_profile();
        let stock_schema = Some(crate::orientation_contract::generic_schema());
        let mut changed = false;
        if prompt_digest(&profile.orientation_prompt) == OLD_GENERIC_ORIENTATION
            && profile.orientation_schema == stock_schema
        {
            profile.orientation_prompt = replacement.orientation_prompt.clone();
            profile.orientation_schema = replacement.orientation_schema.clone();
            changed = true;
        }
        for step in &mut profile.steps {
            let old_digest = match step.id.as_str() {
                "grant_clarity" => Some(OLD_GRANT_CLARITY),
                "grant_consistency" => Some(OLD_GRANT_CONSISTENCY),
                _ => None,
            };
            if old_digest.is_some_and(|digest| prompt_digest(&step.prompt) == digest) {
                if let Some(new_step) = replacement
                    .steps
                    .iter()
                    .find(|candidate| candidate.id == step.id)
                {
                    step.prompt = new_step.prompt.clone();
                    changed = true;
                }
            }
        }
        if changed && validated_for_migration(&path, &profile, "prompt-parsimony") {
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
    }

    Ok(())
}
