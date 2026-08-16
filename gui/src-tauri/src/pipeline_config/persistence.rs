use super::*;

pub(crate) fn load_profile(id: &str) -> Result<ProfileData, String> {
    let path = profile_path(id)?;
    let content =
        read_profile_file(&path).map_err(|e| format!("Failed to read profile '{id}': {e}"))?;

    // Try the current format. `steps` may legitimately be empty; serde's
    // required current-format fields distinguish it from the legacy shape.
    if let Ok(profile) = serde_json::from_str::<ProfileData>(&content) {
        validate_profile_data(&profile).map_err(|e| format!("Profile '{id}' is invalid: {e}"))?;
        return Ok(profile);
    }

    // Try legacy format (has referees/post_steps arrays)
    if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
        if !legacy.referees.is_empty() || !legacy.post_steps.is_empty() {
            let profile = ProfileData::new(
                legacy.name,
                convert_legacy_steps(legacy.referees, legacy.post_steps),
                legacy.merge,
            );
            // Keep loading legacy profile files without mutating them during
            // a read. Explicit saves/imports publish the current format under
            // the cross-process profile lock.
            return Ok(profile);
        }
    }

    Err(format!("Failed to parse profile '{id}'"))
}

pub(super) fn save_profile_unlocked(id: &str, profile: &ProfileData) -> Result<(), String> {
    let path = profile_path(id)?;
    validate_profile_data(profile)?;
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Failed to serialize: {e}"))?;
    restore_profile_bytes(&path, json.as_bytes())
        .map_err(|e| format!("Failed to save profile '{}': {e}", path.display()))
}

pub fn save_profile(id: &str, profile: &ProfileData) -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    validate_enabled_sequential_step(&profile.steps)?;
    save_profile_unlocked(id, profile)
}

// ── Public API ──────────────────────────────────────────────────────

/// Load the active profile as a PipelineConfig.
pub fn load() -> PipelineConfig {
    let _ = ensure_migrated();
    let settings = crate::settings::load_persisted();
    match load_profile(&settings.active_profile) {
        Ok(profile) => profile.into(),
        Err(e) => {
            // Always warn when a profile fails to load so the user knows
            // they're running on defaults rather than their saved config.
            let file_exists = profile_path(&settings.active_profile)
                .map(|p| p.exists())
                .unwrap_or(false);
            if file_exists {
                eprintln!(
                    "WARNING: Profile '{}' exists but failed to load: {e}. \
                     The file may be corrupt. Using default pipeline.",
                    settings.active_profile
                );
            } else {
                eprintln!(
                    "WARNING: Profile '{}' not found: {e}. Using default pipeline.",
                    settings.active_profile
                );
            }
            defaults()
        }
    }
}

/// Load the executable config and its display name from one profile-file
/// snapshot. Run manifests use the returned name so a concurrent profile edit
/// cannot make their metadata disagree with the workflow that actually ran.
pub fn load_required_profile_for(active_profile: &str) -> Result<(PipelineConfig, String), String> {
    let workflow = load_required_workflow_for(active_profile)?;
    Ok((workflow.config, workflow.name))
}

/// Load an installed profile as the same normalized portable document used by
/// ephemeral CLI runs. This keeps fingerprints and retained provenance
/// identical across installed and file-backed workflows.
pub fn load_required_workflow_for(active_profile: &str) -> Result<WorkflowDocument, String> {
    ensure_migrated()?;
    let profile = load_profile(active_profile).map_err(|error| {
        format!(
            "Active profile '{}' could not be loaded: {error}. Select or repair a profile before running.",
            active_profile
        )
    })?;
    WorkflowDocument::from_profile_data(profile)
}
