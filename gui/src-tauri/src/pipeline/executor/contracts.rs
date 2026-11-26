//! Named immutable run inputs. No scheduling or provider state.
use crate::models::StepOutput;
use crate::pipeline_config::PipelineConfig;
/// Immutable inputs for one execution, shared by fresh runs and Resume.
pub struct ExecutionRequest<'a> {
    pub app: &'a crate::emit::EventBus,
    pub config: &'a PipelineConfig,
    pub variables: &'a std::collections::HashMap<String, String>,
    pub extra_inputs: &'a std::collections::HashMap<String, String>,
    pub extra_input_sources: &'a std::collections::HashMap<String, String>,
    pub preloaded: &'a std::collections::HashMap<String, Vec<StepOutput>>,
    pub write_dir: Option<&'a str>,
    pub settings: &'a crate::settings::Settings,
    pub author_note: Option<&'a str>,
    pub sources: ExecutionSources<'a>,
}
pub struct ExecutionSources<'a> {
    pub orientation_path: &'a str,
    pub orientation_value: &'a serde_json::Value,
    pub paper_text_path: &'a str,
    pub document_bundle_path: &'a str,
    pub source_path: &'a str,
    pub paper_type: &'a str,
    pub survey_hint: &'a str,
}
