use super::*;

const MAX_PROFILE_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct ProfileMutationGuard {
    _process: std::sync::MutexGuard<'static, ()>,
    file: fs::File,
}

impl Drop for ProfileMutationGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub(super) fn lock_profile_mutations() -> Result<ProfileMutationGuard, String> {
    static PROCESS_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let process = PROCESS_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profiles = profiles_dir()?;
    let root = profiles
        .parent()
        .ok_or("Profiles directory has no parent")?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("profiles.lock"))
        .map_err(|error| format!("Failed to open profile lock: {error}"))?;
    fs2::FileExt::lock_exclusive(&file)
        .map_err(|error| format!("Failed to lock profiles: {error}"))?;
    Ok(ProfileMutationGuard {
        _process: process,
        file,
    })
}

pub(super) fn read_profile_file(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)
        .map_err(|e| format!("Failed to read profile '{}': {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read profile '{}': {e}", path.display()))?;
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(format!(
            "Profile '{}' exceeds the {} MB safety limit",
            path.display(),
            MAX_PROFILE_BYTES / 1024 / 1024
        ));
    }
    String::from_utf8(bytes)
        .map_err(|e| format!("Profile '{}' is not valid UTF-8: {e}", path.display()))
}

/// A profile file's contents and whether they are in the legacy format,
/// without the save-time validator.
fn parse_profile(content: &str) -> Option<Result<(ProfileData, bool), String>> {
    // Try the current format. `steps` may legitimately be empty; serde's
    // required current-format fields distinguish it from the legacy shape.
    if let Ok(mut profile) = serde_json::from_str::<ProfileData>(content) {
        let raw: serde_json::Value = serde_json::from_str(content).ok()?;
        let steps = std::mem::take(&mut profile.steps);
        return Some(
            wire_implicit_context(&raw["steps"], steps, &profile.extraction.input_mode).map(
                |steps| {
                    profile.steps = steps;
                    (profile, false)
                },
            ),
        );
    }

    // Try legacy format (has referees/post_steps arrays)
    let legacy = serde_json::from_str::<LegacyProfileData>(content).ok()?;
    if legacy.referees.is_empty() && legacy.post_steps.is_empty() {
        return None;
    }
    // Keep loading legacy profile files without mutating them during a read.
    // Explicit saves/imports publish the current format under the
    // cross-process profile lock.
    let profile = ProfileData::new(
        legacy.name,
        convert_legacy_steps(legacy.referees, legacy.post_steps),
        legacy.merge,
    );
    Some(Ok((profile, true)))
}

pub(super) fn read_profile_at(path: &Path, id: &str) -> Result<(ProfileData, bool), String> {
    let content =
        read_profile_file(path).map_err(|e| format!("Failed to read profile '{id}': {e}"))?;
    match parse_profile(&content) {
        Some(Ok(parsed)) => Ok(parsed),
        Some(Err(error)) => Err(format!("Profile '{id}' is invalid: {error}")),
        None => Err(format!("Failed to parse profile '{id}'")),
    }
}

pub(super) fn load_profile_at(path: &Path, id: &str) -> Result<ProfileData, String> {
    let (profile, legacy) = read_profile_at(path, id)?;
    if !legacy {
        validate_profile_data(&profile).map_err(|e| format!("Profile '{id}' is invalid: {e}"))?;
    }
    Ok(profile)
}

pub(crate) fn load_profile(id: &str) -> Result<ProfileData, String> {
    validate_profile_id(id)?;
    let id = resolve_renamed_profile_id(&profiles_dir()?, id);
    load_profile_at(&profile_path(id)?, id)
}

/// A saved profile whether or not it passes validation, for showing, exporting,
/// and repairing it. Running one always goes through `load_profile`.
pub(super) fn read_profile(id: &str) -> Result<ProfileData, String> {
    validate_profile_id(id)?;
    let id = resolve_renamed_profile_id(&profiles_dir()?, id);
    read_profile_at(&profile_path(id)?, id).map(|(profile, _)| profile)
}

pub(super) fn save_profile_unlocked(id: &str, profile: &ProfileData) -> Result<(), String> {
    save_profile_unlocked_at(&profile_path(id)?, profile)
}

pub(super) fn save_profile_unlocked_at(path: &Path, profile: &ProfileData) -> Result<(), String> {
    validate_profile_data(profile)?;
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Failed to serialize: {e}"))?;
    restore_profile_bytes(path, json.as_bytes())
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
            // The editor shows a profile that fails validation as it was
            // saved, so that it can be repaired. Runs refuse it.
            if let Ok(profile) = read_profile(&settings.active_profile) {
                eprintln!(
                    "WARNING: Profile '{}' needs repair: {e}",
                    settings.active_profile
                );
                return profile.into();
            }
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
