//! Report contracts, Markdown normalization, rendering, and export formats.

mod artifact_contract;
pub mod formats;
mod markdown;
mod quality;
mod render;
mod summary;

pub use artifact_contract::{
    extract_report_envelope, extract_text_artifact, new_report_nonce, report_markers,
    structured_output_format, text_artifact_output_format, text_artifact_schema,
    ReportRejectionKind, ReportValidationError,
};
pub use markdown::{clean_export_markdown, normalize_math_delimiters, strip_to_report};
pub(crate) use markdown::{fence_marker, report_body};
pub(crate) use quality::describe_orientation;
pub use quality::{build_report_quality, review_is_complete, status_label, status_notes};
pub(crate) use render::capitalize;
pub use render::render_markdown;
pub(crate) use summary::estimate_cost;
pub use summary::{api_cost_of, model_price};

#[cfg(test)]
mod tests;
