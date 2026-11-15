//! Review-owned identities for orientation, expanded steps and captured inputs.
//! Store digests, never expanded secret variables. A missing or unknown record
//! makes an old run readable and rerunnable, but supplies no proof of reuse.
use super::{rerun::files::read_run_file_in, stable_content_fingerprint};
use crate::pipeline::orient;
use crate::pipeline_config::{
    ArtifactSelector, NamedInputArtifactPart, Phase, PipelineConfig, PrimaryArtifactPart,
};
use crate::{runs::RunWriter, settings::Settings};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

const PATH: &str = "context/execution-contract.json";
pub(super) const BASE_BUNDLE: &str = "context/document_base.json";

pub(super) fn save_base_bundle(
    writer: &mut RunWriter,
    bundle: &crate::document_bundle::DocumentBundle,
) -> Result<(), String> {
    writer.add_text(
        BASE_BUNDLE,
        "Document before orientation",
        "document",
        &bundle.to_json_pretty()?,
    )
}

fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    stable_content_fingerprint(value).map_err(|e| format!("Could not identify Review inputs: {e}"))
}

fn provider(
    settings: &Settings,
    id: &str,
    selection: Option<crate::settings::ModelSelection>,
    effort: String,
) -> serde_json::Value {
    serde_json::json!({"provider":id,"transport":settings.model_transport(id),"model":selection.unwrap_or_else(|| settings.model_selection(id)),"effort":effort,
        "endpoint_digest":(id == "local").then(||format!("{:x}",Sha256::digest(settings.local_base_url.as_bytes())))})
}
fn fallback(settings: &Settings) -> Option<serde_json::Value> {
    settings.usage_limit_fallback_agent().map(|id| {
        provider(
            settings,
            id,
            settings.usage_limit_fallback_model_selection(id),
            settings.usage_limit_fallback_effort(id),
        )
    })
}

pub(super) struct OrientationRecipe {
    pub template: String,
    pub schema: Option<serde_json::Value>,
    pub key: String,
    plan: serde_json::Value,
}

