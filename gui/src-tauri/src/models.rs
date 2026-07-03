use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// --- Enums ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PaperType {
    Theory,
    Empirical,
    Mixed,
}

impl std::fmt::Display for PaperType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaperType::Theory => write!(f, "theory"),
            PaperType::Empirical => write!(f, "empirical"),
            PaperType::Mixed => write!(f, "mixed"),
        }
    }
}

// --- Orientation Map ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperMetadata {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default = "default_paper_type")]
    pub paper_type: PaperType,
    #[serde(default)]
    pub page_count: Option<u32>,
    #[serde(default)]
    pub has_appendix: bool,
    #[serde(default)]
    pub has_online_appendix: bool,
}

impl Default for PaperMetadata {
    fn default() -> Self {
        Self {
            title: String::new(),
            authors: Vec::new(),
            date: None,
            paper_type: default_paper_type(),
            page_count: None,
            has_appendix: false,
            has_online_appendix: false,
        }
    }
}

fn default_paper_type() -> PaperType {
    PaperType::Mixed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionEntry {
    pub number: String,
    pub title: String,
    #[serde(default)]
    pub page_start: Option<u32>,
    #[serde(default)]
    pub page_end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormalResult {
    pub kind: String,
    pub number: String,
    #[serde(default)]
    pub page: Option<u32>,
    pub summary: String,
    #[serde(default)]
    pub proof_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableFigure {
    pub kind: String,
    pub number: String,
    #[serde(default)]
    pub page: Option<u32>,
    pub caption_summary: String,
    pub what_it_shows: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotationEntry {
    pub symbol: String,
    pub definition: String,
    #[serde(default)]
    pub page_introduced: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionQualityNote {
    pub page_range: String,
    pub description: String,
}

/// Typed *paper-review view* of a survey JSON. The survey itself is stored as
/// raw `serde_json::Value` (any schema a profile's survey prompt produces);
/// this struct is only how the built-in paper profiles interpret it for
/// `{paper_type}`, report headers, and pretty rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrientationMap {
    /// Defaulted so loosely-shaped survey JSON from non-paper workflows
    /// still validates; the typed fields simply stay empty.
    #[serde(default)]
    pub metadata: PaperMetadata,
    #[serde(default)]
    pub sections: Vec<SectionEntry>,
    #[serde(default)]
    pub formal_results: Vec<FormalResult>,
    #[serde(default)]
    pub tables_figures: Vec<TableFigure>,
    #[serde(default)]
    pub notation: Vec<NotationEntry>,
    #[serde(default)]
    pub stated_contribution: String,
    #[serde(default)]
    pub key_references: Vec<String>,
    #[serde(default)]
    pub extraction_quality_notes: Vec<ExtractionQualityNote>,
}

impl OrientationMap {
    /// Minimal placeholder when orientation is disabled.
    pub fn empty(paper_text: &str) -> Self {
        // Guess paper type from text heuristics
        let lower = paper_text.to_lowercase();
        let has_regression = lower.contains("regression") || lower.contains("standard error") || lower.contains("coefficient");
        let has_theorem = lower.contains("theorem") || lower.contains("proof") || lower.contains("proposition");
        let paper_type = if has_theorem && !has_regression {
            PaperType::Theory
        } else if has_regression && !has_theorem {
            PaperType::Empirical
        } else {
            PaperType::Mixed
        };

        Self {
            metadata: PaperMetadata {
                title: String::new(),
                authors: vec![],
                date: None,
                paper_type,
                page_count: None,
                has_appendix: false,
                has_online_appendix: false,
            },
            sections: vec![],
            formal_results: vec![],
            tables_figures: vec![],
            notation: vec![],
            stated_contribution: String::new(),
            key_references: vec![],
            extraction_quality_notes: vec![],
        }
    }
}

/// Interpret a survey JSON as a paper orientation map, if it has that shape.
/// Returns `None` for surveys produced by custom (non-paper) survey prompts,
/// so callers fall back to schema-agnostic handling. Detection is by the
/// paper schema's distinctive keys rather than parse success, because every
/// field of `OrientationMap` is defaulted and any JSON object would "parse".
pub fn paper_view(survey: &serde_json::Value) -> Option<OrientationMap> {
    let is_paper_shaped = survey
        .get("metadata")
        .map(|m| m.get("paper_type").is_some())
        .unwrap_or(false)
        || survey.get("formal_results").is_some()
        || survey.get("stated_contribution").is_some();
    if !is_paper_shaped {
        return None;
    }
    serde_json::from_value(survey.clone()).ok()
}

// --- Step Output ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutput {
    pub step_id: String,
    pub step_label: String,
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub raw_text: String,
}

fn default_phase() -> String {
    "parallel".to_string()
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
            })
            .collect();
        if let Some(ref editor) = self.editor {
            outputs.push(StepOutput {
                step_id: "editor_synthesis".to_string(),
                step_label: "Consolidate Issues".to_string(),
                phase: "sequential".to_string(),
                agent: String::new(),
                raw_text: editor.overall_assessment.clone(),
            });
        }
        outputs
    }

    /// Get the final output text (last sequential step, or last step if none sequential).
    pub fn final_output(&self) -> Option<&str> {
        if !self.step_outputs.is_empty() {
            return self
                .step_outputs
                .iter()
                .rev()
                .find(|s| s.phase == "sequential")
                .or_else(|| self.step_outputs.last())
                .map(|s| s.raw_text.as_str());
        }
        // Legacy
        self.editor
            .as_ref()
            .map(|e| e.overall_assessment.as_str())
    }
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

// --- Extraction Result ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub text: String,
    pub method: String,
    pub source_path: String,
    pub paper_hash: String,
    #[serde(default)]
    pub quality_notes: Vec<String>,
}

