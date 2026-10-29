//! Compiled prompt assets, shared by catalog fingerprints and prompt construction.

pub(super) const ORIENTATION_TEMPLATE: &str =
    include_str!("../../../../prompts/auto_review/orientation.md");
pub(super) const SLIDE_ORIENTATION_PREAMBLE: &str =
    include_str!("../../../../prompts/auto_review/orientation_slides.md");
pub(super) const SUBJECT_REVIEW_BASE: &str =
    include_str!("../../../../prompts/auto_review/subjects/_review_contract.md");
pub(super) const METHOD_REVIEW_BASE: &str =
    include_str!("../../../../prompts/auto_review/methods/_review_contract.md");
pub(super) const CORE_CONTRIBUTION: &str =
    include_str!("../../../../prompts/auto_review/core/contribution.md");
pub(super) const CORE_CONSISTENCY: &str =
    include_str!("../../../../prompts/auto_review/core/consistency.md");
pub(super) const CORE_EXPOSITION: &str =
    include_str!("../../../../prompts/auto_review/core/exposition.md");
pub(super) const SYNTHESIS: &str = include_str!("../../../../prompts/auto_review/synthesis.md");
pub(super) const VALIDATE: &str = include_str!("../../../../prompts/auto_review/validate.md");
