//! Durable Review records for extraction, orientation, calls and reports.

mod calls;
mod extraction;
mod findings;
mod orientation;
mod report;

pub use calls::{StepCallRecord, StepOutput, ToolCallCounts, ToolCallKind};
pub use extraction::ExtractionResult;
pub use findings::{
    Finding, FindingEvidence, FindingSet, NamedRunProduct, RunProducts, ValidationDisposition,
};
pub use orientation::{
    paper_view, survey_hint, ExtractionQualityNote, FormalResult, NotationEntry, OrientationMap,
    PaperMetadata, PaperType, ReviewPlan, ReviewSelectionNote, SectionEntry, TableFigure,
};
pub use report::{EditorSynthesis, PipelineReport, RefereeReport, ReportQuality, StepFailure};

#[cfg(test)]
mod tests;
