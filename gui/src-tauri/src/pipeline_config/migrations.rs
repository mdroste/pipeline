use super::*;

/// Apply catalog changes to profiles created by older Pipeline releases.
pub(super) fn migrate_builtin_catalog(profiles: &Path) -> Result<(), String> {
    let marker = profiles.join(".builtin-catalog-v3");
    if !marker.exists() {
        for (id, replacement) in RETIRED_BUILTIN_PROFILES {
            // Do not leave Settings pointing at a profile archived below.
            let _ = crate::settings::replace_active_profile_if(id, replacement);
            archive_retired_profile(profiles, id)?;
        }
        fs::write(&marker, b"paper-and-code-profile-catalog\n").map_err(|error| {
            format!(
                "Failed to record the built-in profile catalog migration '{}': {error}",
                marker.display()
            )
        })?;
    }

    let retired_v9_marker = profiles.join(".builtin-catalog-v9");
    if !retired_v9_marker.exists() {
        for (id, replacement) in V9_RETIRED_BUILTIN_PROFILES {
            if profiles.join(format!("{id}.json")).exists() {
                // Do not leave Settings pointing at a profile archived below.
                let _ = crate::settings::replace_active_profile_if(id, replacement);
                archive_retired_profile(profiles, id)?;
            }
        }
        fs::write(&retired_v9_marker, b"paper-and-grant-profile-catalog\n").map_err(|error| {
            format!(
                "Failed to record the retired-profile catalog migration '{}': {error}",
                retired_v9_marker.display()
            )
        })?;
    }

    // Paper Review (Full) and (Quick) retired in favor of Auto Paper Review.
    // The active-profile swap is unconditional (v3 style, not v9 style):
    // earlier swaps in this function retarget retired IDs to "deep-review",
    // and on an installation that never created that profile file the swap
    // must still land on "auto-review" rather than a nonexistent profile.
    let retired_v15_marker = profiles.join(".builtin-catalog-v15");
    if !retired_v15_marker.exists() {
        for (id, replacement) in V15_RETIRED_BUILTIN_PROFILES {
            // Do not leave Settings pointing at a profile archived below.
            let _ = crate::settings::replace_active_profile_if(id, replacement);
            archive_retired_profile(profiles, id)?;
        }
        fs::write(&retired_v15_marker, b"auto-paper-review-default-catalog\n").map_err(
            |error| {
                format!(
                    "Failed to record the retired-profile catalog migration '{}': {error}",
                    retired_v15_marker.display()
                )
            },
        )?;
    }

    // The v2 structured Auto Review contract replaces the whole built-in
    // suite (specialist output schemas, `sources`, host-enforced id lineage).
    // This must run before the older orientation-pair refresh below, so a
    // pre-v2 store is archived and recreated once rather than half-upgraded.
    let structured_auto_review_marker = profiles.join(".structured-auto-review-contract-v2");
    if !structured_auto_review_marker.exists() {
        refresh_stale_auto_review_suites(profiles)?;
        fs::write(
            &structured_auto_review_marker,
            b"structured-specialist-auto-review\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the structured Auto Review migration '{}': {error}",
                structured_auto_review_marker.display()
            )
        })?;
    }

    // The modular manifest catalog deliberately starts a new, single Auto
    // Review contract. Development builds may already have written a built-in
    // profile with the retired hard-coded-catalog contract; such a profile is
    // rejected by current validation before the editor can offer its reset
    // action. Replace only the host-owned orientation prompt/schema pair and
    // preserve the rest of the workflow, including user edits to its steps.
    // Use a semantic marker rather than the historical numeric namespace:
    // unreleased development stores may contain now-retired v11–v18 markers.
    let modular_auto_review_marker = profiles.join(".modular-auto-review-contract-v1");
    if !modular_auto_review_marker.exists() {
        refresh_builtin_auto_review_contracts(profiles)?;
        fs::write(
            &modular_auto_review_marker,
            b"modular-auto-review-contract\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the Auto Review contract migration '{}': {error}",
                modular_auto_review_marker.display()
            )
        })?;
    }

    // Earlier development builds saved the fully rendered specialist catalog
    // in the prompt. Compact exact stock prompts back to the editable template;
    // user-authored prompts are never rewritten. Runtime expands the live
    // catalog immediately before the provider call.
    let compact_auto_review_prompt_marker = profiles.join(".compact-auto-review-prompt-v1");
    if !compact_auto_review_prompt_marker.exists() {
        compact_builtin_auto_review_prompts(profiles)?;
        fs::write(
            &compact_auto_review_prompt_marker,
            b"runtime-expanded-auto-review-catalog\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the compact Auto Review prompt migration '{}': {error}",
                compact_auto_review_prompt_marker.display()
            )
        })?;
    }

    // Remove instructions duplicated by host wrappers and structured schemas,
    // and give Grant Review a proposal-specific survey. Update only exact
    // prior stock prompts; workflow-local edits remain untouched.
    let prompt_parsimony_marker = profiles.join(".prompt-parsimony-v1");
    if !prompt_parsimony_marker.exists() {
        refresh_parsimonious_prompt_defaults(profiles)?;
        fs::write(
            &prompt_parsimony_marker,
            b"parsimonious-review-prompts-and-grant-survey\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the prompt-parsimony migration '{}': {error}",
                prompt_parsimony_marker.display()
            )
        })?;
    }

    // Grant Review now shares the same typed findings and validation contract
    // as Auto Review. A legacy grant suite cannot be upgraded safely one field
    // at a time because its terminal output was free-form and it had no
    // validation step. Preserve the old file in the retired-builtins archive,
    // then install the current definition. Profiles that merely reuse the
    // built-in id but do not have the grant-review skeleton are left alone.
    let structured_grant_review_marker = profiles.join(".structured-grant-review-contract-v1");
    if !structured_grant_review_marker.exists() {
        refresh_stale_grant_review_suite(profiles)?;
        fs::write(
            &structured_grant_review_marker,
            b"typed-validated-grant-review\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the structured Grant Review migration '{}': {error}",
                structured_grant_review_marker.display()
            )
        })?;
    }

    // Artifact access is part of the workflow definition, not an ambient
    // executor default. Refresh every shipped profile into the explicit
    // producer/role format. This must run before migrations that load and
    // validate profiles: older profiles declared Read/Write directly, while
    // the current validator permits only optional external capabilities.
    let artifact_marker = profiles.join(".builtin-catalog-v6");
    if !artifact_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            profile.steps = configure_artifact_flow(
                profile.steps,
                &profile.extraction.input_mode,
                builtin_primary_readers(id),
            );
            validate_profile_data(&profile)?;
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
        fs::write(&artifact_marker, b"explicit-step-artifact-context\n").map_err(|error| {
            format!(
                "Failed to record the artifact-context migration '{}': {error}",
                artifact_marker.display()
            )
        })?;
    }

    // (The pre-v15 cosmetic rename of deep-review/quick-review display names
    // is gone: those files are archived above, and a custom profile that
    // later reuses one of the freed IDs must never be parsed or renamed by
    // startup migrations.)

    // Refresh shipped prompt text only when a built-in still contains the
    // exact prior default. Hash matching preserves every customized prompt,
    // while ensuring existing installations receive the cleaner synthesis and
    // merge defaults rather than only newly created profiles.
    let prompt_marker = profiles.join(".builtin-catalog-v4");
    if !prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_shipped_prompt_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(&prompt_marker, b"clean-terminal-report-prompts\n").map_err(|error| {
            format!(
                "Failed to record the prompt catalog migration '{}': {error}",
                prompt_marker.display()
            )
        })?;
    }

    // Reuse warmed primary context and encourage batched evidence retrieval.
    // Update only exact prior stock prompts so user customizations remain
    // byte-for-byte intact. This must run before the v7 whole-profile
    // fingerprint: `full_review_profile` already contains the new defaults.
    let retrieval_prompt_marker = profiles.join(".builtin-catalog-v8");
    if !retrieval_prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_efficient_retrieval_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &retrieval_prompt_marker,
            b"shared-context-aware-batched-retrieval-prompts\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the retrieval-prompt migration '{}': {error}",
                retrieval_prompt_marker.display()
            )
        })?;
    }

    // Tighten the Paper Review prompts around evidence, prioritization, and
    // false-positive control. Update only exact prior defaults so edits made
    // in the workflow editor remain untouched. This precedes the v7 whole-
    // profile fingerprint because `full_review_profile` contains the new text.
    let review_quality_prompt_marker = profiles.join(".builtin-catalog-v10");
    if !review_quality_prompt_marker.exists() {
        for id in BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_review_quality_prompt_defaults(&mut profile) {
                validate_profile_data(&profile)?;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &review_quality_prompt_marker,
            b"evidence-prioritized-paper-review-prompts\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the review-quality prompt migration '{}': {error}",
                review_quality_prompt_marker.display()
            )
        })?;
    }

    // Shared context reuse is now part of Paper Review (Full). Built-ins are
    // customizable, so only upgrade an exact semantic match for either prior
    // stock variant (validation enabled on first-run creation, disabled after
    // a reset). A stock profile whose user explicitly left caching disabled is
    // indistinguishable from the old default; any other customization or
    // already-enabled profile is preserved.
    let context_cache_marker = profiles.join(".builtin-catalog-v7");
    if !context_cache_marker.exists() {
        let path = profiles.join("deep-review.json");
        if path.exists() {
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            validate_profile_data(&profile)?;
            if matches_prior_stock_full_review(&profile)? {
                profile.context_cache.enabled = true;
                let json = serde_json::to_string_pretty(&profile).map_err(|error| {
                    format!("Failed to serialize '{}': {error}", path.display())
                })?;
                restore_profile_bytes(&path, json.as_bytes())
                    .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
            }
        }
        fs::write(
            &context_cache_marker,
            b"full-review-shared-context-default\n",
        )
        .map_err(|error| {
            format!(
                "Failed to record the shared-context migration '{}': {error}",
                context_cache_marker.display()
            )
        })?;
    }

    Ok(())
}

