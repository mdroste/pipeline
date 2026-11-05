use super::persistence::{load_profile_at, read_profile_at};
use super::settings_restore::restored_settings;
use super::*;

/// Save to the active profile.
pub fn save(config: &PipelineConfig) -> Result<(), String> {
    let _ = ensure_migrated();
    let settings = crate::settings::load_persisted_required()?;
    save_for(&settings.active_profile, config)
}

pub fn save_for(profile_id: &str, config: &PipelineConfig) -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    validate_profile_id(profile_id)?;
    let profile_id = resolve_renamed_profile_id(&profiles_dir()?, profile_id);
    validate_profile_steps(&config.steps)?;
    validate_enabled_sequential_step(&config.steps)?;
    // Saving is how a profile that fails validation gets repaired.
    let name = read_profile(profile_id)?.name;
    let profile = ProfileData::from_config(name, config);
    save_profile_unlocked(profile_id, &profile)
}

/// Reset active profile to defaults: the matching stock definition for a
/// built-in, otherwise the default Paper Review pipeline.
/// The stock definition is written without re-reading the existing file, so
/// a profile that no longer passes validation can still be reset.
pub fn reset_defaults() -> Result<PipelineConfig, String> {
    let _ = ensure_migrated();
    let id = get_active_profile_id();
    let _lock = lock_profile_mutations()?;
    reset_defaults_at(&profiles_dir()?, &id)
}

pub(super) fn reset_defaults_at(profiles: &Path, id: &str) -> Result<PipelineConfig, String> {
    validate_profile_id(id)?;
    let path = profiles.join(format!("{id}.json"));
    let stock: ProfileData = match id {
        PAPER_REVIEW_ID => auto_review_profile(),
        PAPER_REVIEW_FAST_ID => quick_auto_review_profile(),
        SLIDE_REVIEW_ID => slide_review_profile(),
        GRANT_REVIEW_ID => grant_review_profile(),
        DECLAUDE_ID => declaude_profile(),
        _ => {
            // Keep the custom profile's display name, read leniently.
            let name = read_profile_file(&path)
                .ok()
                .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                .and_then(|value| value.get("name")?.as_str().map(str::to_string))
                .unwrap_or_else(|| id.to_string());
            ProfileData::from_config(name, &defaults())
        }
    };
    save_profile_unlocked_at(&path, &stock)?;
    Ok(stock.into())
}

// ── Profile management ──────────────────────────────────────────────

pub fn get_active_profile_id() -> String {
    crate::settings::load_persisted().active_profile
}

pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    ensure_migrated()?;
    list_profiles_at(&profiles_dir()?)
}

/// What can be read of a saved profile that can't be used, with the reason.
fn invalid_profile_summary(id: String, path: &Path, reason: String) -> ProfileSummary {
    let saved = read_profile_file(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok());
    let field = |key: &str| saved.as_ref().and_then(|value| value.get(key).cloned());
    ProfileSummary {
        builtin: BUILTIN_PROFILES.contains(&id.as_str()),
        name: field("name")
            .and_then(|name| name.as_str().map(str::to_string))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| id.clone()),
        step_count: field("steps")
            .and_then(|steps| steps.as_array().map(Vec::len))
            .unwrap_or(0),
        id,
        invalid: Some(reason),
    }
}

pub(super) fn list_profiles_at(dir: &Path) -> Result<Vec<ProfileSummary>, String> {
    let mut summaries = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("Profile listing");
    for entry in fs::read_dir(dir).map_err(|e| format!("Failed to read profiles dir: {e}"))? {
        walk.entry()?;
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        // A file name no profile can have is not a saved profile.
        if let Err(error) = validate_profile_id(&id) {
            eprintln!("WARNING: skipped file '{id}' in profiles: {error}");
            continue;
        }
        match load_profile_at(&path, &id) {
            Ok(profile) => summaries.push(profile_summary(id, &profile)),
            Err(error) => summaries.push(invalid_profile_summary(id, &path, error)),
        }
    }
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(summaries)
}