impl OrientationRecipe {
    pub fn new(
        config: &PipelineConfig,
        settings: &Settings,
        mode: &str,
        text: &str,
        source_record: Option<&str>,
        note: Option<&str>,
        quality_notes: &[String],
    ) -> Result<Self, String> {
        let template = match orient::resolve_survey_template(&config.orientation_prompt, mode) {
            Some(template) => template,
            None => crate::prompts::load_prompt("orientation").map_err(|e| e.to_string())?,
        };
        let schema = orient::resolve_survey_schema(
            &config.orientation_prompt,
            config.orientation_schema.as_ref(),
            mode,
        );
        let resolved = schema
            .as_ref()
            .map(crate::auto_review::resolve_schema_catalogs)
            .transpose()?;
        let expanded = if crate::auto_review::uses_auto_review_contract(config) {
            crate::auto_review::expand_orientation_prompt(&template)
        } else {
            template.clone()
        };
        let agent = settings.orientation_agent();
        let plan = serde_json::json!({
            "template": expanded,
            "schema": resolved,
            "model": provider(settings, agent, settings.orientation_model_selection(agent), settings.orientation_effort(agent)),
            "backup": fallback(settings),
            "mode": mode,
            "document_digest": format!("{:x}",Sha256::digest(text.as_bytes())),
            "source_record_digest": source_record.map(|r|format!("{:x}",Sha256::digest(r.as_bytes()))),
        });
        let key = digest(&(env!("CARGO_PKG_VERSION"), &plan, note, quality_notes))?;
        Ok(Self {
            template,
            schema,
            key,
            plan,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct ExecutionContract {
    version: u8,
    orientation: String,
    orientation_response: String,
    pub orientation_notes: Vec<String>,
    steps: BTreeMap<String, Option<String>>,
    // The resolved materialization is for inspection. Resume compares signatures
    // and executes a freshly validated plan, never deserializes executable rules here.
    #[serde(default, skip_deserializing)]
    plan: serde_json::Value,
    reused_from: BTreeMap<String, String>,
}

impl ExecutionContract {
    pub fn load(dir: &Path) -> Option<Self> {
        serde_json::from_str::<Self>(&read_run_file_in(dir, PATH).ok()?)
            .ok()
            .filter(|c| c.version == 1)
    }
    pub fn matches_orientation(
        &self,
        key: &str,
        value: &serde_json::Value,
    ) -> Result<bool, String> {
        Ok(self.orientation == key && self.orientation_response == digest(value)?)
    }
    pub fn changed_steps(&self, parent: Option<&Self>) -> HashSet<String> {
        self.steps
            .iter()
            .filter(|(id, key)| key.is_none() || parent.and_then(|p| p.steps.get(*id)) != Some(key))
            .map(|(id, _)| id.clone())
            .collect()
    }
    pub fn save(
        &mut self,
        writer: &mut RunWriter,
        reused: impl Iterator<Item = (String, String)>,
    ) -> Result<(), String> {
        self.reused_from.extend(reused);
        writer.add_text(
            PATH,
            "Effective workflow and reused steps",
            "context",
            &serde_json::to_string_pretty(self).map_err(|e| e.to_string())?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build(
        config: &PipelineConfig,
        settings: &Settings,
        orientation: &OrientationRecipe,
        response: &serde_json::Value,
        orientation_notes: Vec<String>,
        note: Option<&str>,
        variables: &HashMap<String, String>,
        named: &HashMap<String, String>,
        writer: &RunWriter,
    ) -> Result<Self, String> {
        let mut steps = BTreeMap::new();
        let mut effective_steps = Vec::new();
        let response_key = digest(response)?;
        let source_record = writer.artifact_digest("context/source-snapshot.json");
        let document = writer.artifact_digest(crate::runs::DOCUMENT_TEXT_PATH);
        let bundle = writer.artifact_digest("context/document_bundle.json");
        for step in config.steps.iter().filter(|s| s.enabled) {
            let mut inputs = Vec::new();
            for selector in &step.context.include {
                match selector {
                    ArtifactSelector::Primary { parts } => {
                        for part in parts {
                            match part {
                                PrimaryArtifactPart::Source => inputs.push(source_record),
                                PrimaryArtifactPart::Text => {
                                    inputs.push(document);
                                    inputs.push(bundle);
                                }
                                PrimaryArtifactPart::Structure | PrimaryArtifactPart::Visuals => {
                                    inputs.push(bundle)
                                }
                            }
                        }
                    }
                    ArtifactSelector::NamedInput { key, parts } => {
                        if parts.contains(&NamedInputArtifactPart::Text) {
                            inputs
                                .push(named.get(key).and_then(|path| writer.artifact_digest(path)));
                        }
                        if parts.contains(&NamedInputArtifactPart::Source) {
                            inputs.push(writer.artifact_digest(&format!(
                                "named-sources/{}/context/source-snapshot.json",
                                crate::pipeline::executor::step_slug(key)
                            )));
                        }
                    }
                    ArtifactSelector::Survey => inputs.push(Some(&response_key)),
                    ArtifactSelector::Step { .. } => {} // changed producers invalidate their transitive consumers
                }
            }
            if step.kind == crate::pipeline_config::StepKind::Analyzer
                || step.for_each.as_ref().is_some_and(|f| f.artifact.is_none())
            {
                inputs.extend([document, bundle, source_record]);
            }
            let model = step.kind.is_model();
            let parallel = (model && step.phase == Phase::Parallel)
                .then_some(&config.parallel_context_template);
            let mut bindings = BTreeMap::new();
            let mut has_secret = false;
            for var in &config.variables {
                let consumes = |text: &str| {
                    text.split("{var:").skip(1).any(|tail| {
                        tail.split_once('}')
                            .is_some_and(|(key, _)| key.trim() == var.key)
                    })
                };
                if consumes(&step.prompt)
                    || consumes(&step.system_prompt)
                    || parallel.is_some_and(|t| consumes(t))
                {
                    if var.secret {
                        has_secret = true;
                    } else {
                        bindings.insert(&var.key, variables.get(&var.key));
                    }
                }
            }
            let agents: Vec<_> = step
                .agents
                .iter()
                .map(|id| {
                    provider(
                        settings,
                        id,
                        step.model_selection_for(settings, id),
                        step.effort_for(settings, id),
                    )
                })
                .collect();
            let merge = (step.agents.len() > 1 && config.merge.enabled).then(|| {
                let (agent, selection, effort) = config.merge.call_policy(settings);
                (
                    &config.merge,
                    provider(settings, &agent, Some(selection), effort),
                )
            });
            let schema = step
                .output_schema
                .as_ref()
                .map(crate::pipeline::structured::resolve_schema_reference)
                .transpose()?;
            let effective = serde_json::json!({"step": step,"resolved_schema":schema.as_ref().filter(|schema|Some(*schema)!=step.output_schema.as_ref()),
                "agents":agents,"merge":merge,"selected_input_digests":inputs,"uses_reentered_secret":has_secret});
            let key = if has_secret {
                None
            } else {
                Some(digest(&(
                    env!("CARGO_PKG_VERSION"),
                    &effective,
                    parallel,
                    bindings,
                    model.then_some(note),
                    model.then(|| fallback(settings)),
                    model && config.context_cache.enabled,
                ))?)
            };
            effective_steps.push(effective);
            steps.insert(step.id.clone(), key);
        }
        Ok(Self {
            version: 1,
            orientation: orientation.key.clone(),
            orientation_response: response_key,
            orientation_notes,
            steps,
            plan: serde_json::json!({
                "orientation":orientation.plan,
                "parallel_context_template":config.parallel_context_template,
                "steps":effective_steps
            }),
            reused_from: BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_config::{AnalyzerSpec, StepConfig, StepKind};

    #[test]
    fn changed_instructions_invalidate_only_their_consumers() {
        let root = tempfile::tempdir().unwrap();
        let mut writer = RunWriter::create_in(root.path(), "original").unwrap();
        let settings = Settings::default();
        let mut workflow = crate::pipeline_config::workflow_template().unwrap().config;
        workflow.orientation_prompt = "Identify the central claims: {input_text}".into();
        workflow.steps = vec![
            StepConfig {
                id: "check".into(),
                kind: StepKind::Analyzer,
                enabled: true,
                analyzer: Some(AnalyzerSpec {
                    id: "reference_audit".into(),
                    version: 1,
                    options: serde_json::json!({"network":false}),
                }),
                ..Default::default()
            },
            StepConfig {
                id: "summary".into(),
                enabled: true,
                after: vec!["check".into()],
                ..Default::default()
            },
            StepConfig {
                id: "separate".into(),
                enabled: true,
                prompt: "Assess figures".into(),
                ..Default::default()
            },
        ];
        let recipe = |config: &PipelineConfig| {
            OrientationRecipe::new(config, &settings, "document", "draft", None, None, &[]).unwrap()
        };
        let response = serde_json::json!({"topic":"trade"});
        let inputs = HashMap::new();
        let build = |cfg: &PipelineConfig, w: &RunWriter| {
            ExecutionContract::build(
                cfg,
                &settings,
                &recipe(cfg),
                &response,
                vec![],
                None,
                &inputs,
                &inputs,
                w,
            )
            .unwrap()
        };
        let mut old = build(&workflow, &writer);
        old.save(&mut writer, std::iter::empty()).unwrap();
        let parent = ExecutionContract::load(writer.dir()).unwrap();
        assert!(parent
            .matches_orientation(&recipe(&workflow).key, &response)
            .unwrap());
        workflow.steps[0].analyzer.as_mut().unwrap().options = serde_json::json!({"network":true});
        let new = build(&workflow, &writer);
        let changed = new.changed_steps(Some(&parent));
        assert_eq!(changed, HashSet::from(["check".to_string()]));
        assert_eq!(
            crate::pipeline::executor::dependents_of(&workflow, &changed),
            HashSet::from(["summary".to_string()])
        );
        assert_eq!(new.changed_steps(None).len(), 3); // older run
        workflow.orientation_prompt = "Prioritize identification: {input_text}".into();
        assert!(!parent
            .matches_orientation(&recipe(&workflow).key, &response)
            .unwrap());
        assert!(!parent
            .matches_orientation(&parent.orientation, &serde_json::json!({}))
            .unwrap());
    }

    #[test]
    fn a_reentered_secret_never_proves_reuse() {
        let root = tempfile::tempdir().unwrap();
        let writer = RunWriter::create_in(root.path(), "secret").unwrap();
        let mut config = crate::pipeline_config::workflow_template().unwrap().config;
        config.variables = vec![serde_json::from_value(
            serde_json::json!({"key":"key","label":"Key","kind":"text","secret":true}),
        )
        .unwrap()];
        config.steps[0].prompt = "Use {var: key }".into();
        let s = Settings::default();
        let recipe =
            OrientationRecipe::new(&config, &s, "document", "draft", None, None, &[]).unwrap();
        let secret = HashMap::from([("key".into(), "secret-value".into())]);
        let c = ExecutionContract::build(
            &config,
            &s,
            &recipe,
            &serde_json::json!({}),
            vec![],
            None,
            &secret,
            &HashMap::new(),
            &writer,
        )
        .unwrap();
        assert!(c.changed_steps(Some(&c)).contains(&config.steps[0].id));
        assert!(!serde_json::to_string(&c).unwrap().contains("secret-value"));
    }
}
