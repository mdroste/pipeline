//! Portable workflow input, validation, inspection, and installation.

use super::args::WorkflowArgs;
use super::io::emit_stdout;
use pipeline_gui_lib::{commands, pipeline_config};
use std::io::Read as _;
use std::path::{Path, PathBuf};

const MAX_WORKFLOW_BYTES: usize = 10_000_000;

fn read_workflow_source(source: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    if source == "-" {
        std::io::stdin()
            .lock()
            .take(MAX_WORKFLOW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read workflow from stdin: {error}"))?;
    } else {
        let file = pipeline_gui_lib::safety::open_regular_file(Path::new(source))
            .map_err(|error| format!("Failed to read workflow '{source}': {error}"))?;
        file.take(MAX_WORKFLOW_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read workflow '{source}': {error}"))?;
    }
    if bytes.len() > MAX_WORKFLOW_BYTES {
        return Err("Workflow exceeds the 10 MB safety limit".to_string());
    }
    String::from_utf8(bytes).map_err(|error| format!("Workflow is not valid UTF-8: {error}"))
}

pub(super) fn load_cli_workflow(source: &str) -> Result<commands::HeadlessWorkflow, String> {
    let content = read_workflow_source(source)?;
    let document = pipeline_config::parse_workflow_document_strict(&content)?;
    let source = if source == "-" {
        "stdin".to_string()
    } else {
        Path::new(source)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(source))
            .to_string_lossy()
            .to_string()
    };
    Ok(commands::HeadlessWorkflow { source, document })
}

fn workflow_validation_summary(workflow: &commands::HeadlessWorkflow) -> serde_json::Value {
    let agents = workflow
        .document
        .config
        .steps
        .iter()
        .flat_map(|step| step.agents.iter())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    serde_json::json!({
        "valid": true,
        "schema_version": workflow.document.schema_version,
        "name": workflow.document.name,
        "fingerprint": workflow.document.fingerprint,
        "step_count": workflow.document.config.steps.len(),
        "agents": agents,
        "source": workflow.source,
    })
}

pub(super) fn cmd_workflow(args: WorkflowArgs) -> i32 {
    match args {
        WorkflowArgs::Schema => {
            match serde_json::to_string_pretty(&pipeline_config::workflow_json_schema()) {
                Ok(schema) => {
                    emit_stdout(&schema);
                    0
                }
                Err(error) => {
                    eprintln!("Error: failed to encode workflow schema: {error}");
                    1
                }
            }
        }
        WorkflowArgs::Template => match pipeline_config::workflow_template() {
            Ok(template) => {
                print!("{}", template.canonical_json);
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
        WorkflowArgs::Validate { source, json } => {
            let workflow = match load_cli_workflow(&source) {
                Ok(workflow) => workflow,
                Err(error) => {
                    if json {
                        emit_stdout(
                            &serde_json::to_string_pretty(&serde_json::json!({
                                "valid": false,
                                "error": error,
                            }))
                            .expect("workflow validation error is serializable"),
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            let summary = workflow_validation_summary(&workflow);
            if json {
                emit_stdout(
                    &serde_json::to_string_pretty(&summary)
                        .expect("workflow validation summary is serializable"),
                );
            } else {
                emit_stdout(&format!(
                    "Valid workflow '{}' ({} steps, {})",
                    workflow.document.name,
                    workflow.document.config.steps.len(),
                    workflow.document.fingerprint
                ));
            }
            0
        }
        WorkflowArgs::Install { source, json } => {
            let workflow = match load_cli_workflow(&source) {
                Ok(workflow) => workflow,
                Err(error) => {
                    if json {
                        emit_stdout(
                            &serde_json::to_string_pretty(&serde_json::json!({
                                "installed": false,
                                "error": error,
                            }))
                            .expect("workflow install error is serializable"),
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            let installed = match pipeline_config::install_workflow_document(&workflow.document) {
                Ok(installed) => installed,
                Err(error) => {
                    if json {
                        emit_stdout(
                            &serde_json::to_string_pretty(&serde_json::json!({
                                "installed": false,
                                "error": error,
                            }))
                            .expect("workflow install error is serializable"),
                        );
                    } else {
                        eprintln!("Error: {error}");
                    }
                    return 1;
                }
            };
            if json {
                emit_stdout(
                    &serde_json::to_string_pretty(&serde_json::json!({
                        "installed": true,
                        "id": installed.id,
                        "name": installed.name,
                        "step_count": installed.step_count,
                        "fingerprint": workflow.document.fingerprint,
                    }))
                    .expect("workflow install summary is serializable"),
                );
            } else {
                emit_stdout(&format!(
                    "Installed workflow '{}' as {}",
                    installed.name, installed.id
                ));
            }
            0
        }
    }
}
