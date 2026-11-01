//! Rename and archival migrations that preserve existing profile contents.

use super::super::{
    read_profile_file, restore_profile_bytes, ProfileData, RENAMED_BUILTIN_PROFILES,
};
use std::fs;
use std::path::Path;

pub(super) const RENAMED_BUILTIN_IDS_MARKER: &str = ".paper-review-ids-v1";

/// Move renamed built-ins to their current IDs. This runs before
/// `create_builtin_profiles`, which would otherwise write fresh stock copies
/// under the new IDs, and before the catalog migrations, which address the
/// built-ins by their current IDs. The user's file moves with its edits; only
/// an untouched stock display name changes. A destination that already exists
/// (a custom profile, or a move interrupted before the source was removed) is
/// kept, and the source is archived recoverably instead.
pub(in crate::pipeline_config) fn migrate_renamed_builtin_ids(
    profiles: &Path,
) -> Result<(), String> {
    let marker = profiles.join(RENAMED_BUILTIN_IDS_MARKER);
    if marker.exists() {
        return Ok(());
    }
    for renamed in RENAMED_BUILTIN_PROFILES {
        let source = profiles.join(format!("{}.json", renamed.old_id));
        if !source.exists() {
            continue;
        }
        let destination = profiles.join(format!("{}.json", renamed.id));
        if destination.exists() {
            archive_retired_profile(profiles, renamed.old_id)?;
        } else {
            let content = read_profile_file(&source)
                .map_err(|error| format!("Failed to read '{}': {error}", source.display()))?;
            let content = match serde_json::from_str::<ProfileData>(&content) {
                Ok(mut profile) if profile.name == renamed.old_name => {
                    profile.name = renamed.name.to_string();
                    serde_json::to_string_pretty(&profile).map_err(|error| {
                        format!("Failed to serialize '{}': {error}", source.display())
                    })?
                }
                _ => content,
            };
            restore_profile_bytes(&destination, content.as_bytes())
                .map_err(|error| format!("Failed to write '{}': {error}", destination.display()))?;
            fs::remove_file(&source)
                .map_err(|error| format!("Failed to remove '{}': {error}", source.display()))?;
        }
        // Do not leave Settings pointing at the former ID.
        let _ = crate::settings::replace_active_profile_if(renamed.old_id, renamed.id);
    }
    fs::write(&marker, b"paper-review-profile-ids\n").map_err(|error| {
        format!(
            "Failed to record the built-in profile rename '{}': {error}",
            marker.display()
        )
    })
}

/// Hide a retired built-in from the profile list without discarding any user
/// customizations it may contain. Avoid overwriting an archive left by a
/// partially completed or earlier migration.
pub(in crate::pipeline_config) fn archive_retired_profile(
    profiles: &Path,
    id: &str,
) -> Result<(), String> {
    archive_profile_file(profiles, &profiles.join(format!("{id}.json")), id)
}

/// Move `source` into the archive as `<id>.json`. It leaves the places Pipeline
/// reads from, and every byte of it stays recoverable.
pub(in crate::pipeline_config) fn archive_profile_file(
    profiles: &Path,
    source: &Path,
    id: &str,
) -> Result<(), String> {
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

    fs::rename(source, &destination).map_err(|error| {
        format!(
            "Failed to archive retired profile '{}' as '{}': {error}",
            source.display(),
            destination.display()
        )
    })
}
