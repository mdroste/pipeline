//! Declared variables, named inputs, extraction settings, and published products.

use serde::{Deserialize, Serialize};

/// A run-time variable a profile declares. When present, the Run action
/// prompts for values (pre-filled with `default`) and steps reference them as
/// `{var:key}`. Turns a profile into a reusable template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarSpec {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// "text" | "choice" | "file". Advisory — the value is always a string.
    #[serde(default = "default_var_kind")]
    pub kind: String,
    #[serde(default)]
    pub default: String,
    /// Options for `kind == "choice"`.
    #[serde(default)]
    pub choices: Vec<String>,
    /// A required variable must resolve to a non-empty supplied/default value
    /// before any extraction or provider work begins.
    #[serde(default)]
    pub required: bool,
    /// Secret values are accepted at runtime but must not be written verbatim
    /// to durable manifests or user-facing previews.
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub validation: VarValidation,
}

impl Default for VarSpec {
    fn default() -> Self {
        Self {
            key: String::new(),
            label: String::new(),
            kind: default_var_kind(),
            default: String::new(),
            choices: Vec::new(),
            required: false,
            secret: false,
            validation: VarValidation::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VarValidation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pattern: String,
}

fn default_var_kind() -> String {
    "text".to_string()
}

/// An additional named input a profile accepts beyond the primary one. Its
/// extracted text is exposed to prompts as `{input:key}` (a path to Read).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSlot {
    pub key: String,
    #[serde(default)]
    pub label: String,
    /// "document" (single file) or "folder". Defaults to document.
    #[serde(default = "default_slot_mode")]
    pub mode: String,
    #[serde(default)]
    pub required: bool,
    /// Lowercase extensions without a leading dot. Empty accepts Pipeline's
    /// normal document set for document inputs.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Optional accepted MIME declarations used for compatibility/preflight.
    #[serde(default)]
    pub mime_types: Vec<String>,
    /// Zero uses Pipeline's global input limit.
    #[serde(default)]
    pub max_bytes: u64,
    /// `public`, `internal`, `confidential`, or `secret` (empty = internal).
    #[serde(default)]
    pub sensitivity: String,
}

impl Default for InputSlot {
    fn default() -> Self {
        Self {
            key: String::new(),
            label: String::new(),
            mode: default_slot_mode(),
            required: false,
            extensions: Vec::new(),
            mime_types: Vec::new(),
            max_bytes: 0,
            sensitivity: String::new(),
        }
    }
}

fn default_slot_mode() -> String {
    "document".to_string()
}

/// Per-profile extraction configuration. Workflows may choose a method, but
/// parser-specific tuning is centralized in global Settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// "auto" | "llm" | "paddleocr-vl-full" | "pdftotext" | "" (= inherit
    /// global). The retired "paddleocr-vl" value is migrated to the Full
    /// Parser when older profiles are deserialized.
    #[serde(default, deserialize_with = "deserialize_extraction_method")]
    pub method: String,
    /// Input mode for the workflow: "" or "document" (single file, default),
    /// "folder" (inventory of a directory; steps Read files on demand), or
    /// "none" (runs from the prompts alone).
    #[serde(default)]
    pub input_mode: String,
    /// Extra named inputs (beyond the primary one) this profile accepts.
    #[serde(default)]
    pub extra_inputs: Vec<InputSlot>,
}

fn deserialize_extraction_method<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(match value.as_str() {
        "paddleocr-vl" => "paddleocr-vl-full".to_string(),
        _ => value,
    })
}

/// Explicit public products selected from a workflow's step graph. Empty
/// fields preserve legacy inference for profiles saved before this contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub primary_step: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub findings_step: String,
    /// Additional typed products published by the workflow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named: Vec<NamedProductSpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedProductSpec {
    pub key: String,
    pub step: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub media_type: String,
    /// `text`, `markdown`, `json`, or `artifact`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub viewer: String,
    /// `full`, `redacted`, or `disabled`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub export_policy: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sensitivity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}
