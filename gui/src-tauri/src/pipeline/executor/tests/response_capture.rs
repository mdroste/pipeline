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
