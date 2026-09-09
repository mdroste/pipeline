use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HarnessModule {
    pub id: &'static str,
    pub kind: &'static str,
    pub version: i64,
    pub name: &'static str,
    pub description: &'static str,
    pub requires_workspace: bool,
    pub capability: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HarnessPreset {
    pub id: String,
    pub workspace_id: Option<String>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_instructions: Option<String>,
    pub modules: Vec<String>,
    pub built_in: bool,
    pub source_preset_id: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveHarness {
    pub schema_version: i64,
    pub session_id: String,
    pub workspace_id: Option<String>,
    pub preset: HarnessPreset,
    pub mode: String,
    pub web_search: bool,
    pub command_network: bool,
    pub permission_profile: String,
    pub context_budget_bytes: usize,
    pub enabled_modules: Vec<String>,
    pub unavailable_modules: Vec<String>,
    pub diagnostics: Vec<String>,
    pub developer_instructions: String,
    pub context_preview: String,
    pub context_truncated: bool,
    pub dynamic_tools: Vec<Value>,
    pub fingerprint: String,
    pub value_sources: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_availability: Vec<ModuleAvailability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instruction_sections: Vec<InstructionSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleAvailability {
    pub id: String,
    pub available: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstructionSection {
    pub id: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct PreparedHarness {
    pub effective: EffectiveHarness,
    pub config_snapshot_id: String,
    pub context_snapshot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    pub scope_key: String,
    pub workspace_id: Option<String>,
    pub schema_version: i64,
    pub body: Value,
    pub revision: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWorkspaceConfigRequest {
    pub workspace_id: Option<String>,
    pub expected_revision: Option<i64>,
    pub body: Value,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonePresetRequest {
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub source_workspace_id: Option<String>,
    pub source_preset_id: String,
    pub name: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum BasePromptUpdate {
    CodexDefault,
    Replace { text: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePresetRequest {
    pub preset_id: String,
    pub expected_revision: i64,
    #[serde(default)]
    pub base_prompt: Option<BasePromptUpdate>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub modules: Vec<String>,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessCatalog {
    pub schema_version: i64,
    pub tool_catalog_version: i64,
    pub modules: Vec<HarnessModule>,
    pub presets: Vec<HarnessPreset>,
    pub prompt_layers: Vec<InstructionSection>,
}
