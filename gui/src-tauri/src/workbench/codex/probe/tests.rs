use super::*;

#[test]
fn fixture_preserves_numeric_and_string_response_ids() {
    let fixture =
        include_str!("../../../../../tests/fixtures/workbench/app-server-0.147.0/handshake.jsonl");
    let ids: Vec<Value> = fixture
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter_map(|frame| frame.get("id").cloned())
        .collect();
    assert_eq!(ids, vec![json!(1), json!("account-read"), json!(3)]);
}

#[test]
fn classifies_colliding_client_and_server_id_namespaces() {
    let fixture = include_str!(
        "../../../../../tests/fixtures/workbench/app-server-0.147.0/interleaved-server-request.jsonl"
    );
    let frames: Vec<Value> = fixture
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(matches!(
        classify_frame(&frames[0]),
        Ok(FrameKind::ServerRequest { id, method })
            if id == &json!(7) && method == "item/tool/call"
    ));
    assert_eq!(
        classify_frame(&frames[1]),
        Ok(FrameKind::Notification("turn/started"))
    );
    assert!(matches!(
        classify_frame(&frames[2]),
        Ok(FrameKind::Response(response)) if response["id"] == json!(7)
    ));
}

#[test]
fn secret_environment_denylist_covers_supported_provider_keys() {
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
        "GEMINI_API_KEY",
    ] {
        assert!(SECRET_ENVIRONMENT_KEYS.contains(&key));
    }
}

#[test]
fn generated_experimental_bundle_accepts_probe_requests() {
    let bundle: Value = serde_json::from_str(include_str!(
        "../../../../../../docs/workbench/protocol/0.147.0/experimental.schemas.json"
    ))
    .unwrap();
    let definitions = bundle["definitions"].clone();
    let request_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$ref": "#/definitions/ClientRequest",
        "definitions": definitions
    });
    let notification_schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$ref": "#/definitions/ClientNotification",
        "definitions": bundle["definitions"].clone()
    });
    let request_validator = jsonschema::draft7::options()
        .build(&request_schema)
        .unwrap();
    let notification_validator = jsonschema::draft7::options()
        .build(&notification_schema)
        .unwrap();

    assert!(request_validator.is_valid(&initialize_request(json!(1))));
    assert!(request_validator.is_valid(&thread_start_request(
        json!(2),
        Path::new("/tmp/pipeline-workbench-fixture")
    )));
    assert!(request_validator.is_valid(&permission_profile_list_request(
        json!(3),
        Path::new("/tmp/pipeline-workbench-fixture")
    )));
    assert!(request_validator.is_valid(&command_exec_request(
        json!(4),
        read_file_command(Path::new("/tmp/pipeline-workbench-fixture/readable.txt")),
        Path::new("/tmp/pipeline-workbench-fixture")
    )));
    assert!(request_validator.is_valid(&streaming_command_exec_request(
        json!(5),
        long_running_command(),
        Path::new("/tmp/pipeline-workbench-fixture"),
        ACTIVE_COMMAND_PROCESS_ID
    )));
    assert!(request_validator.is_valid(&command_exec_terminate_request(
        json!(6),
        ACTIVE_COMMAND_PROCESS_ID
    )));
    assert!(request_validator.is_valid(&tty_command_exec_request(
        json!(7),
        long_running_command(),
        Path::new("/tmp/pipeline-workbench-fixture"),
        TERMINATED_COMMAND_PROCESS_ID
    )));
    assert!(request_validator.is_valid(&command_exec_resize_request(
        json!(8),
        TERMINATED_COMMAND_PROCESS_ID
    )));
    assert!(request_validator.is_valid(&stdin_command_exec_request(
        json!(9),
        stdin_echo_command(),
        Path::new("/tmp/pipeline-workbench-fixture")
    )));
    assert!(request_validator.is_valid(&command_exec_write_request(
        json!(10),
        STDIN_COMMAND_PROCESS_ID,
        STDIN_FIXTURE_CONTENT.as_bytes()
    )));
    assert!(notification_validator.is_valid(&initialized_notification()));
}
