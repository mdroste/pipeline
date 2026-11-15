//! Canonical, input-neutral findings published by completed runs.
//!
//! Step responses remain immutable provider evidence. Structured findings and
//! conservative legacy findings feed the same durable report product.

mod legacy;
mod parse;
mod products;
mod render;
mod schema;

pub use parse::parse_value;
pub use products::{build_run_products, canonical_findings, ensure_legacy_products};
pub use render::render_markdown;
pub use schema::{
    evidence_schema, legacy_evidence_schema, legacy_output_schema, output_schema,
    output_schema_for_taxonomy, validate_v2_semantics, validation_output_schema_for_taxonomy,
    CATEGORIES, FINDING_SCHEMA_VERSION,
};

const MAX_FINDINGS: usize = 1_000;
const MAX_EVIDENCE: usize = 50;
const MAX_SOURCES: usize = 100;
const MAX_TITLE_BYTES: usize = 1_000;
const MAX_BODY_BYTES: usize = 20_000;
const MAX_REFERENCE_BYTES: usize = 1_000;
const MAX_LINE: u32 = 10_000_000;

#[cfg(test)]
mod tests;

mod publication;
pub(crate) use publication::{check_publication, validate_declaration};
