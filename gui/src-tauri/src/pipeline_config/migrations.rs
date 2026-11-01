//! Profile-store initialization and the ordered migration entry point.

use super::{
    convert_legacy_steps, create_builtin_profiles, lock_profile_mutations, profiles_dir,
    read_profile_file, save_profile_unlocked_at, validate_profile_data, LegacyProfileData,
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
use retirement::archive_profile_file;
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

/// Move a file this migration can't carry over out of the way. A file that can't
/// be moved stays where it is, so a launch never depends on this.
fn archive_legacy_file(profiles: &Path, source: &Path, id: &str) {
    if let Err(error) = archive_profile_file(profiles, source, id) {
        eprintln!("WARNING: left '{}' where it is: {error}", source.display());
    }
}

/// Whether `path` now holds exactly `profile`, read back from disk.
fn holds_profile(path: &Path, profile: &ProfileData) -> bool {
    let saved = read_profile_file(path)
        .ok()
        .and_then(|content| serde_json::from_str::<ProfileData>(&content).ok());
    saved.is_some_and(|saved| {
        let saved = serde_json::to_value(saved).ok();
        saved.is_some() && saved == serde_json::to_value(profile).ok()
    })
}

/// Save what an old file held as the migrated profile, and delete the old file
/// only once that copy reads back the same. Without such a copy, whether the
/// file made no sense, failed validation, or a migrated profile already exists,
/// it goes to the archive and startup carries on.
fn carry_over(
    profiles: &Path,
    source: &Path,
    id: &str,
    profile: Option<ProfileData>,
) -> Result<(), String> {
    let destination = profiles.join("migrated.json");
    if let Some(profile) = profile.filter(|_| !destination.exists()) {
        match validate_profile_data(&profile) {
            Ok(()) => {
                save_profile_unlocked_at(&destination, &profile)?;
                if holds_profile(&destination, &profile) {
                    let _ = fs::remove_file(source);
                    return Ok(());
                }
            }
            Err(error) => {
                eprintln!("WARNING: could not migrate '{}': {error}", source.display());
            }
        }
    }
    archive_legacy_file(profiles, source, id);
    Ok(())
}

/// Carry the single-file configurations of the earliest releases into the
/// profiles directory.
pub(super) fn migrate_legacy_files(
    root: &Path,
    profiles: &Path,
    initialized: bool,
) -> Result<(), String> {
    let old_path = root.join("pipeline.json");
    if initialized {
        // Nothing reads this file now, and nothing shows it was carried over.
        archive_legacy_file(profiles, &old_path, "pipeline");
        return Ok(());
    }

    // Migrate from pipeline.json → save as a "Migrated" profile
    if old_path.exists() {
        let legacy = read_profile_file(&old_path)
            .ok()
            .and_then(|content| serde_json::from_str::<LegacyProfileData>(&content).ok())
            // Unknown fields are ignored, so any JSON object reads as an empty
            // configuration, and a copy of that would keep nothing.
            .filter(|legacy| !legacy.referees.is_empty() || !legacy.post_steps.is_empty());
        let profile = legacy.map(|legacy| {
            ProfileData::new(
                "Migrated",
                convert_legacy_steps(legacy.referees, legacy.post_steps),
                legacy.merge,
            )
        });
        carry_over(profiles, &old_path, "pipeline", profile)?;
    }

    // Migrate from old referees.json
    let old_referees = root.join("referees.json");
    if old_referees.exists() {
        let referees = read_profile_file(&old_referees)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<LegacyRefereeConfig>>(&content).ok())
            .filter(|referees| !referees.is_empty());
        let profile = referees.map(|referees| {
            ProfileData::new(
                "Migrated",
                convert_legacy_steps(referees, vec![]),
                MergeConfig::default(),
            )
        });
        carry_over(profiles, &old_referees, "referees", profile)?;
    }

    // default.json from a prior version is no longer a profile of its own.
    let old_default = profiles.join("default.json");
    archive_legacy_file(profiles, &old_default, "default");
    Ok(())
}

/// Migrate from old single-file formats to profiles directory. Idempotent.
pub(super) fn ensure_migrated() -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    let root = crate::storage::data_root()?;
    let profiles = profiles_dir()?;
    // Every installation initialized under the profiles-directory scheme
    // created deep-review.json unconditionally; the v15 retirement archives
    // it and records its marker on first startup (including fresh installs),
    // so either artifact identifies an already-initialized profile store.
    let initialized = profiles.join("deep-review.json").exists()
        || profiles.join(".builtin-catalog-v15").exists();

    migrate_legacy_files(&root, &profiles, initialized)?;

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
