use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewSelectionNote {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

/// Optional adaptive-review routing data produced by Paper Review (Auto).
/// It remains part of the raw orientation artifact; this typed view exists for
/// human-facing rendering, provenance, and host-owned workflow materialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewPlan {
    #[serde(default)]
    pub primary_domain: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub paper_forms: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub subject_specialist_ids: Vec<String>,
    #[serde(default)]
    pub method_specialist_ids: Vec<String>,
    #[serde(default)]
    pub selection_notes: Vec<ReviewSelectionNote>,
    #[serde(default)]
    pub routing_uncertainty: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_plan: Option<ReviewPlan>,
}

impl OrientationMap {
    /// Minimal compatibility placeholder for older saved reports without an orientation map.
    pub fn empty(paper_text: &str) -> Self {
        // Guess paper type from text heuristics
        let lower = paper_text.to_lowercase();
        let has_regression = lower.contains("regression")
            || lower.contains("standard error")
            || lower.contains("coefficient");
        let has_theorem =
            lower.contains("theorem") || lower.contains("proof") || lower.contains("proposition");
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
            review_plan: None,
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

/// One-line instruction telling a step what to look for in the survey JSON.
/// Paper-shaped surveys keep the referee vocabulary; any other survey gets a
/// neutral sentence listing its actual top-level keys, so steps learn what
/// the map offers without the engine assuming a schema.
pub fn survey_hint(survey: &serde_json::Value) -> String {
    if paper_view(survey).is_some() {
        return "Read it for the paper's structure, sections, formal results, tables, figures, and notation.".to_string();
    }
    let keys: Vec<&str> = survey
        .as_object()
        .map(|o| o.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    if keys.is_empty() {
        "Read it for a structured map of the input before you begin.".to_string()
    } else {
        format!(
            "Read it for a structured map of the input; it covers: {}.",
            keys.join(", ")
        )
    }
}