/// Every marker `migrate_builtin_catalog` records. A store just created by
/// `create_builtin_profiles` is already in the current catalog format, so
/// fresh installs record all of these up front: the migration bodies exist to
/// upgrade older stores, and at least one (the v6 artifact-context refresh)
/// is not a no-op when replayed against current profiles — it would undo
/// auto_review_profile()'s deliberately narrowed auto_validate context.
/// Keep this list in sync when adding a migration marker.
const CATALOG_MIGRATION_MARKERS: &[&str] = &[
    ".builtin-catalog-v3",
    ".builtin-catalog-v4",
    ".builtin-catalog-v6",
    ".builtin-catalog-v7",
    ".builtin-catalog-v8",
    ".builtin-catalog-v9",
    ".builtin-catalog-v10",
    ".builtin-catalog-v15",
    ".structured-auto-review-contract-v2",
    ".modular-auto-review-contract-v1",
    ".compact-auto-review-prompt-v1",
    ".prompt-parsimony-v1",
    ".structured-grant-review-contract-v1",
];

/// Replace a legacy built-in Grant Review suite with its typed, validated
/// successor. The archived copy keeps any user edits recoverable.
pub(super) fn refresh_stale_grant_review_suite(profiles: &Path) -> Result<(), String> {
    let id = "grant-review";
    let path = profiles.join(format!("{id}.json"));
    if !path.exists() {
        return Ok(());
    }
    let content = read_profile_file(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let profile: ProfileData = serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
    let has_legacy_skeleton = [
        "grant_significance",
        "grant_approach",
        "grant_investigators",
        "grant_clarity",
        "grant_synthesis",
    ]
    .iter()
    .all(|step_id| profile.steps.iter().any(|step| step.id == *step_id));
    let is_current = profile.steps.iter().any(|step| step.id == "grant_validate")
        && profile
            .steps
            .iter()
            .find(|step| step.id == "grant_validate")
            .and_then(|step| step.output_schema.as_ref())
            .and_then(|schema| schema.get("x-pipeline-schema-ref"))
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
pub(super) fn refresh_stale_auto_review_suites(profiles: &Path) -> Result<(), String> {
    for (id, replacement) in [
        ("auto-review", auto_review_profile()),
        ("auto-review-quick", quick_auto_review_profile()),
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

pub(super) fn refresh_builtin_auto_review_contracts(profiles: &Path) -> Result<(), String> {
    for (id, replacement) in [
        ("auto-review", auto_review_profile()),
        ("auto-review-quick", quick_auto_review_profile()),
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
        validate_profile_data(&profile)?;
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
        restore_profile_bytes(&path, json.as_bytes())
            .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
    }
    Ok(())
}

pub(super) fn compact_builtin_auto_review_prompts(profiles: &Path) -> Result<(), String> {
    let template = crate::auto_review::orientation_prompt();
    let expanded = crate::auto_review::expand_orientation_prompt(&template);
    for id in ["auto-review", "auto-review-quick"] {
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
        validate_profile_data(&profile)?;
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
pub(super) fn refresh_parsimonious_prompt_defaults(profiles: &Path) -> Result<(), String> {
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
        ("auto-review", auto_review_profile()),
        ("auto-review-quick", quick_auto_review_profile()),
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
        if changed {
            validate_profile_data(&profile)?;
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
    }

    let path = profiles.join("grant-review.json");
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
        if changed {
            validate_profile_data(&profile)?;
            let json = serde_json::to_string_pretty(&profile)
                .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
            restore_profile_bytes(&path, json.as_bytes())
                .map_err(|error| format!("Failed to update '{}': {error}", path.display()))?;
        }
    }

    Ok(())
}

pub(super) fn record_fresh_install_markers(profiles: &Path) -> Result<(), String> {
    for marker in CATALOG_MIGRATION_MARKERS {
        let path = profiles.join(marker);
        if !path.exists() {
            fs::write(&path, b"fresh-install\n").map_err(|error| {
                format!(
                    "Failed to record migration marker '{}': {error}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Overwrite the prompt-bearing fields that `ProfileData::new` and
/// `MergeConfig::default` fill via `prompts::load_prompt` — which honors
/// ~/.pipeline/prompts/ user overrides — with the compiled-in defaults.
/// Untouched-stock fingerprints must never depend on the user's current
/// prompt overrides: a merge.md or parallel_context.md override (a documented
/// feature) would otherwise silently prevent an untouched stock profile from
/// being recognized and upgraded. Real profile creation keeps honoring
/// overrides; only migration-time fingerprint construction is pinned.
pub(super) fn pin_fingerprint_prompt_defaults(profile: &mut ProfileData) {
    if let Some(template) = prompts::compiled_default("parallel_context") {
        profile.parallel_context_template = template.to_string();
    }
    if let Some(merge) = prompts::compiled_default("merge") {
        profile.merge.prompt = merge.to_string();
    }
}

pub(super) fn matches_prior_stock_full_review(profile: &ProfileData) -> Result<bool, String> {
    if profile.context_cache.enabled {
        return Ok(false);
    }
    let actual = serde_json::to_value(profile)
        .map_err(|error| format!("Failed to fingerprint Paper Review (Full): {error}"))?;
    for validate_enabled in [false, true] {
        let mut prior_stock = full_review_profile(validate_enabled);
        pin_fingerprint_prompt_defaults(&mut prior_stock);
        // full_review_profile's step prompts also come from load_prompt (via
        // prompt_step); each step id is exactly its shipped prompt name, so
        // pin those to the compiled defaults as well.
        for step in &mut prior_stock.steps {
            if let Some(default) = prompts::compiled_default(&step.id) {
                step.prompt = default.to_string();
            }
        }
        prior_stock.context_cache.enabled = false;
        let expected = serde_json::to_value(prior_stock)
            .map_err(|error| format!("Failed to fingerprint stock Full profile: {error}"))?;
        if actual == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn prompt_digest(prompt: &str) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(prompt.as_bytes()))
}

pub(super) fn migrate_shipped_prompt_defaults(profile: &mut ProfileData) -> bool {
    const OLD_EDITOR: &str = "2393f4d8f6f39641b5a5e609e132513abb3850b4995eb0522a3ae077fdef29b0";
    const OLD_EDITOR_ISSUES: &str =
        "01079fa2f1a5c0c44ba88861966b8383bbb5a4ac3b47b0e95ac3a8256bb36eeb";
    const OLD_MERGE: &str = "cd90280a68b6818ba075f293f3ec54629568dd2d3e13dc891bcfb3164cdb9b18";
    const ACADEMIC_MERGE: &str = "320a872359321270bd37632bd8c08720d558bdd81c0f8da29ec9fb5f68a09aea";
    let mut changed = false;
    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_EDITOR => prompts::compiled_default("editor_synthesis"),
            OLD_EDITOR_ISSUES => prompts::compiled_default("editor_synthesis_issues"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    if matches!(
        prompt_digest(&profile.merge.prompt).as_str(),
        OLD_MERGE | ACADEMIC_MERGE
    ) {
        if let Some(replacement) = prompts::compiled_default("merge") {
            profile.merge.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}

pub(super) fn migrate_efficient_retrieval_defaults(profile: &mut ProfileData) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "2d3f253aba5a39e76a560cda448296647b42cd3b4d487bbb761af9e7172fbe65";
    const OLD_GENERIC_PARALLEL_CONTEXT: &str =
        "d2bc29faeb032cc35cb60f2aff561f17a737c18348a76140205777f8b3c33197";
    const OLD_VALIDATE_FEEDBACK: &str =
        "ad7af438fed68f08e4f2e85de9155314f9579d6d96a4c960fb4b53b5ff0e7555";

    let mut changed = false;
    let parallel_replacement = match prompt_digest(&profile.parallel_context_template).as_str() {
        OLD_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context"),
        OLD_GENERIC_PARALLEL_CONTEXT => prompts::compiled_default("parallel_context_generic"),
        _ => None,
    };
    if let Some(replacement) = parallel_replacement {
        profile.parallel_context_template = replacement.to_string();
        changed = true;
    }

    for step in &mut profile.steps {
        if prompt_digest(&step.prompt) == OLD_VALIDATE_FEEDBACK {
            if let Some(replacement) = prompts::compiled_default("validate_feedback") {
                step.prompt = replacement.to_string();
                changed = true;
            }
        }
    }
    changed
}

pub(super) fn migrate_review_quality_prompt_defaults(profile: &mut ProfileData) -> bool {
    const OLD_PARALLEL_CONTEXT: &str =
        "b5343b777ec44f21af434dbea2daff76e6ef352ac58cf2a928b16e204b04b709";
    const OLD_CONTRIBUTION: &str =
        "733b35d1e2137ea876ca4ef504532ae2d5875291f943584ce1eb4e42ad937e03";
    const OLD_TECHNICAL: &str = "e7574cc654ce76e21ea254d517e7cdd1e9991fd691249226b6c6a5745664e03e";
    const OLD_EMPIRICAL: &str = "b8823245b925a15775cde437ed7ad31fe9fc3766d61c9e2f56677a9b14b2998e";
    const OLD_EXPOSITION: &str = "15f0b9340bd28b4d4e0f3fc5d1b59cd01845c62854eb856eb24e9b052edd5914";
    const OLD_EDITOR_SYNTHESIS: &str =
        "5fd0eed34fddbc6ab32d9330b097b72895e58b91be9c013f42bf5636495ed5d7";
    const OLD_VALIDATE_FEEDBACK: &str =
        "fa49fa759b553c171ed721d2cc8696b1f083a891e237be01ace245188761a744";

    let mut changed = false;
    if prompt_digest(&profile.parallel_context_template) == OLD_PARALLEL_CONTEXT {
        if let Some(replacement) = prompts::compiled_default("parallel_context") {
            profile.parallel_context_template = replacement.to_string();
            changed = true;
        }
    }

    for step in &mut profile.steps {
        let replacement = match prompt_digest(&step.prompt).as_str() {
            OLD_CONTRIBUTION => prompts::compiled_default("contribution"),
            OLD_TECHNICAL => prompts::compiled_default("technical"),
            OLD_EMPIRICAL => prompts::compiled_default("empirical"),
            OLD_EXPOSITION => prompts::compiled_default("exposition"),
            OLD_EDITOR_SYNTHESIS => prompts::compiled_default("editor_synthesis"),
            OLD_VALIDATE_FEEDBACK => prompts::compiled_default("validate_feedback"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            step.prompt = replacement.to_string();
            changed = true;
        }
    }
    changed
}

/// Hide a retired built-in from the profile list without discarding any user
/// customizations it may contain. Avoid overwriting an archive left by a
/// partially completed or earlier migration.
pub(super) fn archive_retired_profile(profiles: &Path, id: &str) -> Result<(), String> {
    let source = profiles.join(format!("{id}.json"));
    if !source.exists() {
        return Ok(());
    }

    let archive_dir = profiles.join(".retired-builtins");
    fs::create_dir_all(&archive_dir).map_err(|error| {
        format!(
            "Failed to create retired-profile archive '{}': {error}",
            archive_dir.display()
        )
    })?;
    let destination = (1..=1000)
        .map(|copy| {
            let suffix = if copy == 1 {
                String::new()
            } else {
                format!("-{copy}")
            };
            archive_dir.join(format!("{id}{suffix}.json"))
        })
        .find(|candidate| !candidate.exists())
        .ok_or_else(|| format!("Too many archived copies of profile '{id}'"))?;

    fs::rename(&source, &destination).map_err(|error| {
        format!(
            "Failed to archive retired profile '{}' as '{}': {error}",
            source.display(),
            destination.display()
        )
    })
}

// ── Migration ───────────────────────────────────────────────────────

/// Migrate from old single-file formats to profiles directory. Idempotent.
pub(super) fn ensure_migrated() -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let old_path = home.join(".pipeline").join("pipeline.json");
    let profiles = profiles_dir()?;
    // Every installation initialized under the profiles-directory scheme
    // created deep-review.json unconditionally; the v15 retirement archives
    // it and records its marker on first startup (including fresh installs),
    // so either artifact identifies an already-initialized profile store.
    let initialized = profiles.join("deep-review.json").exists()
        || profiles.join(".builtin-catalog-v15").exists();

    if !initialized {
        // Migrate from pipeline.json → save as a "Migrated" profile
        if old_path.exists() {
            if let Ok(content) = read_profile_file(&old_path) {
                // Try legacy format with referees/post_steps
                if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
                    let profile = ProfileData::new(
                        "Migrated",
                        convert_legacy_steps(legacy.referees, legacy.post_steps),
                        legacy.merge,
                    );
                    save_profile_unlocked("migrated", &profile)?;
                    // Delete the source only after the durable destination is
                    // published. Unrecognized legacy JSON is left untouched
                    // for manual recovery.
                    let _ = fs::remove_file(&old_path);
                }
            }
        }

        // Migrate from old referees.json
        let old_referees = home.join(".pipeline").join("referees.json");
        if old_referees.exists() {
            if let Ok(content) = read_profile_file(&old_referees) {
                if let Ok(referees) = serde_json::from_str::<Vec<LegacyRefereeConfig>>(&content) {
                    let profile = ProfileData::new(
                        "Migrated",
                        convert_legacy_steps(referees, vec![]),
                        MergeConfig::default(),
                    );
                    let migrated_path = profiles.join("migrated.json");
                    if !migrated_path.exists() {
                        save_profile_unlocked("migrated", &profile)?;
                        let _ = fs::remove_file(&old_referees);
                    }
                }
            }
        }

        // Also remove old default.json if present (from prior version)
        let old_default = profiles.join("default.json");
        if old_default.exists() {
            let _ = fs::remove_file(&old_default);
        }
    } else if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }

    // Always ensure current built-in profiles exist and apply catalog changes
    // to installations created by earlier releases.
    create_builtin_profiles()?;
    if !initialized {
        // The profiles were just written in the current format; record every
        // catalog migration as already applied so their upgrade bodies can
        // never rewrite them (the v6 refresh in particular is destructive
        // when replayed against a current stock Auto profile).
        record_fresh_install_markers(&profiles)?;
    }
    migrate_builtin_catalog(&profiles)?;

    // Self-heal settings that still point at a retired built-in whose file no
    // longer exists (a restored pre-retirement settings backup, or a failed
    // settings write during the one-time swap above). Entries are ordered so
    // chains resolve in one pass (… → deep-review → auto-review). A custom
    // profile that reuses a retired ID has a file and is left alone.
    for (id, replacement) in RETIRED_BUILTIN_PROFILES
        .iter()
        .chain(V9_RETIRED_BUILTIN_PROFILES)
        .chain(V15_RETIRED_BUILTIN_PROFILES)
    {
        if !profiles.join(format!("{id}.json")).exists() {
            let _ = crate::settings::replace_active_profile_if(id, replacement);
        }
    }

    Ok(())
}

// ── Profile I/O ─────────────────────────────────────────────────────
