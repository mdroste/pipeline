//! Exercise the real reviewer dispatch paths against a recording local provider.
use super::*;
use serde_json::{json, Value};

fn provider() -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let recorded = prompts.clone();
    std::thread::spawn(move || {
        let mut rejected_core = false;
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = std::io::BufReader::new(stream);
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let response = if first.starts_with("GET") {
                json!({"data": [{"id": "fixture"}]})
            } else {
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                let prompt = body["messages"].as_array().unwrap().iter()
                    .filter(|message| message["role"] == "user")
                    .map(|message| match &message["content"] {
                        Value::String(text) => text.clone(),
                        Value::Array(parts) => parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n"),
                        _ => panic!("unexpected user content"),
                    }).collect::<Vec<_>>().join("\n");
                let reject = prompt.contains("Core task") && !rejected_core;
                rejected_core |= reject;
                recorded.lock().unwrap().push(prompt);
                json!({"choices": [{"message": {"role": "assistant", "content": if reject { "invalid artifact" } else { r#"{"content":"A completed review."}"# }}, "finish_reason":"stop"}]})
            }.to_string();
            let _ = write!(reader.into_inner(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len());
        }
    });
    (format!("http://{address}/v1"), prompts)
}

async fn review(note: &str, cached: bool) -> Vec<String> {
    let (url, prompts) = provider();
    let settings = crate::settings::Settings {
        preferred_provider: "local".into(),
        default_orientation_agent: "local".into(),
        default_merge_agent: "local".into(),
        local_base_url: url,
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    let run = crate::run_scope::ActiveScope::begin(format!("author-note-{cached}"));
    crate::run_scope::scoped(run.scope().clone(), async {
        let _settings = crate::settings::freeze_for_run(settings.clone());
        let dir = tempfile::tempdir().unwrap();
        let paper = dir.path().join("paper.txt");
        std::fs::write(&paper, "DOCUMENT_SENTINEL").unwrap();
        let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
        let extraction = crate::models::ExtractionResult {
            text: "DOCUMENT_SENTINEL".into(),
            method: "text".into(),
            source_path: String::new(),
            paper_hash: "fixture".into(),
            quality_notes: vec![],
        };
        crate::pipeline::orient::build_orientation_map(
            &app,
            &extraction,
            Some("Orientation task {input_text}"),
            Some(&json!({"type":"object"})),
            None,
            Some(note),
        )
        .await
        .unwrap();
        let mut steps = Vec::new();
        for (id, phase, prompt) in [
            ("core", Phase::Parallel, "Core task"),
            ("specialist", Phase::Parallel, "Specialist task"),
            ("final", Phase::Sequential, "Sequential task"),
        ] {
            let mut step = make_step(id, phase);
            step.agents = vec!["local".into()];
            step.prompt = prompt.into();
            step.context.include = if id == "specialist" {
                vec![]
            } else {
                vec![ArtifactSelector::Primary {
                    parts: vec![PrimaryArtifactPart::Text],
                }]
            };
            steps.push(step);
        }
        let config = PipelineConfig {
            steps,
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: crate::pipeline_config::ContextCacheConfig { enabled: cached },
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: "{step_prompt}".into(),
            variables: vec![],
        };
        let empty = Default::default();
        let result = execute_steps(
            &app,
            &config,
            "",
            &Value::Null,
            paper.to_str().unwrap(),
            "",
            "",
            "",
            "",
            &empty,
            &empty,
            &empty,
            &Default::default(),
            None,
            &settings,
            Some(note),
        )
        .await
        .unwrap();
        assert!(result.failed_steps.is_empty(), "{:?}", result.failed_steps);
        assert_eq!(result.outputs.len(), 3);
        let outputs = ["a", "b"]
            .into_iter()
            .map(|agent| StepOutput {
                step_id: format!("core/{agent}"),
                step_label: "Core".into(),
                merge_group: "core".into(),
                raw_text: "review".into(),
                agent: agent.into(),
                ..Default::default()
            })
            .collect();
        crate::pipeline::merge::merge_step_outputs(
            &app,
            outputs,
            &crate::pipeline_config::MergeConfig {
                prompt: "Merge task {topic} {agent_reports}".into(),
                agents: vec!["local".into()],
                ..Default::default()
            },
            &Arc::new(tokio::sync::Semaphore::new(1)),
            None,
            &settings,
            &Default::default(),
            Some(note),
        )
        .await
        .unwrap();
    })
    .await;
    let result = prompts.lock().unwrap().clone();
    result
}

#[tokio::test]
async fn author_notes_reach_all_review_paths_and_retries_without_cross_run_leaks() {
    let (cached, uncached) = tokio::join!(
        review("CACHE_NOTE {var:literal}", true),
        review("PLAIN_NOTE", false)
    );
    for (prompts, own, other) in [
        (&cached, "CACHE_NOTE {var:literal}", "PLAIN_NOTE"),
        (&uncached, "PLAIN_NOTE", "CACHE_NOTE"),
    ] {
        assert!(prompts.len() >= 6, "{prompts:?}");
        for task in [
            "Orientation task",
            "Core task",
            "Specialist task",
            "Sequential task",
            "Merge task",
        ] {
            assert!(
                prompts.iter().any(|prompt| prompt.contains(task)),
                "missing {task}"
            );
        }
        assert!(prompts.iter().any(|prompt| prompt.contains("RETRY NOTICE")));
        for prompt in prompts {
            assert!(prompt.starts_with("NOTE FROM THE AUTHOR\n"), "{prompt}");
            assert_eq!(prompt.matches(own).count(), 1, "{prompt}");
            assert!(!prompt.contains(other));
        }
    }
    assert!(cached
        .iter()
        .any(|prompt| prompt.contains("SHARED INPUT CONTEXT")));
    assert!(uncached
        .iter()
        .all(|prompt| !prompt.contains("SHARED INPUT CONTEXT")));
}
