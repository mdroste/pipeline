use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingEvidence {
    /// `document`, `source`, `external`, or `call`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence_type: String,
    /// Host-authored status. Model-produced v2 artifacts may only claim
    /// `unverified`; later evidence verification can promote the record.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub verification_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact_path: String,
    /// Root-relative path in a selected source tree. Unlike `artifact_path`,
    /// this is the human-facing locator; completed runs best-effort snapshot
    /// cited files and add the corresponding `artifact_path` separately.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_path: String,
    /// Optional content fingerprint supplied by a deterministic producer.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doi: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub accessed_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub query_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub call_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub quote: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    /// Canonical 1-based global rank. Display grouping never changes it.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub rank: u32,
    pub id: String,
    /// Stable producer key when a workflow has one (rule id, check id, etc.).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_key: String,
    /// Report ids of the reviewer analyses that support this finding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Stable reviewer/producer ids contributing to this finding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewer_ids: Vec<String>,
    /// Durable call identifiers supporting the finding when available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_call_ids: Vec<String>,
    pub title: String,
    /// Input-neutral grouping used as a report section or source location.
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "section")]
    pub category: String,
    /// Optional high/medium/low priority. Array order remains authoritative.
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "severity")]
    pub priority: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confidence: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub verification_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub problem: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub consequence: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recommended_action: String,
    pub body: String,
    #[serde(default)]
    pub evidence: Vec<FindingEvidence>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FindingSet {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub source_step_id: String,
    #[serde(default)]
    pub source_step_label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxonomy: Vec<String>,
    #[serde(default)]
    pub findings: Vec<Finding>,
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationDisposition {
    pub finding_id: String,
    pub disposition: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub merged_into: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunProducts {
    #[serde(default)]
    pub schema_version: u32,
    /// Step whose terminal response is the human-facing primary result.
    #[serde(default)]
    pub primary_step_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings: Option<FindingSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_dispositions: Vec<ValidationDisposition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named: Vec<NamedRunProduct>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedRunProduct {
    pub key: String,
    pub source_step_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub media_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub viewer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub export_policy: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sensitivity: String,
    pub content: serde_json::Value,
}
