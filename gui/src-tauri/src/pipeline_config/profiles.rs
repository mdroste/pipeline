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
    validate_profile_steps(&config.steps)?;
    validate_enabled_sequential_step(&config.steps)?;
    let name = load_profile(profile_id)?.name;
    let profile = ProfileData::from_config(name, config);
    save_profile_unlocked(profile_id, &profile)
}

/// Reset active profile to defaults: the matching stock definition for a
/// built-in, otherwise the default (Auto Paper Review) pipeline.
pub fn reset_defaults() -> PipelineConfig {
    let d = if get_active_profile_id() == "grant-review" {
        grant_review_profile().into()
    } else {
        defaults()
    };
    let _ = save(&d);
    d
}

// ── Profile management ──────────────────────────────────────────────

pub fn get_active_profile_id() -> String {
    crate::settings::load_persisted().active_profile
}

pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    ensure_migrated()?;
    let dir = profiles_dir()?;
    let mut summaries = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("Profile listing");
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read profiles dir: {e}"))? {
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
        match load_profile(&id) {
            Ok(profile) => summaries.push(profile_summary(id, &profile)),
            Err(error) => eprintln!("WARNING: skipped invalid profile '{id}': {error}"),
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
        // If this was the active profile, switch back to auto-review. The
        // profile file remains recoverable until this settings write commits.
        crate::settings::replace_active_profile_if(id, "auto-review").map(|_| ())
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
    let profile = load_profile(id)?;
    crate::settings::set_active_profile(id)?;
    Ok(profile.into())
}

// ── Export/Import ───────────────────────────────────────────────────

