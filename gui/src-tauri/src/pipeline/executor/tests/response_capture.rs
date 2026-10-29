use super::*;
fn request<'a>(
    app: &'a crate::emit::EventBus,
    settings: &'a crate::settings::Settings,
    root: Option<&'a str>,
) -> StepCallRequest<'a> {
    StepCallRequest {
        app,
        pass_key: "audit",
        log_label: "audit",
        prompt: "test",
        system_prompt: None,
        tools: &[],
        agent: None,
        cwd: None,
        read_dirs: &[],
        run_artifact_dir: root,
        write_dir: None,
        report_rel: "report.md",
        report_nonce: "audit",
        output_schema: None,
        preserved_finding_ids: None,
        command_model: None,
        display_model: "test",
        model_policy: "fixed",
        effort: "medium",
        settings,
        shared_context: None,
    }
}
#[tokio::test]
async fn required_capture_errors_are_not_converted_to_nonpersistent_success() {
    let t = tempfile::tempdir().unwrap();
    let settings = crate::settings::Settings::default();
    let app: crate::emit::EventBus = Arc::new(crate::emit::CliEvents);
    let blocked = t.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let req = request(&app, &settings, Some(blocked.to_str().unwrap()));
    assert!(
        capture_response(&req, 1, "terminal", "{\"content\":\"valid\"}")
            .await
            .is_err()
    );
    let full = t.path().join("full");
    let journal = full.join("agent-responses");
    std::fs::create_dir_all(&journal).unwrap();
    std::fs::File::create(journal.join("prior.md"))
        .unwrap()
        .set_len(128 * 1024 * 1024)
        .unwrap();
    let req = request(&app, &settings, Some(full.to_str().unwrap()));
    assert!(capture_response(&req, 1, "terminal", "valid")
        .await
        .err()
        .unwrap()
        .contains("safety limit"));
    assert!(
        capture_response(&request(&app, &settings, None), 1, "terminal", "valid")
            .await
            .unwrap()
            .is_none()
    );
}

/// A local OpenAI-compatible server that answers every request with text no
/// artifact schema accepts, reporting `prompt`/`completion` tokens each time.
fn serve_unusable_completions(prompt: u64, completion: u64) -> String {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
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
            let reply = serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": "not an artifact"},
                    "finish_reason": "stop",
                }],
                "usage": {"prompt_tokens": prompt, "completion_tokens": completion},
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

#[tokio::test]
async fn a_call_whose_every_attempt_fails_keeps_their_usage() {
    let settings = crate::settings::Settings {
        local_base_url: serve_unusable_completions(100, 20),
        local_model: "fixture".into(),
        max_retries: 1,
        ..Default::default()
    };
    let app: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
    let mut req = request(&app, &settings, None);
    req.agent = Some("local");
    req.command_model = Some("fixture");
    // A private run scope, so cancellation in other tests cannot reach it.
    let run = crate::run_scope::ActiveScope::begin("metered-step-call-test");
    let outcome =
        crate::run_scope::scoped(run.scope().clone(), execute_metered_step_call(req)).await;
    let Err(failure) = outcome else {
        panic!("an unusable response must not be accepted");
    };
    assert!(
        failure.error.contains("invalid terminal structured output"),
        "{}",
        failure.error
    );
    assert!(failure.spent.text.is_empty());
    assert_eq!(failure.spent.attempt_count, 2);
    assert_eq!(
        (
            failure.spent.usage.input_tokens,
            failure.spent.usage.output_tokens
        ),
        (200, 40),
        "both attempts were paid for"
    );
}
