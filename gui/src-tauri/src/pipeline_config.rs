//! Pipeline configuration: profiles, steps, and post-processing.
//! Profiles stored at ~/.pipeline/profiles/{id}.json.
//! Active profile tracked via settings.active_profile.

use crate::prompts;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Data types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Parallel,
    Sequential,
}

fn default_tools() -> Vec<String> {
    vec![]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    pub id: String,
    pub label: String,
    pub prompt: String,
    pub enabled: bool,
    pub phase: Phase,
    #[serde(default = "default_tools")]
    pub tools: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    /// Per-step model override. Empty = use the global setting for this step's provider.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Per-step effort override (low/medium/high/max). Empty = use the global setting.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub effort: String,
}

impl Default for StepConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            prompt: String::new(),
            enabled: true,
            phase: Phase::Parallel,
            tools: Vec::new(),
            agents: Vec::new(),
            model: String::new(),
            effort: String::new(),
        }
    }
}

/// Configuration for the cross-agent merge step.
/// When a step runs on multiple LLM agents, this merges their
/// independent outputs into a single report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConfig {
    pub enabled: bool,
    /// Prompt template. Placeholders replaced at runtime:
    ///   {topic}          — step label (e.g., "Contribution")
    ///   {agent_reports}  — all agent analyses for this step, as markdown
    pub prompt: String,
    /// Which provider runs the merge calls. Empty = global setting.
    #[serde(default)]
    pub agents: Vec<String>,
}

/// Per-profile extraction overrides. When `method` is empty, the global
/// Settings value is used; same for the marker flags (which fall back to
/// the global toggle when this struct is absent on a profile).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// "auto" | "llm" | "marker" | "pdftotext" | "" (= inherit global).
    #[serde(default)]
    pub method: String,
    /// `Some` overrides the global setting; `None` inherits.
    #[serde(default)]
    pub marker_disable_ocr: Option<bool>,
    #[serde(default)]
    pub marker_disable_images: Option<bool>,
}

/// Combined config returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    /// Whether to build an orientation map before running steps.
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    /// Per-profile orientation prompt. Empty = use the default template loaded
    /// from prompts/orientation.md (or the user override at
    /// ~/.pipeline/prompts/orientation.md). The placeholder `{paper_text}` is
    /// substituted with the extracted paper at runtime.
    #[serde(default)]
    pub orientation_prompt: String,
    /// Per-profile extraction overrides. When unset (default), the global
    /// Settings values are used.
    #[serde(default)]
    pub extraction: ExtractionConfig,
    /// Template wrapping each parallel step's prompt. Placeholders:
    ///   {step_prompt}  — the step's own instructions
    ///   {paper_type}   — theory / empirical / mixed
    ///   {orientation}  — orientation map reference (empty if disabled)
    ///   {paper_path}   — path to extracted paper text
    ///   {figure_hint}  — figure/table access instructions
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
}

/// On-disk profile format stored in ~/.pipeline/profiles/{id}.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileData {
    pub name: String,
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
}