pub fn create_profile(name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
    let id = slugify(name);
    if id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{id}' already exists"));
    }
    // New profiles start domain-neutral: generic starter steps, generic
    // context template, and an explicit generic survey prompt (empty would
    // fall back to the paper survey at runtime).
    let profile = generic_profile(
        name,
        generic_starter_steps(),
        ExtractionConfig::default(),
        &[],
    );
    save_profile_unlocked(&id, &profile)?;
    Ok(profile_summary(id, &profile))
}

pub fn duplicate_profile(source_id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
    let source = load_profile(source_id)?;
    let new_id = slugify(new_name);
    if new_id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&new_id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{new_id}' already exists"));
    }
    let profile = duplicate_profile_data(source, new_name);
    save_profile_unlocked(&new_id, &profile)?;
    Ok(profile_summary(new_id, &profile))
}

pub(super) fn duplicate_profile_data(mut source: ProfileData, new_name: &str) -> ProfileData {
    source.name = new_name.to_string();
    source
}

pub fn rename_profile(id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
    let mut profile = load_profile(id)?;
    profile.name = new_name.to_string();
    save_profile_unlocked(id, &profile)?;
    Ok(profile_summary(id.to_string(), &profile))
}

pub fn delete_profile(id: &str) -> Result<(), String> {
    let _lock = lock_profile_mutations()?;
    if BUILTIN_PROFILES.contains(&id) {
        return Err(format!("Cannot delete the built-in profile '{id}'"));
    }
    let path = profile_path(id)?;
    if !path.exists() {
        return Err(format!("Profile '{id}' does not exist"));
    }
    delete_profile_file_transactionally(&path, || {
        // If this was the active profile, switch back to Paper Review. The
        // profile file remains recoverable until this settings write commits.
        crate::settings::replace_active_profile_if(id, PAPER_REVIEW_ID).map(|_| ())
    })
}

pub(super) fn delete_profile_file_transactionally(
    path: &Path,
    update_references: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Profile path has no valid file name")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let tombstone = path.with_file_name(format!(
        ".{file_name}.deleting-{}-{nonce}",
        std::process::id()
    ));
    fs::rename(path, &tombstone).map_err(|e| format!("Failed to stage profile deletion: {e}"))?;

    if let Err(error) = update_references() {
        return match fs::rename(&tombstone, path) {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!(
                "{error}. The profile reference update failed and restoring {} also failed: {rollback}. Recover the profile from {}.",
                path.display(),
                tombstone.display()
            )),
        };
    }

    // Reference updates have committed. Failure to unlink the hidden
    // tombstone must not make the UI retry an already-committed deletion; keep
    // it as a recoverable backup and report the cleanup problem to diagnostics.
    if let Err(error) = fs::remove_file(&tombstone) {
        eprintln!(
            "WARNING: profile deletion committed but temporary backup {} could not be removed: {error}",
            tombstone.display()
        );
    }
    Ok(())
}

pub fn switch_profile(id: &str) -> Result<PipelineConfig, String> {
    let _ = ensure_migrated();
    let profile = read_profile(id)?;
    crate::settings::set_active_profile(id)?;
    Ok(profile.into())
}

// ── Export/Import ───────────────────────────────────────────────────

