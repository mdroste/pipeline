//! Ordered catalog upgrade dispatch and exact migration marker records.

use super::super::{
    builtin_primary_readers, configure_artifact_flow, read_profile_file, restore_profile_bytes,
    ProfileData, MIGRATED_BUILTIN_PROFILES, RETIRED_BUILTIN_PROFILES, V15_RETIRED_BUILTIN_PROFILES,
    V9_RETIRED_BUILTIN_PROFILES,
};
use super::prompts::{
    matches_prior_stock_full_review, migrate_efficient_retrieval_defaults,
    migrate_review_quality_prompt_defaults, migrate_shipped_prompt_defaults,
};
use super::retirement::{archive_retired_profile, RENAMED_BUILTIN_IDS_MARKER};
use super::suites::{
    compact_builtin_auto_review_prompts, refresh_builtin_auto_review_contracts,
    refresh_parsimonious_prompt_defaults, refresh_stale_auto_review_suites,
    refresh_stale_grant_review_suite,
};
use super::validated_for_migration;
use std::fs;
use std::path::Path;

pub(in crate::pipeline_config) fn migrate_builtin_catalog(profiles: &Path) -> Result<(), String> {
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
    // must still land on Paper Review rather than a nonexistent profile.
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
    let structured_grant_review_marker = profiles.join(".structured-grant-review-contract-v2");
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
        for id in MIGRATED_BUILTIN_PROFILES {
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
            if !validated_for_migration(&path, &profile, "artifact-flow") {
                continue;
            }
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
        for id in MIGRATED_BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_shipped_prompt_defaults(&mut profile) {
                if !validated_for_migration(&path, &profile, "shipped-prompt") {
                    continue;
                }
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
        for id in MIGRATED_BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_efficient_retrieval_defaults(&mut profile) {
                if !validated_for_migration(&path, &profile, "efficient-retrieval") {
                    continue;
                }
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
        for id in MIGRATED_BUILTIN_PROFILES {
            let path = profiles.join(format!("{id}.json"));
            if !path.exists() {
                continue;
            }
            let content = read_profile_file(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let mut profile: ProfileData = serde_json::from_str(&content)
                .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))?;
            if migrate_review_quality_prompt_defaults(&mut profile) {
                if !validated_for_migration(&path, &profile, "review-quality-prompt") {
                    continue;
                }
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
            if validated_for_migration(&path, &profile, "context-cache")
                && matches_prior_stock_full_review(&profile)?
            {
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
pub(in crate::pipeline_config) const CATALOG_MIGRATION_MARKERS: &[&str] = &[
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
    ".structured-grant-review-contract-v2",
    RENAMED_BUILTIN_IDS_MARKER,
];

pub(in crate::pipeline_config) fn record_fresh_install_markers(
    profiles: &Path,
) -> Result<(), String> {
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