impl ProfileData {
    /// Create a new profile with default pipeline settings.
    pub fn new(name: impl Into<String>, steps: Vec<StepConfig>, merge: MergeConfig) -> Self {
        Self {
            name: name.into(),
            steps,
            merge,
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Default parallel-step context template, with user override applied from
/// ~/.pipeline/prompts/parallel_context.md if present.
pub fn default_parallel_template() -> String {
    prompts::load_prompt("parallel_context").unwrap_or_default()
}

/// Summary returned when listing profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub step_count: usize,
}

// ── Export/Import envelope ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExportEnvelope {
    #[serde(rename = "step")]
    Step { data: StepConfig },
    #[serde(rename = "profile")]
    Profile {
        name: String,
        steps: Vec<StepConfig>,
        #[serde(default)]
        merge: MergeConfig,
        #[serde(default = "default_true")]
        use_orientation: bool,
        #[serde(default)]
        orientation_prompt: String,
        #[serde(default)]
        extraction: ExtractionConfig,
        #[serde(default = "default_parallel_template")]
        parallel_context_template: String,
    },
    #[serde(rename = "bundle")]
    Bundle {
        settings: crate::settings::Settings,
        profiles: Vec<ProfileExport>,
        active_profile: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExport {
    pub id: String,
    pub name: String,
    pub steps: Vec<StepConfig>,
    #[serde(default)]
    pub merge: MergeConfig,
    #[serde(default = "default_true")]
    pub use_orientation: bool,
    #[serde(default)]
    pub orientation_prompt: String,
    #[serde(default)]
    pub extraction: ExtractionConfig,
    #[serde(default = "default_parallel_template")]
    pub parallel_context_template: String,
}

// ── Legacy types (deserialization only) ────────────────────────────

#[derive(Deserialize)]
struct LegacyRefereeConfig {
    id: String,
    label: String,
    prompt: String,
    enabled: bool,
    #[serde(default)]
    web_search: bool,
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct LegacyPostStepConfig {
    id: String,
    label: String,
    prompt: String,
    enabled: bool,
    #[serde(default)]
    agents: Vec<String>,
}

#[derive(Deserialize)]
struct LegacyProfileData {
    name: String,
    #[serde(default)]
    referees: Vec<LegacyRefereeConfig>,
    #[serde(default)]
    post_steps: Vec<LegacyPostStepConfig>,
    #[serde(default)]
    merge: MergeConfig,
}

fn referee_to_step(r: LegacyRefereeConfig) -> StepConfig {
    let tools = if r.web_search {
        vec!["WebSearch".to_string()]
    } else {
        vec![]
    };
    StepConfig {
        id: r.id,
        label: r.label,
        prompt: r.prompt,
        enabled: r.enabled,
        phase: Phase::Parallel,
        tools,
        agents: r.agents,
        ..Default::default()
    }
}

fn post_step_to_step(p: LegacyPostStepConfig) -> StepConfig {
    StepConfig {
        id: p.id,
        label: p.label,
        prompt: p.prompt,
        enabled: p.enabled,
        phase: Phase::Sequential,
        tools: vec![],
        agents: p.agents,
        ..Default::default()
    }
}

fn convert_legacy_steps(
    referees: Vec<LegacyRefereeConfig>,
    post_steps: Vec<LegacyPostStepConfig>,
) -> Vec<StepConfig> {
    let mut steps: Vec<StepConfig> = referees.into_iter().map(referee_to_step).collect();
    steps.extend(post_steps.into_iter().map(post_step_to_step));
    steps
}

// ── Paths & helpers ─────────────────────────────────────────────────

fn profiles_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("profiles");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create profiles dir: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)) {
            eprintln!("WARNING: could not tighten permissions on {}: {e}", dir.display());
        }
    }
    Ok(dir)
}

fn profile_path(id: &str) -> Result<PathBuf, String> {
    validate_profile_id(id)?;
    Ok(profiles_dir()?.join(format!("{id}.json")))
}

fn validate_profile_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Profile ID cannot be empty".into());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Profile ID must contain only letters, numbers, hyphens, and underscores".into(),
        );
    }
    Ok(())
}

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Sanitize a step ID by replacing characters reserved for internal use.
/// `/` is the step_id/agent delimiter in multi-agent composite keys;
/// allowing it in step IDs would corrupt merge grouping and multi-agent detection.
pub fn sanitize_step_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .collect();
    // Collapse consecutive dashes and trim leading/trailing
    sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Sanitize all step IDs in a list, replacing reserved characters.
fn sanitize_steps(steps: &mut [StepConfig]) {
    for step in steps.iter_mut() {
        let clean = sanitize_step_id(&step.id);
        if clean != step.id {
            step.id = clean;
        }
    }
}

// ── Defaults ────────────────────────────────────────────────────────
//
// The four pipeline-level prompts (parallel_context, merge, editor_synthesis,
// validate_feedback) live in prompts/*.md and are loaded through prompts::load_prompt,
// which lets users override them at ~/.pipeline/prompts/<name>.md.

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prompt: prompts::load_prompt("merge").unwrap_or_default(),
            agents: vec![],
        }
    }
}