pub fn export_profile_data(id: &str) -> Result<String, String> {
    ensure_migrated()?;
    let profile = read_profile(id)?;
    let envelope = ExportEnvelope::Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: profile.name,
        steps: profile.steps,
        merge: profile.merge,
        outputs: profile.outputs,
        context_cache: profile.context_cache,
        use_orientation: profile.use_orientation,
        orientation_prompt: profile.orientation_prompt,
        orientation_schema: profile.orientation_schema,
        extraction: profile.extraction,
        parallel_context_template: profile.parallel_context_template,
        variables: profile.variables,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

pub fn export_bundle() -> Result<String, String> {
    let _ = ensure_migrated();
    let mut settings = crate::settings::load_persisted_required()?;
    // Strip API keys from the export to prevent credential leakage
    settings.anthropic_api_key = String::new();
    settings.openai_api_key = String::new();
    settings.google_api_key = String::new();
    settings.local_api_key = String::new();
    let envelope = ExportEnvelope::Bundle {
        active_profile: settings.active_profile.clone(),
        settings,
        profiles: bundle_profiles_at(&profiles_dir()?)?,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

/// Every saved profile, including those that need repair. Only a file too
/// damaged to read as a profile is left out, and the listing still shows it.
pub(super) fn bundle_profiles_at(dir: &Path) -> Result<Vec<ProfileExport>, String> {
    let mut profiles = Vec::new();
    for summary in list_profiles_at(dir)? {
        let path = dir.join(format!("{}.json", summary.id));
        match read_profile_at(&path, &summary.id) {
            Ok((profile, _)) => {
                let mut export = ProfileExport::from_profile(summary.id, profile);
                export.invalid = summary.invalid;
                profiles.push(export);
            }
            Err(error) => eprintln!("WARNING: backup left out a profile: {error}"),
        }
    }
    Ok(profiles)
}

/// An export file with its older steps given the context they implied. `raw`
/// is the same file before parsing, which still shows what each step left out.
fn with_older_steps_wired(
    envelope: ExportEnvelope,
    raw: &serde_json::Value,
) -> Result<ExportEnvelope, String> {
    Ok(match envelope {
        ExportEnvelope::Step { data } if raw["data"].get("context").is_none() => {
            ExportEnvelope::Step {
                data: older_step_with_context(data)?,
            }
        }
        ExportEnvelope::Profile {
            schema_version,
            name,
            steps,
            merge,
            outputs,
            context_cache,
            use_orientation,
            orientation_prompt,
            orientation_schema,
            extraction,
            parallel_context_template,
            variables,
        } => ExportEnvelope::Profile {
            schema_version,
            name,
            steps: wire_implicit_context(&raw["steps"], steps, &extraction.input_mode)?,
            merge,
            outputs,
            context_cache,
            use_orientation,
            orientation_prompt,
            orientation_schema,
            extraction,
            parallel_context_template,
            variables,
        },
        ExportEnvelope::Bundle {
            settings,
            mut profiles,
            active_profile,
        } => {
            for (index, profile) in profiles.iter_mut().enumerate() {
                let steps = std::mem::take(&mut profile.steps);
                profile.steps = wire_implicit_context(
                    &raw["profiles"][index]["steps"],
                    steps,
                    &profile.extraction.input_mode,
                )
                .map_err(|error| format!("Profile '{}': {error}", profile.name))?;
            }
            ExportEnvelope::Bundle {
                settings,
                profiles,
                active_profile,
            }
        }
        other => other,
    })
}

/// Auto-detect and parse an export file, handling legacy formats.
pub fn import_envelope(json: &str) -> Result<ExportEnvelope, String> {
    let parsed = serde_json::from_str::<ExportEnvelope>(json);
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
    // Try new format first
    if let Ok(envelope) = parsed {
        return with_older_steps_wired(envelope, &value);
    }

    // Otherwise look for a legacy format

    match value.get("type").and_then(|t| t.as_str()) {
        Some("referee") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyRefereeConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid referee config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: older_step_with_context(referee_to_step(legacy))?,
                });
            }
        }
        Some("post_step") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyPostStepConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid post-step config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: older_step_with_context(post_step_to_step(legacy))?,
                });
            }
        }
        Some("profile") if value.get("referees").is_some() => {
            // Legacy profile with separate referees/post_steps
            let name = value["name"].as_str().unwrap_or("Imported").to_string();
            let referees: Vec<LegacyRefereeConfig> =
                serde_json::from_value(value["referees"].clone()).unwrap_or_default();
            let post_steps: Vec<LegacyPostStepConfig> =
                serde_json::from_value(value["post_steps"].clone()).unwrap_or_default();
            let merge: MergeConfig = value
                .get("merge")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            return Ok(ExportEnvelope::Profile {
                schema_version: 1,
                name,
                steps: convert_legacy_steps(referees, post_steps),
                merge,
                outputs: OutputConfig::default(),
                context_cache: ContextCacheConfig::default(),
                use_orientation: true,
                orientation_prompt: String::new(),
                orientation_schema: None,
                extraction: ExtractionConfig::default(),
                parallel_context_template: default_parallel_template(),
                variables: Vec::new(),
            });
        }
        _ => {}
    }

    // Try bare legacy PipelineConfig (oldest format: top-level referees/post_steps)
    if value.get("referees").is_some() {
        let referees: Vec<LegacyRefereeConfig> =
            serde_json::from_value(value["referees"].clone()).unwrap_or_default();
        let post_steps: Vec<LegacyPostStepConfig> = value
            .get("post_steps")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let merge: MergeConfig = value
            .get("merge")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        return Ok(ExportEnvelope::Profile {
            schema_version: 1,
            name: "Imported".into(),
            steps: convert_legacy_steps(referees, post_steps),
            merge,
            outputs: OutputConfig::default(),
            context_cache: ContextCacheConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
            variables: Vec::new(),
        });
    }

    Err("Unrecognized file format".into())
}

