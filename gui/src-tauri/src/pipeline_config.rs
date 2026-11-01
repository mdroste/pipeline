//! Pipeline configuration: profiles, steps, and post-processing.
//! Profiles stored at ~/.pipeline/profiles/{id}.json.
//! Active profile tracked via settings.active_profile.

use crate::prompts;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_PROFILE_STEPS: usize = 100;
pub const MAX_STEP_PROMPT_BYTES: usize = 1024 * 1024;
pub const MAX_FAN_OUT_ITEMS: u32 = 100;
const MAX_PROFILE_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_STEP_LABEL_CHARS: usize = 200;
const MAX_PROFILE_NAME_CHARS: usize = 200;
const MAX_VARIABLES: usize = 100;
const MAX_EXTRA_INPUTS: usize = 100;
pub(crate) const MAX_OUTPUT_SCHEMA_BYTES: usize = 1024 * 1024;
const MAX_RUN_IF_PATTERN_BYTES: usize = 16 * 1024;
const MAX_JSON_POINTER_BYTES: usize = 4 * 1024;
const ALLOWED_TOOLS: &[&str] = &["WebSearch"];
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "antigravity", "local"];

// ── Paths & helpers ─────────────────────────────────────────────────

mod builtins;
mod declaude;
mod inputs;
mod legacy;
mod migrations;
mod persistence;
mod profile;
mod profiles;
mod settings_restore;
mod step;
mod storage;
mod validation;
mod workflow;

pub use builtins::{
    adaptive_orientation_defaults, default_parallel_template, generic_parallel_template,
};
pub use inputs::{
    ExtractionConfig, InputSlot, NamedProductSpec, OutputConfig, VarSpec, VarValidation,
};
pub use profile::{
    ContextCacheConfig, ExportEnvelope, MergeConfig, PipelineConfig, ProfileData, ProfileExport,
    ProfileSummary, CURRENT_SCHEMA_VERSION,
};
pub use step::{
    apply_agent_defaults, AnalyzerSpec, ArtifactSelector, DependencyPolicy, ForEach,
    ForEachArtifact, NamedInputArtifactPart, Phase, PrimaryArtifactPart, RunCondition,
    StepArtifactPart, StepConfig, StepContext, StepKind,
};

pub(crate) use persistence::load_profile;
pub use persistence::{load, load_required_profile_for, load_required_workflow_for, save_profile};
pub use profiles::{
    create_profile, delete_profile, duplicate_profile, export_bundle, export_profile_data,
    get_active_profile_id, import_bundle, import_envelope, import_profile_data,
    install_workflow_document, list_profiles, rename_profile, reset_defaults, save, save_for,
    switch_profile,
};
pub use storage::{sanitize_step_id, slugify};
pub(crate) use validation::{resolve_dependencies, validate_runtime_config};
pub use validation::{
    validate_dependencies, validate_enabled_sequential_step, validate_unique_step_ids,
    validate_workflow_semantics,
};
pub use workflow::{
    parse_workflow_document_strict, workflow_json_schema, workflow_template, WorkflowDocument,
};

use builtins::{
    auto_review_profile, builtin_primary_readers, configure_artifact_flow, create_builtin_profiles,
    defaults, full_review_profile, generic_profile, generic_starter_steps, grant_review_profile,
    profile_summary, quick_auto_review_profile, resolve_renamed_profile_id, slide_review_profile,
    BUILTIN_PROFILES, GRANT_REVIEW_ID, MIGRATED_BUILTIN_PROFILES, PAPER_REVIEW_FAST_ID,
    PAPER_REVIEW_ID, RENAMED_BUILTIN_PROFILES, RETIRED_BUILTIN_PROFILES, SLIDE_REVIEW_ID,
    V15_RETIRED_BUILTIN_PROFILES, V9_RETIRED_BUILTIN_PROFILES,
};
use declaude::{declaude_profile, DECLAUDE_ID};
use legacy::{
    convert_legacy_steps, older_step_with_context, post_step_to_step, referee_to_step,
    wire_implicit_context, LegacyPostStepConfig, LegacyProfileData, LegacyRefereeConfig,
};
use migrations::ensure_migrated;
use persistence::{
    lock_profile_mutations, read_profile, read_profile_file, save_profile_unlocked,
    save_profile_unlocked_at,
};
use profiles::restore_profile_bytes;
use storage::{profile_path, profiles_dir, validate_profile_id};
use validation::{validate_profile_data, validate_profile_steps};

#[cfg(test)]
use builtins::default_steps;
#[cfg(test)]
use migrations::{
    archive_retired_profile, compact_builtin_auto_review_prompts, migrate_builtin_catalog,
    migrate_legacy_files, migrate_renamed_builtin_ids, migrate_review_quality_prompt_defaults,
    prompt_digest, refresh_builtin_auto_review_contracts, CATALOG_MIGRATION_MARKERS,
};
#[cfg(test)]
use persistence::load_profile_at;
#[cfg(test)]
use profiles::{
    bundle_profiles_at, delete_profile_file_transactionally, duplicate_profile_data,
    list_profiles_at, reset_defaults_at, validate_bundle_profiles,
};
#[cfg(test)]
use settings_restore::{restored_settings, DEVICE_SETTINGS};
#[cfg(test)]
use validation::validate_run_conditions;

#[cfg(test)]
mod tests;