fn default_steps() -> Vec<StepConfig> {
    vec![
        StepConfig {
            id: "contribution".into(),
            label: "Contribution".into(),
            prompt: prompts::load_prompt("contribution").unwrap_or_default(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec!["WebSearch".into()],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "technical".into(),
            label: "Technical Correctness".into(),
            prompt: prompts::load_prompt("technical").unwrap_or_default(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "empirical".into(),
            label: "Empirical Strategy".into(),
            prompt: prompts::load_prompt("empirical").unwrap_or_default(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "consistency".into(),
            label: "Internal Consistency".into(),
            prompt: prompts::load_prompt("consistency").unwrap_or_default(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "exposition".into(),
            label: "Exposition & Framing".into(),
            prompt: prompts::load_prompt("exposition").unwrap_or_default(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "editor_synthesis".into(),
            label: "Consolidate Issues".into(),
            prompt: prompts::load_prompt("editor_synthesis").unwrap_or_default(),
            enabled: true,
            phase: Phase::Sequential,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
        StepConfig {
            id: "validate_feedback".into(),
            label: "Validate Feedback".into(),
            prompt: prompts::load_prompt("validate_feedback").unwrap_or_default(),
            enabled: false,
            phase: Phase::Sequential,
            tools: vec![],
            agents: vec![],
            ..Default::default()
        },
    ]
}

fn defaults() -> PipelineConfig {
    PipelineConfig {
        steps: default_steps(),
        merge: MergeConfig::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        extraction: ExtractionConfig::default(),
        parallel_context_template: default_parallel_template(),
    }
}

/// Profile IDs that cannot be deleted.
// All three are recreated by ensure_builtin_profiles() on startup, so
// deleting any of them would silently "undo" itself — block deletion for all.
const BUILTIN_PROFILES: &[&str] = &["deep-review", "quick-review", "empirical"];

/// Built-in profiles created on first run.
fn create_builtin_profiles() -> Result<(), String> {
    let profiles = profiles_dir()?;

    // Quick Review — fast two-step pass
    let quick_path = profiles.join("quick-review.json");
    if !quick_path.exists() {
        let profile = ProfileData {
            name: "Quick Review".into(),
            steps: vec![
                StepConfig {
                    id: "contribution".into(),
                    label: "Contribution".into(),
                    prompt: prompts::load_prompt("contribution").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "consistency".into(),
                    label: "Internal Consistency".into(),
                    prompt: prompts::load_prompt("consistency").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "editor_synthesis".into(),
                    label: "Consolidate Issues".into(),
                    prompt: prompts::load_prompt("editor_synthesis").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Sequential,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
            ],
            merge: MergeConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
        };
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|e| format!("Serialize error: {e}"))?;
        fs::write(&quick_path, json)
            .map_err(|e| format!("Failed to write quick-review profile: {e}"))?;
    }

    // Empirical — tailored for empirical papers
    let empirical_path = profiles.join("empirical.json");
    if !empirical_path.exists() {
        let profile = ProfileData {
            name: "Empirical".into(),
            steps: vec![
                StepConfig {
                    id: "contribution".into(),
                    label: "Contribution".into(),
                    prompt: prompts::load_prompt("contribution").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec!["WebSearch".into()],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "empirical".into(),
                    label: "Empirical Strategy".into(),
                    prompt: prompts::load_prompt("empirical").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "consistency".into(),
                    label: "Internal Consistency".into(),
                    prompt: prompts::load_prompt("consistency").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "exposition".into(),
                    label: "Exposition & Framing".into(),
                    prompt: prompts::load_prompt("exposition").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Parallel,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "editor_synthesis".into(),
                    label: "Consolidate Issues".into(),
                    prompt: prompts::load_prompt("editor_synthesis").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Sequential,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
                StepConfig {
                    id: "validate_feedback".into(),
                    label: "Validate Feedback".into(),
                    prompt: prompts::load_prompt("validate_feedback").unwrap_or_default(),
                    enabled: true,
                    phase: Phase::Sequential,
                    tools: vec![],
                    agents: vec![],
                    ..Default::default()
                },
            ],
            merge: MergeConfig::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
        };
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|e| format!("Serialize error: {e}"))?;
        fs::write(&empirical_path, json)
            .map_err(|e| format!("Failed to write empirical profile: {e}"))?;
    }

    Ok(())
}

// ── Migration ───────────────────────────────────────────────────────

/// Migrate from old single-file formats to profiles directory. Idempotent.
fn ensure_migrated() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let old_path = home.join(".pipeline").join("pipeline.json");
    let profiles = profiles_dir()?;
    let deep_review_path = profiles.join("deep-review.json");

    if !deep_review_path.exists() {
        // Migrate from pipeline.json → save as a "Migrated" profile
        if old_path.exists() {
            if let Ok(content) = fs::read_to_string(&old_path) {
                // Try legacy format with referees/post_steps
                if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
                    let profile = ProfileData::new("Migrated", convert_legacy_steps(legacy.referees, legacy.post_steps), legacy.merge);
                    let migrated_path = profiles.join("migrated.json");
                    let json = serde_json::to_string_pretty(&profile)
                        .map_err(|e| format!("Serialize error: {e}"))?;
                    fs::write(&migrated_path, json)
                        .map_err(|e| format!("Failed to write migrated profile: {e}"))?;
                }
                let _ = fs::remove_file(&old_path);
            }
        }

        // Migrate from old referees.json
        let old_referees = home.join(".pipeline").join("referees.json");
        if old_referees.exists() {
            if let Ok(content) = fs::read_to_string(&old_referees) {
                if let Ok(referees) = serde_json::from_str::<Vec<LegacyRefereeConfig>>(&content) {
                    let profile = ProfileData::new("Migrated", convert_legacy_steps(referees, vec![]), MergeConfig::default());
                    let migrated_path = profiles.join("migrated.json");
                    if !migrated_path.exists() {
                        let json = serde_json::to_string_pretty(&profile)
                            .map_err(|e| format!("Serialize error: {e}"))?;
                        fs::write(&migrated_path, json)
                            .map_err(|e| format!("Failed to write migrated profile: {e}"))?;
                    }
                    let _ = fs::remove_file(&old_referees);
                }
            }
        }

        // Also remove old default.json if present (from prior version)
        let old_default = profiles.join("default.json");
        if old_default.exists() {
            let _ = fs::remove_file(&old_default);
        }

        // Create Deep Review as the primary profile
        let profile = ProfileData::new("Deep Review", {
            let mut s = default_steps();
            if let Some(step) = s.iter_mut().find(|s| s.id == "validate_feedback") {
                step.enabled = true;
            }
            s
        }, MergeConfig::default());
        let json = serde_json::to_string_pretty(&profile)
            .map_err(|e| format!("Serialize error: {e}"))?;
        fs::write(&deep_review_path, json)
            .map_err(|e| format!("Failed to write deep-review profile: {e}"))?;
    } else if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }

    // Always ensure other built-in profiles exist
    create_builtin_profiles()?;

    Ok(())
}

// ── Profile I/O ─────────────────────────────────────────────────────

fn load_profile(id: &str) -> Result<ProfileData, String> {
    let path = profile_path(id)?;
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read profile '{id}': {e}"))?;

    // Try new format (has steps array)
    if let Ok(mut profile) = serde_json::from_str::<ProfileData>(&content) {
        if !profile.steps.is_empty() {
            sanitize_steps(&mut profile.steps);
            return Ok(profile);
        }
    }

    // Try legacy format (has referees/post_steps arrays)
    if let Ok(legacy) = serde_json::from_str::<LegacyProfileData>(&content) {
        if !legacy.referees.is_empty() || !legacy.post_steps.is_empty() {
            let profile = ProfileData::new(legacy.name, convert_legacy_steps(legacy.referees, legacy.post_steps), legacy.merge);
            // Write back in new format
            let _ = save_profile(id, &profile);
            return Ok(profile);
        }
    }

    Err(format!("Failed to parse profile '{id}'"))
}

pub fn save_profile(id: &str, profile: &ProfileData) -> Result<(), String> {
    use std::io::Write as _;
    let path = profile_path(id)?;
    let mut clean = profile.clone();
    sanitize_steps(&mut clean.steps);
    let json =
        serde_json::to_string_pretty(&clean).map_err(|e| format!("Failed to serialize: {e}"))?;
    // Atomic write with a unique temp name so concurrent writers for the same
    // profile can't clobber each other's .tmp file.
    let dir = path.parent().ok_or_else(|| format!("No parent dir for {}", path.display()))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| format!("Failed to create temp file in {}: {e}", dir.display()))?;
    tmp.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write profile: {e}"))?;
    tmp.persist(&path)
        .map_err(|e| format!("Failed to save profile: {}", e.error))?;
    Ok(())
}

// ── Public API ──────────────────────────────────────────────────────

/// Load the active profile as a PipelineConfig.
pub fn load() -> PipelineConfig {
    let _ = ensure_migrated();
    let settings = crate::settings::load();
    match load_profile(&settings.active_profile) {
        Ok(profile) => PipelineConfig {
            steps: profile.steps,
            merge: profile.merge,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
        },
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

/// Step ids key the executor's pass events and the merge grouping
/// (`{id}/{agent}`), so duplicates silently collide. Reject them at save and
/// import time. Deliberately not enforced on load/migration, so an existing
/// profile with duplicates can still be opened and repaired in the editor.
pub fn validate_unique_step_ids(steps: &[StepConfig]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for step in steps {
        if !seen.insert(step.id.as_str()) {
            return Err(format!(
                "Duplicate step id '{}' — step ids must be unique",
                step.id
            ));
        }
    }
    Ok(())
}

/// Save to the active profile.
pub fn save(config: &PipelineConfig) -> Result<(), String> {
    let _ = ensure_migrated();
    validate_unique_step_ids(&config.steps)?;
    let settings = crate::settings::load();
    let name = load_profile(&settings.active_profile)
        .map(|p| p.name)
        .unwrap_or_else(|_| "Default".into());
    let profile = ProfileData {
        name,
        steps: config.steps.clone(),
        merge: config.merge.clone(),
        use_orientation: config.use_orientation,
        orientation_prompt: config.orientation_prompt.clone(),
        extraction: config.extraction.clone(),
        parallel_context_template: config.parallel_context_template.clone(),
    };
    save_profile(&settings.active_profile, &profile)
}

/// Reset active profile to defaults.
pub fn reset_defaults() -> PipelineConfig {
    let d = defaults();
    let _ = save(&d);
    d
}

/// Load steps from the active profile.
pub fn load_steps() -> Vec<StepConfig> {
    load().steps
}

// ── Profile management ──────────────────────────────────────────────

pub fn get_active_profile_id() -> String {
    crate::settings::load().active_profile
}

pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    let _ = ensure_migrated();
    let dir = profiles_dir()?;
    let mut summaries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read profiles dir: {e}"))? {
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
        if let Ok(profile) = load_profile(&id) {
            summaries.push(ProfileSummary {
                id,
                name: profile.name,
                step_count: profile.steps.len(),
            });
        }
    }
    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(summaries)
}

pub fn create_profile(name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let id = slugify(name);
    if id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{id}' already exists"));
    }
    let profile = ProfileData::new(name, default_steps(), MergeConfig::default());
    save_profile(&id, &profile)?;
    Ok(ProfileSummary {
        id,
        name: name.to_string(),
        step_count: profile.steps.len(),
    })
}

pub fn duplicate_profile(source_id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let source = load_profile(source_id)?;
    let new_id = slugify(new_name);
    if new_id.is_empty() {
        return Err("Profile name produces empty ID".into());
    }
    let path = profile_path(&new_id)?;
    if path.exists() {
        return Err(format!("A profile with ID '{new_id}' already exists"));
    }
    let mut profile = ProfileData::new(new_name, source.steps, source.merge);
    profile.use_orientation = source.use_orientation;
    profile.orientation_prompt = source.orientation_prompt;
    profile.extraction = source.extraction;
    profile.parallel_context_template = source.parallel_context_template;
    save_profile(&new_id, &profile)?;
    Ok(ProfileSummary {
        id: new_id,
        name: new_name.to_string(),
        step_count: profile.steps.len(),
    })
}

pub fn rename_profile(id: &str, new_name: &str) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    let mut profile = load_profile(id)?;
    profile.name = new_name.to_string();
    save_profile(id, &profile)?;
    Ok(ProfileSummary {
        id: id.to_string(),
        name: new_name.to_string(),
        step_count: profile.steps.len(),
    })
}

