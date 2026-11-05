//! Files from attempts without an accepted response, through real step calls
//! against a local provider that writes as a model would.

use super::super::artifact_context::ResolvedArtifactContext;
use super::super::paths::step_write_dir;
use super::super::step_call::{execute_step_call, StepCallRequest};
use super::*;
use crate::pipeline::unit_files;
use std::path::Path;

const ACCEPTED: &str = r#"{"content":"A complete report."}"#;
const REJECTED: &str = "not an artifact";

/// A local OpenAI-compatible server. Each request first writes its named file
/// into the unit's `files/` folder, then gets its reply; the last turn repeats.
fn provider_writing_files(unit_root: &str, turns: Vec<(&'static str, &'static str)>) -> String {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let files = Path::new(unit_root).join("files");
    std::thread::spawn(move || {
        let mut turns = turns.into_iter().peekable();
        let mut last = None;
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let mut reader = std::io::BufReader::new(stream);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            if let Some(turn) = turns.next() {
                last = Some(turn);
            }
            let (file, content) = last.unwrap();
            std::fs::create_dir_all(&files).unwrap();
            std::fs::write(files.join(file), "written").unwrap();
            let reply = serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop",
                }],
                "usage": {"prompt_tokens": 1, "completion_tokens": 1},
            })
            .to_string();
            let _ = write!(
                reader.into_inner(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    format!("http://{address}/v1")
}

/// One unit of `producer` through the real attempt loop, with one retry.
async fn run_unit(
    artifacts: &str,
    step_key: &str,
    turns: Vec<(&'static str, &'static str)>,
) -> Result<String, String> {
    let unit_root = step_write_dir(Some(artifacts), step_key).unwrap().unwrap();
    let settings = crate::settings::Settings {
        local_base_url: provider_writing_files(&unit_root, turns),
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    let run = crate::run_scope::ActiveScope::begin(format!("unit-files-test-{unit_root}"));
    let call = execute_step_call(StepCallRequest {
        author_note: None,
        app: &app,
        pass_key: step_key,
        log_label: step_key,
        prompt: "test",
        system_prompt: None,
        tools: &[],
        agent: Some("local"),
        cwd: None,
        read_dirs: &[],
        run_artifact_dir: Some(artifacts),
        write_dir: Some(&unit_root),
        report_rel: "report.md",
        report_nonce: "nonce",
        output_schema: None,
        preserved_finding_ids: None,
        command_model: Some("fixture"),
        display_model: "fixture",
        model_policy: "fixed",
        effort: "medium",
        settings: &settings,
        shared_context: None,
    });
    crate::run_scope::scoped(run.scope().clone(), call)
        .await
        .map(|result| result.text)
}

/// What a later step selecting the producer's files is given.
fn selected_by_a_later_step(artifacts: &str, glob: &str) -> ResolvedArtifactContext {
    let mut step = make_step("consumer", Phase::Sequential);
    step.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Files],
        glob: glob.into(),
    }];
    resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: "",
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: Some(artifacts),
            context_pool: &Default::default(),
        },
    )
    .unwrap()
}

#[tokio::test]
async fn a_retry_leaves_only_the_accepted_attempts_files() {
    let folder = tempfile::tempdir().unwrap();
    let artifacts = folder.path().to_str().unwrap();
    let text = run_unit(
        artifacts,
        "producer",
        vec![("draft.csv", REJECTED), ("final.csv", ACCEPTED)],
    )
    .await
    .unwrap();
    assert_eq!(text, "A complete report.");

    let manifest = selected_by_a_later_step(artifacts, "*.csv").manifest;
    assert!(manifest.contains("final.csv"), "{manifest}");
    assert!(!manifest.contains("draft.csv"), "{manifest}");

    // The rejected attempt's file is kept, away from the unit it came from.
    let unit = step_slug("producer");
    let rejected = folder
        .path()
        .join("rejected-files")
        .join(&unit)
        .join(&unit)
        .join("attempt-01/draft.csv");
    assert!(rejected.is_file());
    assert!(!unit_files::call_is_open(
        &folder.path().join("by-step").join(&unit).join(&unit)
    ));
}

#[tokio::test]
async fn a_unit_that_never_gets_an_accepted_response_passes_nothing_on() {
    let folder = tempfile::tempdir().unwrap();
    let artifacts = folder.path().to_str().unwrap();
    run_unit(artifacts, "producer", vec![("draft.csv", REJECTED)])
        .await
        .unwrap_err();

    let manifest = selected_by_a_later_step(artifacts, "*.csv").manifest;
    assert!(!manifest.contains("draft.csv"), "{manifest}");
    let unit = step_slug("producer");
    let rejected = folder.path().join("rejected-files").join(&unit).join(&unit);
    assert!(rejected.join("attempt-01/draft.csv").is_file());
    assert!(rejected.join("attempt-02/draft.csv").is_file());
}

#[tokio::test]
async fn a_failed_agent_keeps_its_files_from_a_step_another_agent_finished() {
    let folder = tempfile::tempdir().unwrap();
    let artifacts = folder.path().to_str().unwrap();
    run_unit(artifacts, "producer/first", vec![("kept.csv", ACCEPTED)])
        .await
        .unwrap();
    run_unit(artifacts, "producer/second", vec![("lost.csv", REJECTED)])
        .await
        .unwrap_err();

    let manifest = selected_by_a_later_step(artifacts, "*.csv").manifest;
    assert!(manifest.contains("kept.csv"), "{manifest}");
    assert!(!manifest.contains("lost.csv"), "{manifest}");
}

#[test]
fn a_call_cut_off_by_a_crash_passes_nothing_on() {
    let folder = tempfile::tempdir().unwrap();
    let artifacts = folder.path().to_str().unwrap();
    let unit_root = step_write_dir(Some(artifacts), "producer")
        .unwrap()
        .unwrap();
    let files = Path::new(&unit_root).join("files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("partial.csv"), "written").unwrap();
    unit_files::begin_call(&unit_root).unwrap();

    let manifest = selected_by_a_later_step(artifacts, "*.csv").manifest;
    assert!(!manifest.contains("partial.csv"), "{manifest}");
    assert!(files.join("partial.csv").is_file(), "kept as evidence");

    // A folder-wide selection is a read grant, and must skip the unit as well.
    let whole_folder = selected_by_a_later_step(artifacts, "");
    assert!(!whole_folder
        .read_dirs
        .iter()
        .any(|root| root.ends_with("/files")));
}
