use super::{RunProducts, StepOutput};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Host-authored quality ledger. Unlike report prose, these fields are
/// derived deterministically from the extraction, execution, and validated
/// product artifacts and therefore remain stable across providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportQuality {
    #[serde(default)]
    pub schema_version: u32,
    /// `done`, `degraded`, `partial`, `failed`, `cancelled`, or `unsaved`.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub extraction_method: String,
    #[serde(default)]
    pub input_bytes: u64,
    #[serde(default)]
    pub orientation_included_bytes: u64,
    #[serde(default)]
    pub orientation_omitted_bytes: u64,
    #[serde(default)]
    pub recovered_pages: u32,
    #[serde(default)]
    pub recovered_blocks: u32,
    #[serde(default)]
    pub recovered_assets: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extraction_notes: Vec<String>,
    /// Input a step's fan-out limit left unread.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub coverage_notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_steps: Vec<String>,
    /// Capabilities declared on enabled workflow steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_capabilities: Vec<String>,
    /// Capability classes observed in provider telemetry. Absence means
    /// "not observed", not necessarily "unavailable".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_providers: Vec<String>,
    #[serde(default)]
    pub schema_retry_events: u32,
    #[serde(default)]
    pub verified_evidence: u32,
    #[serde(default)]
    pub partially_verified_evidence: u32,
    #[serde(default)]
    pub unverified_evidence: u32,
    /// Evidence the verifier cannot test, such as an outside source or a
    /// citation without a quote. It is reported but does not degrade a run.
    #[serde(default)]
    pub not_checkable_evidence: u32,
    #[serde(default)]
    pub manual_review_items: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
    #[serde(default)]
    pub reproducibility_hash: String,
}

// --- Legacy types (for reading old saved reports) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefereeReport {
    pub pass_name: String,
    pub pass_label: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub steelman: String,
    #[serde(default)]
    pub assessment: String,
    #[serde(default)]
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSynthesis {
    pub overall_assessment: String,
}

// --- Step Failure ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFailure {
    pub step_id: String,
    pub step_label: String,
    /// Execution phase of the failed step. Empty on reports saved before this
    /// field existed and on run-level recovery sentinels.
    #[serde(default)]
    pub phase: String,
    pub error: String,
}

// --- Full Report ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    /// Survey JSON built before the steps ran (raw — any schema the profile's
    /// survey prompt produces). Old saved reports hold the paper schema here,
    /// which loads fine as a Value; use `models::paper_view` for typed access.
    #[serde(default)]
    pub orientation: serde_json::Value,
    /// New unified step outputs.
    #[serde(default)]
    pub step_outputs: Vec<StepOutput>,
    /// Steps that failed during execution (e.g. timeout).
    #[serde(default)]
    pub failed_steps: Vec<StepFailure>,
    /// Typed, public products of this run. Raw step outputs remain immutable
    /// provenance and are never replaced by these normalized views.
    #[serde(default)]
    pub products: RunProducts,
    /// Deterministic extraction/execution/evidence quality metadata.
    #[serde(default)]
    pub quality: ReportQuality,
    /// Legacy: individual referee reports (for reading old saved reports).
    #[serde(default)]
    pub referee_reports: Vec<RefereeReport>,
    /// Legacy: editor synthesis (for reading old saved reports).
    #[serde(default)]
    pub editor: Option<EditorSynthesis>,
    #[serde(default = "today")]
    pub report_date: NaiveDate,
    #[serde(default)]
    pub paper_hash: String,
}

impl PipelineReport {
    /// Get all step outputs, normalizing from legacy format if needed.
    pub fn all_outputs(&self) -> Vec<StepOutput> {
        if !self.step_outputs.is_empty() {
            return self.step_outputs.clone();
        }
        // Convert legacy format
        let mut outputs: Vec<StepOutput> = self
            .referee_reports
            .iter()
            .map(|r| StepOutput {
                step_id: r.pass_name.clone(),
                step_label: r.pass_label.clone(),
                phase: "parallel".to_string(),
                agent: r.agent.clone(),
                raw_text: r.raw_text.clone(),
                ..Default::default()
            })
            .collect();
        if let Some(ref editor) = self.editor {
            outputs.push(StepOutput {
                step_id: "editor_synthesis".to_string(),
                step_label: "Consolidate Issues".to_string(),
                phase: "sequential".to_string(),
                agent: String::new(),
                raw_text: editor.overall_assessment.clone(),
                ..Default::default()
            });
        }
        outputs
    }

    /// Get the final usable output text. A successful sequential step remains
    /// authoritative. If every sequential report-producing step failed, do
    /// not silently promote one parallel specialist analysis to the canonical
    /// report. Parallel-only workflows still fall back to their last output.
    pub fn final_output(&self) -> Option<&str> {
        if !self.step_outputs.is_empty() {
            if !self.products.primary_step_id.is_empty() {
                return self.step_outputs.iter().rev().find_map(|output| {
                    let matches = !output.skipped
                        && !output.raw_text.trim().is_empty()
                        && output.step_id.split('/').next().unwrap_or(&output.step_id)
                            == self.products.primary_step_id;
                    matches.then_some(
                        output
                            .rendered_markdown
                            .as_deref()
                            .unwrap_or(&output.raw_text),
                    )
                });
            }
            if let Some(output) =
                self.step_outputs.iter().rev().find(|s| {
                    s.phase == "sequential" && !s.skipped && !s.raw_text.trim().is_empty()
                })
            {
                return Some(
                    output
                        .rendered_markdown
                        .as_deref()
                        .unwrap_or(&output.raw_text),
                );
            }
            if self
                .failed_steps
                .iter()
                .any(|failure| failure.phase == "sequential")
            {
                return None;
            }
            return self
                .step_outputs
                .iter()
                .rev()
                .find(|s| !s.skipped && !s.raw_text.trim().is_empty())
                .map(|s| s.rendered_markdown.as_deref().unwrap_or(&s.raw_text));
        }
        // Legacy
        self.editor
            .as_ref()
            .filter(|editor| !editor.overall_assessment.trim().is_empty())
            .map(|editor| editor.overall_assessment.as_str())
    }

    pub fn final_output_blocked_by_failed_sequential(&self) -> bool {
        self.final_output().is_none()
            && self
                .failed_steps
                .iter()
                .any(|failure| failure.phase == "sequential")
    }
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}