pub fn export_profile_data(id: &str) -> Result<String, String> {
    ensure_migrated()?;
    let profile = load_profile(id)?;
    let envelope = ExportEnvelope::Profile {
        schema_version: CURRENT_SCHEMA_VERSION,
        name: profile.name,
        steps: profile.steps,
        merge: profile.merge,
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
    let summaries = list_profiles()?;
    let mut profiles = Vec::new();
    for s in &summaries {
        let profile = load_profile(&s.id)?;
        profiles.push(ProfileExport::from_profile(s.id.clone(), profile));
    }
    let envelope = ExportEnvelope::Bundle {
        active_profile: settings.active_profile.clone(),
        settings,
        profiles,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

/// Auto-detect and parse an export file, handling legacy formats.
pub fn import_envelope(json: &str) -> Result<ExportEnvelope, String> {
    // Try new format first
    if let Ok(envelope) = serde_json::from_str::<ExportEnvelope>(json) {
        return Ok(envelope);
    }

    // Parse as JSON value for legacy detection
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;

    match value.get("type").and_then(|t| t.as_str()) {
        Some("referee") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyRefereeConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid referee config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: referee_to_step(legacy),
                });
            }
        }
        Some("post_step") => {
            if let Some(data) = value.get("data") {
                let legacy: LegacyPostStepConfig = serde_json::from_value(data.clone())
                    .map_err(|e| format!("Invalid post-step config: {e}"))?;
                return Ok(ExportEnvelope::Step {
                    data: post_step_to_step(legacy),
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

pub fn import_bundle(json: &str) -> Result<(), String> {
    let envelope: ExportEnvelope =
        serde_json::from_str(json).map_err(|e| format!("Invalid bundle: {e}"))?;
    match envelope {
        ExportEnvelope::Bundle {
            settings: imported_settings,
            profiles,
            active_profile,
        } => {
            let validated = validate_bundle_profiles(&profiles, &active_profile)?;
            imported_settings.validate()?;
            if imported_settings.active_profile != active_profile {
                return Err(format!(
                    "Bundle active profile mismatch: settings name '{}', envelope names '{}'",
                    imported_settings.active_profile, active_profile
                ));
            }
            let _lock = lock_profile_mutations()?;
            // Merge imported settings with existing, preserving local API keys.
            let mut current = crate::settings::load_persisted_required()?;
            current.preferred_provider = imported_settings.preferred_provider;
            current.default_parallel_agents = imported_settings.default_parallel_agents;
            current.default_parallel_model_overrides =
                imported_settings.default_parallel_model_overrides;
            current.default_parallel_effort_overrides =
                imported_settings.default_parallel_effort_overrides;
            current.default_sequential_agent = imported_settings.default_sequential_agent;
            current.default_sequential_model_overrides =
                imported_settings.default_sequential_model_overrides;
            current.default_sequential_effort_overrides =
                imported_settings.default_sequential_effort_overrides;
            current.default_orientation_agent = imported_settings.default_orientation_agent;
            current.default_orientation_model_overrides =
                imported_settings.default_orientation_model_overrides;
            current.default_orientation_effort_overrides =
                imported_settings.default_orientation_effort_overrides;
            current.max_workers = imported_settings.max_workers;
            current.claude_model = imported_settings.claude_model;
            current.claude_cli_model_selection = imported_settings.claude_cli_model_selection;
            current.claude_api_model_selection = imported_settings.claude_api_model_selection;
            current.claude_effort = imported_settings.claude_effort;
            current.codex_model = imported_settings.codex_model;
            current.codex_cli_model_selection = imported_settings.codex_cli_model_selection;
            current.codex_api_model_selection = imported_settings.codex_api_model_selection;
            current.codex_effort = imported_settings.codex_effort;
            current.antigravity_cli_model_selection =
                imported_settings.antigravity_cli_model_selection;
            current.antigravity_api_model_selection =
                imported_settings.antigravity_api_model_selection;
            current.antigravity_effort = imported_settings.antigravity_effort;
            if current.local_base_url != imported_settings.local_base_url {
                // A bearer token is scoped to its endpoint. Carrying a local
                // token across an imported server URL can disclose it to a
                // different host on the next request.
                current.local_api_key.clear();
            }
            current.local_base_url = imported_settings.local_base_url;
            current.local_model = imported_settings.local_model;
            current.pdf_extractor = imported_settings.pdf_extractor;
            current.paddle_page_concurrency = imported_settings.paddle_page_concurrency;
            current.paddle_mtmd_batch_tokens = imported_settings.paddle_mtmd_batch_tokens;
            current.paddle_flash_attention = imported_settings.paddle_flash_attention;
            current.paddle_max_output_tokens = imported_settings.paddle_max_output_tokens;
            current.paddle_page_retries = imported_settings.paddle_page_retries;
            current.paddle_full_layout_detection = imported_settings.paddle_full_layout_detection;
            current.paddle_full_layout_threshold = imported_settings.paddle_full_layout_threshold;
            current.paddle_full_layout_nms = imported_settings.paddle_full_layout_nms;
            current.paddle_full_layout_merge_bboxes_mode =
                imported_settings.paddle_full_layout_merge_bboxes_mode;
            current.paddle_full_merge_layout_blocks =
                imported_settings.paddle_full_merge_layout_blocks;
            current.paddle_full_ocr_image_blocks = imported_settings.paddle_full_ocr_image_blocks;
            current.paddle_full_format_block_content =
                imported_settings.paddle_full_format_block_content;
            current.paddle_full_merge_tables = imported_settings.paddle_full_merge_tables;
            current.paddle_full_relevel_titles = imported_settings.paddle_full_relevel_titles;
            current.paddle_full_show_formula_numbers =
                imported_settings.paddle_full_show_formula_numbers;
            current.pdf_extraction_timeout_secs = imported_settings.pdf_extraction_timeout_secs;
            current.reuse_pdf_extraction_cache = imported_settings.reuse_pdf_extraction_cache;
            current.verbose_logging = imported_settings.verbose_logging;
            current.step_timeout_secs = imported_settings.step_timeout_secs;
            current.max_retries = imported_settings.max_retries;
            current.max_saved_runs = imported_settings.max_saved_runs;
            current.active_profile = active_profile;

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
                if let Err(error) = save_profile_unlocked(id, profile) {
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
            if let Err(error) = crate::settings::save(&current) {
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
        validate_profile_data(&data).map_err(|e| format!("Profile '{}': {e}", profile.name))?;
        validate_enabled_sequential_step(&data.steps)
            .map_err(|e| format!("Profile '{}': {e}", profile.name))?;
        validated.push((profile.id.clone(), data));
    }
    if !ids.contains(active_profile) {
        return Err(format!(
            "Active profile '{active_profile}' is not present in the bundle"
        ));
    }
    Ok(validated)
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