pub fn delete_profile(id: &str) -> Result<(), String> {
    if BUILTIN_PROFILES.contains(&id) {
        return Err(format!("Cannot delete the built-in profile '{id}'"));
    }
    let path = profile_path(id)?;
    if !path.exists() {
        return Err(format!("Profile '{id}' does not exist"));
    }
    fs::remove_file(&path).map_err(|e| format!("Failed to delete profile: {e}"))?;

    // If this was the active profile, switch back to deep-review
    let mut settings = crate::settings::load();
    if settings.active_profile == id {
        settings.active_profile = "deep-review".into();
        crate::settings::save(&settings)?;
    }
    Ok(())
}

pub fn switch_profile(id: &str) -> Result<PipelineConfig, String> {
    let _ = ensure_migrated();
    let profile = load_profile(id)?;
    let mut settings = crate::settings::load();
    settings.active_profile = id.to_string();
    crate::settings::save(&settings)?;
    Ok(PipelineConfig {
        steps: profile.steps,
        merge: profile.merge,
        use_orientation: profile.use_orientation,
        orientation_prompt: profile.orientation_prompt,
        extraction: profile.extraction,
        parallel_context_template: profile.parallel_context_template,
    })
}

// ── Export/Import ───────────────────────────────────────────────────

pub fn export_step_data(step: &StepConfig) -> Result<String, String> {
    let envelope = ExportEnvelope::Step { data: step.clone() };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

pub fn export_profile_data(id: &str) -> Result<String, String> {
    let profile = load_profile(id)?;
    let envelope = ExportEnvelope::Profile {
        name: profile.name,
        steps: profile.steps,
        merge: profile.merge,
        use_orientation: profile.use_orientation,
        orientation_prompt: profile.orientation_prompt,
        extraction: profile.extraction,
        parallel_context_template: profile.parallel_context_template,
    };
    serde_json::to_string_pretty(&envelope).map_err(|e| format!("Serialize error: {e}"))
}

pub fn export_bundle() -> Result<String, String> {
    let _ = ensure_migrated();
    let mut settings = crate::settings::load();
    // Strip API keys from the export to prevent credential leakage
    settings.anthropic_api_key = String::new();
    settings.openai_api_key = String::new();
    settings.google_api_key = String::new();
    let summaries = list_profiles()?;
    let mut profiles = Vec::new();
    for s in &summaries {
        let profile = load_profile(&s.id)?;
        profiles.push(ProfileExport {
            id: s.id.clone(),
            name: profile.name,
            steps: profile.steps,
            merge: profile.merge,
            use_orientation: profile.use_orientation,
            orientation_prompt: profile.orientation_prompt,
            extraction: profile.extraction,
            parallel_context_template: profile.parallel_context_template,
        });
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
                name,
                steps: convert_legacy_steps(referees, post_steps),
                merge,
                use_orientation: true,
                orientation_prompt: String::new(),
                extraction: ExtractionConfig::default(),
                parallel_context_template: default_parallel_template(),
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
            name: "Imported".into(),
            steps: convert_legacy_steps(referees, post_steps),
            merge,
            use_orientation: true,
            orientation_prompt: String::new(),
            extraction: ExtractionConfig::default(),
            parallel_context_template: default_parallel_template(),
        });
    }

    Err("Unrecognized file format".into())
}