#[allow(clippy::too_many_arguments)]
pub fn import_profile_data(
    name: &str,
    steps: Vec<StepConfig>,
    merge: MergeConfig,
    outputs: OutputConfig,
    context_cache: ContextCacheConfig,
    _use_orientation: bool,
    orientation_prompt: String,
    orientation_schema: Option<serde_json::Value>,
    extraction: ExtractionConfig,
    parallel_context_template: String,
    variables: Vec<VarSpec>,
) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let _lock = lock_profile_mutations()?;
    validate_unique_step_ids(&steps)?;
    validate_dependencies(&steps)?;
    validate_enabled_sequential_step(&steps)?;
    let mut id = slugify(name);
    if id.is_empty() {
        id = format!("imported-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    }
    // Deduplicate ID if it already exists
    let base_id = id.clone();
    let mut counter = 1u32;
    while profile_path(&id)?.exists() {
        id = format!("{base_id}-{counter}");
        counter += 1;
    }
    let mut profile = ProfileData::new(name, steps, merge);
    profile.outputs = outputs;
    profile.context_cache = context_cache;
    profile.use_orientation = true;
    profile.orientation_prompt = orientation_prompt;
    profile.orientation_schema = orientation_schema;
    profile.extraction = extraction;
    profile.parallel_context_template = parallel_context_template;
    profile.variables = variables;
    save_profile_unlocked(&id, &profile)?;
    Ok(profile_summary(id, &profile))
}

/// Install a validated portable workflow as a new profile. Installation is
/// explicit; `run --workflow` never mutates the profile store.
pub fn install_workflow_document(workflow: &WorkflowDocument) -> Result<ProfileSummary, String> {
    import_profile_data(
        &workflow.name,
        workflow.config.steps.clone(),
        workflow.config.merge.clone(),
        workflow.config.outputs.clone(),
        workflow.config.context_cache.clone(),
        workflow.config.use_orientation,
        workflow.config.orientation_prompt.clone(),
        workflow.config.orientation_schema.clone(),
        workflow.config.extraction.clone(),
        workflow.config.parallel_context_template.clone(),
        workflow.config.variables.clone(),
    )
}

pub fn import_bundle(json: &str) -> Result<(), String> {
    let raw: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid bundle: {e}"))?;
    let envelope: ExportEnvelope =
        serde_json::from_str(json).map_err(|e| format!("Invalid bundle: {e}"))?;
    match with_older_steps_wired(envelope, &raw)? {
        ExportEnvelope::Bundle {
            settings: imported_settings,
            profiles,
            active_profile,
        } => {
            let validated = validate_bundle_profiles(&profiles, &active_profile)?;
            // Older bundles carry values the loader normalizes on every launch
            // (retired providers, retired access modes); validate what the
            // app would actually load, not the raw export.
            let imported_settings = imported_settings.normalized_for_import();
            imported_settings.validate()?;
            let keep_as_saved: std::collections::HashSet<&str> = profiles
                .iter()
                .filter(|profile| profile.invalid.is_some())
                .map(|profile| profile.id.as_str())
                .collect();
            if imported_settings.active_profile != active_profile {
                return Err(format!(
                    "Bundle active profile mismatch: settings name '{}', envelope names '{}'",
                    imported_settings.active_profile, active_profile
                ));
            }
            let _lock = lock_profile_mutations()?;
            // The backup's settings over this computer's. A bearer token is
            // scoped to its endpoint: carrying a local token across an imported
            // server URL can disclose it to a different host on the next request.
            let current = crate::settings::load_persisted_required()?;
            let mut restored = restored_settings(&current, &imported_settings, &raw["settings"])?;
            if current.local_base_url != restored.local_base_url {
                restored.local_api_key.clear();
            }
            restored.active_profile = active_profile;

            // Snapshot every destination before the first mutation. If any
            // profile or the final settings write fails, restore the exact
            // previous bytes (or remove a newly-created file).
            let snapshots: Vec<(PathBuf, Option<Vec<u8>>)> = validated
                .iter()
                .map(|(id, _)| {
                    let path = profile_path(id)?;
                    let prior = match fs::symlink_metadata(&path) {
                        Ok(_) => Some(read_profile_file(&path)?.into_bytes()),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => {
                            return Err(format!(
                                "Cannot snapshot existing profile '{}': {error}",
                                path.display()
                            ));
                        }
                    };
                    Ok((path, prior))
                })
                .collect::<Result<_, String>>()?;
            for (id, profile) in &validated {
                let saved = if keep_as_saved.contains(id.as_str()) {
                    write_profile_as_saved(id, profile)
                } else {
                    save_profile_unlocked(id, profile)
                };
                if let Err(error) = saved {
                    let rollback = restore_profile_snapshots(&snapshots);
                    return Err(match rollback {
                        Ok(()) => error,
                        Err(rollback_error) => {
                            format!("{error}; rollback also failed: {rollback_error}")
                        }
                    });
                }
            }
            // API keys are intentionally NOT overwritten from the import
            if let Err(error) = crate::settings::save(&restored) {
                let rollback = restore_profile_snapshots(&snapshots);
                return Err(match rollback {
                    Ok(()) => error,
                    Err(rollback_error) => {
                        format!("{error}; rollback also failed: {rollback_error}")
                    }
                });
            }
            Ok(())
        }
        _ => Err("Expected a bundle export file".into()),
    }
}

pub(super) fn validate_bundle_profiles(
    profiles: &[ProfileExport],
    active_profile: &str,
) -> Result<Vec<(String, ProfileData)>, String> {
    let mut ids = std::collections::HashSet::new();
    let mut validated = Vec::with_capacity(profiles.len());
    for profile in profiles {
        validate_profile_id(&profile.id)?;
        if !ids.insert(profile.id.clone()) {
            return Err(format!("Duplicate profile id '{}'", profile.id));
        }
        let data = profile.to_profile_data();
        // Every load validates, so one kept as it was still can't be run.
        if profile.invalid.is_none() {
            validate_profile_data(&data).map_err(|e| format!("Profile '{}': {e}", profile.name))?;
            validate_enabled_sequential_step(&data.steps)
                .map_err(|e| format!("Profile '{}': {e}", profile.name))?;
        }
        validated.push((profile.id.clone(), data));
    }
    if !ids.contains(active_profile) {
        return Err(format!(
            "Active profile '{active_profile}' is not present in the bundle"
        ));
    }
    Ok(validated)
}

/// A restored profile that needed repair when it was backed up, written
/// without the validator it is already known to fail.
fn write_profile_as_saved(id: &str, profile: &ProfileData) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(profile).map_err(|e| format!("Failed to serialize: {e}"))?;
    restore_profile_bytes(&profile_path(id)?, json.as_bytes())
        .map_err(|e| format!("Failed to save profile '{id}': {e}"))
}

fn restore_profile_snapshots(snapshots: &[(PathBuf, Option<Vec<u8>>)]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (path, prior) in snapshots {
        let result = match prior {
            Some(bytes) => restore_profile_bytes(path, bytes),
            None => {
                if path.exists() {
                    fs::remove_file(path)
                } else {
                    Ok(())
                }
            }
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

pub(super) fn restore_profile_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "profile has no parent")
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
