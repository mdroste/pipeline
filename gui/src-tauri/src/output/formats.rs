//! Shareable Review report formats: one self-contained HTML file, a fresh
//! Word report, Word comments on the reviewed `.docx`, and highlights on the
//! reviewed PDF.
//!
//! Every renderer here is a pure function over bytes the caller has already
//! read from an immutable run. Nothing in this module writes to run storage,
//! and model-written text is always treated as untrusted input: it is escaped
//! for HTML/XML and stripped of characters those formats cannot carry.

mod docx_comments;
mod docx_markdown;
mod docx_package;
mod docx_scan;
mod html;
mod model;
mod pdf_annotate;
mod pdf_words;
mod quote_match;
mod text;

pub use docx_comments::{annotate_docx, DocxAnnotation};
pub use docx_package::render_docx_report;
pub use html::render_html_report;
pub use model::{ExportEvidence, ExportFinding, ReportExport, ReportMetadata, Verification};
pub use pdf_annotate::{annotate_pdf, PdfAnnotation};
pub use pdf_words::{parse_bbox_layout, pdftotext_bbox_layout, PageWords};

/// Author recorded on Word comments and PDF annotations.
pub const ANNOTATION_AUTHOR: &str = "Pipeline Review";

#[cfg(test)]
mod tests;
