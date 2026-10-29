//! Profile-store initialization and the ordered migration entry point.

use super::{
    convert_legacy_steps, create_builtin_profiles, lock_profile_mutations, profiles_dir,
    read_profile_file, save_profile_unlocked, validate_profile_data, LegacyProfileData,
    LegacyRefereeConfig, MergeConfig, ProfileData, RENAMED_BUILTIN_PROFILES,
    RETIRED_BUILTIN_PROFILES, V15_RETIRED_BUILTIN_PROFILES, V9_RETIRED_BUILTIN_PROFILES,
};
use std::fs;
use std::path::Path;

mod catalog;
mod prompts;
mod retirement;
mod suites;

pub(super) use catalog::{migrate_builtin_catalog, record_fresh_install_markers};
pub(super) use retirement::migrate_renamed_builtin_ids;

#[cfg(test)]
pub(super) use catalog::CATALOG_MIGRATION_MARKERS;
#[cfg(test)]
pub(super) use prompts::{migrate_review_quality_prompt_defaults, prompt_digest};
#[cfg(test)]
pub(super) use retirement::archive_retired_profile;
#[cfg(test)]
pub(super) use suites::{
    compact_builtin_auto_review_prompts, refresh_builtin_auto_review_contracts,
    refresh_parsimonious_prompt_defaults, refresh_stale_auto_review_suites,
};

/// Apply catalog changes to profiles created by older Pipeline releases.
/// A built-in that fails current validation is left alone (and logged), as
/// load_profile already skips it; failing the whole catalog here would block
/// every launch and every run until the user repairs the file by hand.
fn validated_for_migration(path: &Path, profile: &ProfileData, migration: &str) -> bool {
    match validate_profile_data(profile) {
        Ok(()) => true,
        Err(error) => {
            eprintln!(
                "WARNING: left '{}' unchanged by the {migration} migration: {error}",
                path.display()
            );
            false
        }
    }
}

/// Migrate from old single-file formats to profiles directory. Idempotent.
pub(super) fn ensure_migrated() -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    let root = crate::storage::data_root()?;
    let old_path = root.join("pipeline.json");
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
        let old_referees = root.join("referees.json");
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
    // to installations created by earlier releases. Renamed built-ins move
    // first so their stock copies are not created beside the user's file.
    migrate_renamed_builtin_ids(&profiles)?;
    create_builtin_profiles()?;
    if !initialized {
        // The profiles were just written in the current format; record every
        // catalog migration as already applied so their upgrade bodies can
        // never rewrite them (the v6 refresh in particular is destructive
        // when replayed against a current stock Auto profile).
        record_fresh_install_markers(&profiles)?;
    }
    migrate_builtin_catalog(&profiles)?;

    // Self-heal settings that still point at a retired or renamed built-in
    // whose file no longer exists (a restored older settings backup, or a
    // failed settings write during a one-time swap). Entries are ordered so
    // chains resolve in one pass (… → deep-review → auto-paper-review). A
    // custom profile that reuses a former ID has a file and is left alone.
    let renamed = RENAMED_BUILTIN_PROFILES
        .iter()
        .map(|renamed| (renamed.old_id, renamed.id));
    for (id, replacement) in RETIRED_BUILTIN_PROFILES
        .iter()
        .chain(V9_RETIRED_BUILTIN_PROFILES)
        .chain(V15_RETIRED_BUILTIN_PROFILES)
        .copied()
        .chain(renamed)
    {
        if !profiles.join(format!("{id}.json")).exists() {
            let _ = crate::settings::replace_active_profile_if(id, replacement);
        }
    }

    Ok(())
}