pub fn import_profile_data(
    name: &str,
    steps: Vec<StepConfig>,
    merge: MergeConfig,
    use_orientation: bool,
    orientation_prompt: String,
    extraction: ExtractionConfig,
    parallel_context_template: String,
) -> Result<ProfileSummary, String> {
    let _ = ensure_migrated();
    validate_unique_step_ids(&steps)?;
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
    let sc = steps.len();
    let mut profile = ProfileData::new(name, steps, merge);
    profile.use_orientation = use_orientation;
    profile.orientation_prompt = orientation_prompt;
    profile.extraction = extraction;
    profile.parallel_context_template = parallel_context_template;
    save_profile(&id, &profile)?;
    Ok(ProfileSummary {
        id,
        name: name.to_string(),
        step_count: sc,
    })
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
            // Validate every profile before saving any, so a bad bundle
            // doesn't leave a partial import behind.
            for p in &profiles {
                validate_unique_step_ids(&p.steps)
                    .map_err(|e| format!("Profile '{}': {e}", p.name))?;
            }
            for p in &profiles {
                let profile = ProfileData {
                    name: p.name.clone(),
                    steps: p.steps.clone(),
                    merge: p.merge.clone(),
                    use_orientation: p.use_orientation,
                    orientation_prompt: p.orientation_prompt.clone(),
                    extraction: p.extraction.clone(),
                    parallel_context_template: p.parallel_context_template.clone(),
                };
                save_profile(&p.id, &profile)?;
            }
            // Merge imported settings with existing, preserving local API keys.
            // Validate numeric ranges and provider to prevent invalid configs.
            let mut current = crate::settings::load();
            let valid_providers = ["claude", "codex", "gemini"];
            if valid_providers.contains(&imported_settings.preferred_provider.as_str()) {
                current.preferred_provider = imported_settings.preferred_provider;
            }
            current.max_workers = imported_settings.max_workers.clamp(1, 10);
            current.claude_model = imported_settings.claude_model;
            current.claude_effort = imported_settings.claude_effort;
            current.codex_model = imported_settings.codex_model;
            current.codex_effort = imported_settings.codex_effort;
            current.gemini_model = imported_settings.gemini_model;
            current.pdf_extractor = imported_settings.pdf_extractor;
            current.marker_disable_ocr = imported_settings.marker_disable_ocr;
            current.marker_disable_images = imported_settings.marker_disable_images;
            current.verbose_logging = imported_settings.verbose_logging;
            current.step_timeout_secs = imported_settings.step_timeout_secs.clamp(60, 7200);
            current.max_retries = imported_settings.max_retries.clamp(0, 10);
            current.active_profile = active_profile;
            // API keys are intentionally NOT overwritten from the import
            crate::settings::save(&current)?;
            Ok(())
        }
        _ => Err("Expected a bundle export file".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_unique_step_ids ───────────────────────────────────

    fn step_with_id(id: &str) -> StepConfig {
        StepConfig {
            id: id.to_string(),
            label: id.to_string(),
            prompt: String::new(),
            enabled: true,
            phase: Phase::Parallel,
            tools: vec![],
            agents: vec![],
            model: String::new(),
            effort: String::new(),
        }
    }

    #[test]
    fn unique_step_ids_pass_validation() {
        let steps = vec![step_with_id("a"), step_with_id("b")];
        assert!(validate_unique_step_ids(&steps).is_ok());
    }

    #[test]
    fn duplicate_step_ids_are_rejected() {
        let steps = vec![step_with_id("a"), step_with_id("b"), step_with_id("a")];
        let err = validate_unique_step_ids(&steps).unwrap_err();
        assert!(err.contains("Duplicate step id 'a'"), "{err}");
    }

    // ── slugify ────────────────────────────────────────────────────

    #[test]
    fn slugify_normal() {
        assert_eq!(slugify("Deep Review"), "deep-review");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("My Profile!@#$%"), "my-profile");
    }

    #[test]
    fn slugify_consecutive_dashes() {
        assert_eq!(slugify("a---b"), "a-b");
    }

    #[test]
    fn slugify_leading_trailing() {
        assert_eq!(slugify("  Hello World  "), "hello-world");
    }

    #[test]
    fn slugify_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn slugify_numbers() {
        assert_eq!(slugify("Profile 2.0"), "profile-2-0");
    }

    // ── validate_profile_id ────────────────────────────────────────

    #[test]
    fn validate_id_valid() {
        assert!(validate_profile_id("deep-review").is_ok());
        assert!(validate_profile_id("my_profile_1").is_ok());
    }

    #[test]
    fn validate_id_empty() {
        assert!(validate_profile_id("").is_err());
    }

    #[test]
    fn validate_id_invalid_chars() {
        assert!(validate_profile_id("has spaces").is_err());
        assert!(validate_profile_id("has.dots").is_err());
        assert!(validate_profile_id("path/traversal").is_err());
    }

    // ── Legacy migration ───────────────────────────────────────────

    #[test]
    fn referee_to_step_basic() {
        let r = LegacyRefereeConfig {
            id: "contrib".into(),
            label: "Contribution".into(),
            prompt: "Review...".into(),
            enabled: true,
            web_search: false,
            agents: vec![],
        };
        let step = referee_to_step(r);
        assert_eq!(step.id, "contrib");
        assert_eq!(step.phase, Phase::Parallel);
        assert!(step.tools.is_empty());
    }

    #[test]
    fn referee_to_step_with_web_search() {
        let r = LegacyRefereeConfig {
            id: "contrib".into(),
            label: "Contribution".into(),
            prompt: "Review...".into(),
            enabled: true,
            web_search: true,
            agents: vec![],
        };
        let step = referee_to_step(r);
        assert_eq!(step.tools, vec!["WebSearch".to_string()]);
    }

    #[test]
    fn post_step_to_step_basic() {
        let p = LegacyPostStepConfig {
            id: "editor".into(),
            label: "Editor".into(),
            prompt: "Synthesize...".into(),
            enabled: true,
            agents: vec![],
        };
        let step = post_step_to_step(p);
        assert_eq!(step.id, "editor");
        assert_eq!(step.phase, Phase::Sequential);
    }

    #[test]
    fn convert_legacy_preserves_order() {
        let referees = vec![
            LegacyRefereeConfig { id: "r1".into(), label: "R1".into(), prompt: "p".into(), enabled: true, web_search: false, agents: vec![] },
            LegacyRefereeConfig { id: "r2".into(), label: "R2".into(), prompt: "p".into(), enabled: true, web_search: false, agents: vec![] },
        ];
        let post_steps = vec![
            LegacyPostStepConfig { id: "s1".into(), label: "S1".into(), prompt: "p".into(), enabled: true, agents: vec![] },
        ];
        let steps = convert_legacy_steps(referees, post_steps);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].id, "r1");
        assert_eq!(steps[1].id, "r2");
        assert_eq!(steps[2].id, "s1");
        assert_eq!(steps[0].phase, Phase::Parallel);
        assert_eq!(steps[2].phase, Phase::Sequential);
    }

    // ── sanitize_step_id ──────────────────────────────────────────

    #[test]
    fn sanitize_normal_id() {
        assert_eq!(sanitize_step_id("contribution"), "contribution");
        assert_eq!(sanitize_step_id("my-step_1"), "my-step_1");
    }

    #[test]
    fn sanitize_strips_slash() {
        // '/' is reserved for step_id/agent composite keys
        assert_eq!(sanitize_step_id("step/agent"), "step-agent");
        assert_eq!(sanitize_step_id("a/b/c"), "a-b-c");
    }

    #[test]
    fn sanitize_strips_special_chars() {
        assert_eq!(sanitize_step_id("step with spaces"), "step-with-spaces");
        assert_eq!(sanitize_step_id("step@#$%!"), "step");
    }

    #[test]
    fn sanitize_preserves_dots() {
        assert_eq!(sanitize_step_id("v2.1"), "v2.1");
    }

    #[test]
    fn sanitize_collapses_dashes() {
        assert_eq!(sanitize_step_id("a///b"), "a-b");
        assert_eq!(sanitize_step_id("--leading--"), "leading");
    }
}
