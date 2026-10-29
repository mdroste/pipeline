//! Curated reviewer-role catalog for the built-in Paper Review (Auto) profile.
//!
//! The orientation model may select only stable IDs from this catalog. It
//! never creates executable steps, prompts, tools, providers, or model policy;
//! the host materializes only the selected `StepConfig`s from these static
//! entries and the existing executor handles the run.

mod assets;
mod catalog;
mod materialize;
mod presentation;
mod prompts;
mod schema_contract;
mod schemas;
mod selection;

pub use catalog::{
    catalog, catalog_revision, label_for, specialist_count, AutoReviewCatalog, CatalogGroup,
    CatalogRole, GenreSpec, MethodLevel, MethodSpec, SubjectLevel, SubjectSpec, GENRES, METHODS,
    RESEARCH_ARTICLE_GENRE, SLIDE_DECK_GENRE, SUBJECTS,
};
pub(crate) use materialize::uses_auto_review_contract;
pub use materialize::{
    copyable_specialist_step, materialize_config, quick_steps, steps,
    validate_auto_review_preflight,
};
pub use presentation::{specialist_context_text, specialist_report_markdown};
pub use prompts::{
    apply_agent_count_instruction, expand_orientation_prompt, orientation_prompt,
    slide_orientation_prompt,
};
pub use schemas::{
    is_specialist_schema, orientation_schema, quick_orientation_schema, resolve_schema_catalogs,
    slide_orientation_schema, specialist_schema,
};
pub use selection::{
    adaptive_agent_bounds, configured_agent_count, validate_contract_for_schema,
    validate_profile_contract_identity, validate_review_plan, validate_review_plan_object,
    validate_schema_settings, AdaptiveAgentBounds,
};

/// v2: specialist and core reviewer steps return the structured specialist
/// findings schema, the consolidated findings contract carries `sources` and
/// typed category/priority enums, and `review_plan` drops its unconsumed
/// free-text `methods` array. This pre-release contract intentionally has no
/// migration path; stale built-in profiles are archived and recreated.
pub const AUTO_REVIEW_CONTRACT: &str = "auto-review-v2";
pub const ADAPTIVE_AGENT_COUNT_KEY: &str = "x-pipeline-adaptive-agent-count";
pub const CATALOG_REFERENCE_KEY: &str = "x-pipeline-catalog";
pub const CATALOG_POLICY_KEY: &str = "x-pipeline-catalog-policy";
pub const SUBJECT_CATALOG: &str = "auto-review.subjects";
pub const METHOD_CATALOG: &str = "auto-review.methods";
pub const GENRE_CATALOG: &str = "auto-review.genres";
pub const MIN_ADAPTIVE_AGENTS: usize = 2;
pub const MAX_ADAPTIVE_AGENTS: usize = 6;

#[cfg(test)]
mod tests;
