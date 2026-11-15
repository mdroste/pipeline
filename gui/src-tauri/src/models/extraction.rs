use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub text: String,
    pub method: String,
    pub source_path: String,
    pub paper_hash: String,
    #[serde(default)]
    pub quality_notes: Vec<String>,
}
