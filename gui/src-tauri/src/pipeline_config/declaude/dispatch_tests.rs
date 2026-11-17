//! Exercise the actual stock graph and supervised retry boundary against a
//! local fixture provider. No remote model, credentials, or user data are used.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const SOURCE: &str = "Importantly, output may rise by \\(x\\).";
const REPLACEMENT: &str = "Output may rise by \\(x\\).";

fn candidate(stage: &str, id: &str, hash: &str) -> Value {
    json!({
        "stage":stage, "source_hash":hash,
        "coverage":{"reviewed":["Introduction"],"limitations":[],"incomplete":false},
        "unresolved":[], "edits":[{
            "id":id, "location":"Introduction", "anchor":{"original":SOURCE,"prefix":"","suffix":""},
            "replacement":REPLACEMENT,"operation":"replace","reason":"Removes empty emphasis.",
            "priority":"recommended","meaning":[{"source_claim":"Output may rise by x.","preserved_as":"Output may rise by x."}],
            "referee_ids":["general_topic"],"source_ids":[],"requires":[],"deletion_basis":"not_deletion","retained_at":[]
        }]
    })
}

fn provider() -> (String, Arc<Mutex<Vec<String>>>) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let recorded = prompts.clone();
    std::thread::spawn(move || {
        let mut proposal_attempt = 0;
        let mut verification_attempt = 0;
        let hash = format!("{:x}", Sha256::digest(SOURCE.as_bytes()));
        for stream in listener.incoming() {
            let mut reader = std::io::BufReader::new(stream.unwrap());
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
                json!({"data":[{"id":"fixture"}]})
            } else {
                let mut bytes = vec![0; length]; reader.read_exact(&mut bytes).unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                let prompt = body["messages"].as_array().unwrap().iter()
                    .filter(|message| message["role"] == "user")
                    .map(|message| match &message["content"] {
                        Value::String(text) => text.clone(),
                        Value::Array(parts) => parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n"),
                        _ => panic!("unexpected prompt shape"),
                    }).collect::<Vec<_>>().join("\n");
                let mut artifact;
                if prompt.contains("Return stage candidates-v1 and source_hash") {
                    proposal_attempt += 1;
                    artifact = candidate("candidates-v1", "declaude_referee_1:1", &hash);
                    if proposal_attempt == 1 { artifact["edits"][0]["anchor"]["original"] = json!("FABRICATED_QUOTATION"); }
                } else if prompt.contains("Return stage consolidated-v1 and source_hash") {
                    assert!(!prompt.contains("FABRICATED_QUOTATION"));
                    artifact = candidate("consolidated-v1", "declaude_editor:1", &hash);
                    artifact["edits"][0]["source_ids"] = json!(["declaude_referee_1:1"]);
                } else {
                    verification_attempt += 1;
                    assert!(prompt.contains("Return stage verified-v1 and source_hash"));
                    let preview = prompt.split("private copy at ").last().unwrap().split(". Read the affected").next().unwrap();
                    assert_eq!(std::fs::read_to_string(preview).unwrap(), REPLACEMENT);
                    artifact = json!({"stage":"verified-v1", "source_hash":hash,
                        "coverage":{"reviewed":["Introduction"],"limitations":[],"incomplete":false},"unresolved":[],
                        "decisions":[{"candidate_id":"declaude_editor:1","decision":"accept","reason":"Preserves possibility and x.",
                            "meaning":[{"source_claim":"Output may rise by x.","preserved_as":"Output may rise by x."}],
                            "interaction_check":"The complete revision preserves the modal claim."}]});
                    if verification_attempt == 1 { artifact["decisions"] = json!([]); }
                }
                recorded.lock().unwrap().push(prompt);
                json!({"choices":[{"message":{"role":"assistant","content":artifact.to_string()},"finish_reason":"stop"}]})
            }.to_string();
            let _ = write!(reader.into_inner(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len());
            if verification_attempt == 2 {
                break;
            }
        }
    });
    (url, prompts)
}

#[tokio::test]
async fn declaude_dispatch_retries_invalid_quotes_and_incomplete_verification() {
    let (url, prompts) = provider();
    let settings = crate::settings::Settings {
        preferred_provider: "local".into(),
        local_base_url: url,
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    let run = crate::run_scope::ActiveScope::begin("declaude-contract-fixture");
    crate::run_scope::scoped(run.scope().clone(), async {
        let _settings = crate::settings::freeze_for_run(settings.clone());
        let temp = tempfile::tempdir().unwrap();
        let paper = temp.path().join("paper.txt"); std::fs::write(&paper, SOURCE).unwrap();
        let survey = temp.path().join("orientation.json");
        let brief = json!({"referees":["general_topic"], "extraction_quality_notes":[], "document":{"type":"paper", "audience":"Economists"}, "style_target":"Precise prose"});
        std::fs::write(&survey, brief.to_string()).unwrap();
        let artifacts = temp.path().join("artifacts"); std::fs::create_dir(&artifacts).unwrap();
        let mut profile = declaude_profile(); profile.context_cache.enabled = false;
        let config: PipelineConfig = profile.into();
        let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
        let empty = Default::default();
        let result = crate::pipeline::executor::execute_steps(
            &app, &config, survey.to_str().unwrap(), &brief, paper.to_str().unwrap(), "", "", "", "",
            &empty, &empty, &empty, &Default::default(), Some(artifacts.to_str().unwrap()), &settings, None,
        ).await.unwrap();
        assert!(result.failed_steps.is_empty(), "{:?}", result.failed_steps);
        let proposal = result.outputs.iter().find(|output| output.step_id == "declaude_referee_1").unwrap();
        let final_output = result.outputs.iter().find(|output| output.step_id == "declaude_report").unwrap();
        assert_eq!(proposal.attempt_count, 2);
        assert_eq!(final_output.attempt_count, 2);
        assert!(final_output.structured_json);
        assert!(final_output.rendered_markdown.as_ref().unwrap().contains(REPLACEMENT));
        assert!(final_output.raw_text.contains("declaude_editor:1"));
        assert_eq!(std::fs::read_to_string(paper).unwrap(), SOURCE);
    }).await;
    assert_eq!(prompts.lock().unwrap().len(), 5);
}