// --- Pipeline Progress Events ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageEvent {
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassEvent {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub message: String,
}

// --- Report History ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummary {
    pub paper_hash: String,
    pub title: String,
    pub report_date: NaiveDate,
    pub file_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── OrientationMap::empty ──────────────────────────────────────

    fn empty_orientation() -> OrientationMap {
        OrientationMap {
            metadata: PaperMetadata {
                title: String::new(),
                authors: vec![],
                date: None,
                paper_type: PaperType::Mixed,
                page_count: None,
                has_appendix: false,
                has_online_appendix: false,
            },
            sections: vec![],
            formal_results: vec![],
            tables_figures: vec![],
            notation: vec![],
            stated_contribution: String::new(),
            key_references: vec![],
            extraction_quality_notes: vec![],
        }
    }

    #[test]
    fn empty_detects_theory() {
        let text = "We prove Theorem 1. The proof proceeds by induction. Proposition 2 follows.";
        let omap = OrientationMap::empty(text);
        assert_eq!(omap.metadata.paper_type, PaperType::Theory);
    }

    #[test]
    fn empty_detects_empirical() {
        let text = "Table 3 shows our regression results. The standard error is clustered. The coefficient is significant.";
        let omap = OrientationMap::empty(text);
        assert_eq!(omap.metadata.paper_type, PaperType::Empirical);
    }

    #[test]
    fn empty_detects_mixed() {
        let text = "We prove Theorem 1 and then run a regression to estimate the coefficient.";
        let omap = OrientationMap::empty(text);
        assert_eq!(omap.metadata.paper_type, PaperType::Mixed);
    }

    #[test]
    fn empty_defaults_to_mixed_for_short_text() {
        let omap = OrientationMap::empty("A short abstract.");
        assert_eq!(omap.metadata.paper_type, PaperType::Mixed);
    }

    // ── paper_view ─────────────────────────────────────────────────

    #[test]
    fn paper_view_accepts_paper_shaped_survey() {
        let survey = serde_json::json!({
            "metadata": {"title": "T", "paper_type": "theory"},
            "sections": [{"number": "1", "title": "Intro"}]
        });
        let view = paper_view(&survey).expect("paper-shaped survey should parse");
        assert_eq!(view.metadata.paper_type, PaperType::Theory);
        assert_eq!(view.sections.len(), 1);
    }

    #[test]
    fn paper_view_accepts_serialized_empty_orientation() {
        let survey = serde_json::to_value(OrientationMap::empty("theorem proof")).unwrap();
        let view = paper_view(&survey).expect("placeholder orientation is paper-shaped");
        assert_eq!(view.metadata.paper_type, PaperType::Theory);
    }

    #[test]
    fn paper_view_rejects_custom_survey() {
        let survey = serde_json::json!({
            "overview": "a codebase",
            "key_elements": [{"name": "main.rs"}]
        });
        assert!(paper_view(&survey).is_none());
    }

    #[test]
    fn paper_view_rejects_null() {
        assert!(paper_view(&serde_json::Value::Null).is_none());
    }

    // ── PipelineReport::all_outputs ───────────────────────────────────

    fn make_report(step_outputs: Vec<StepOutput>, referees: Vec<RefereeReport>, editor: Option<EditorSynthesis>) -> PipelineReport {
        PipelineReport {
            orientation: serde_json::to_value(empty_orientation()).unwrap(),
            step_outputs,
            failed_steps: vec![],
            referee_reports: referees,
            editor,
            report_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            paper_hash: "abc123".into(),
        }
    }

    #[test]
    fn all_outputs_returns_step_outputs_when_present() {
        let outputs = vec![StepOutput {
            step_id: "s1".into(),
            step_label: "Step 1".into(),
            phase: "parallel".into(),
            agent: "claude".into(),
            raw_text: "content".into(),
        }];
        let report = make_report(outputs.clone(), vec![], None);
        assert_eq!(report.all_outputs().len(), 1);
        assert_eq!(report.all_outputs()[0].step_id, "s1");
    }

    #[test]
    fn all_outputs_converts_legacy_referees() {
        let referees = vec![RefereeReport {
            pass_name: "r1".into(),
            pass_label: "Referee 1".into(),
            agent: "claude".into(),
            steelman: String::new(),
            assessment: String::new(),
            raw_text: "referee content".into(),
        }];
        let report = make_report(vec![], referees, None);
        let outputs = report.all_outputs();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].step_id, "r1");
        assert_eq!(outputs[0].phase, "parallel");
    }

    #[test]
    fn all_outputs_converts_legacy_editor() {
        let editor = EditorSynthesis {
            overall_assessment: "synthesis text".into(),
        };
        let report = make_report(vec![], vec![], Some(editor));
        let outputs = report.all_outputs();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].step_id, "editor_synthesis");
        assert_eq!(outputs[0].phase, "sequential");
        assert_eq!(outputs[0].raw_text, "synthesis text");
    }

    // ── PipelineReport::final_output ──────────────────────────────────

    #[test]
    fn final_output_prefers_sequential() {
        let outputs = vec![
            StepOutput { step_id: "s1".into(), step_label: "P1".into(), phase: "parallel".into(), agent: String::new(), raw_text: "parallel text".into() },
            StepOutput { step_id: "s2".into(), step_label: "Seq".into(), phase: "sequential".into(), agent: String::new(), raw_text: "sequential text".into() },
        ];
        let report = make_report(outputs, vec![], None);
        assert_eq!(report.final_output(), Some("sequential text"));
    }

    #[test]
    fn final_output_falls_back_to_last() {
        let outputs = vec![
            StepOutput { step_id: "s1".into(), step_label: "P1".into(), phase: "parallel".into(), agent: String::new(), raw_text: "only parallel".into() },
        ];
        let report = make_report(outputs, vec![], None);
        assert_eq!(report.final_output(), Some("only parallel"));
    }

    #[test]
    fn final_output_legacy_editor() {
        let editor = EditorSynthesis { overall_assessment: "legacy synthesis".into() };
        let report = make_report(vec![], vec![], Some(editor));
        assert_eq!(report.final_output(), Some("legacy synthesis"));
    }

    #[test]
    fn final_output_empty_report() {
        let report = make_report(vec![], vec![], None);
        assert_eq!(report.final_output(), None);
    }

    // ── PaperType Display ──────────────────────────────────────────

    #[test]
    fn paper_type_display() {
        assert_eq!(PaperType::Theory.to_string(), "theory");
        assert_eq!(PaperType::Empirical.to_string(), "empirical");
        assert_eq!(PaperType::Mixed.to_string(), "mixed");
    }
}
