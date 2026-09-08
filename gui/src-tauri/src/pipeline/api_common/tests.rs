use super::*;

static READ_DIR_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn custom_endpoint_clients_separate_loopback_http_from_https() {
    assert!(std::ptr::eq(
        custom_endpoint_client("http://127.0.0.1:11434/v1"),
        &*LOCAL_HTTP_CLIENT
    ));
    assert!(std::ptr::eq(
        custom_endpoint_client("https://models.example.test/v1"),
        &*CUSTOM_HTTPS_HTTP_CLIENT
    ));
}

#[test]
fn incomplete_provider_responses_are_identified() {
    assert!(anthropic_output_incomplete(Some("max_tokens")));
    assert!(!anthropic_output_incomplete(Some("end_turn")));
    assert!(openai_output_incomplete(Some("length")));
    assert!(openai_output_incomplete(Some("content_filter")));
    assert!(!openai_output_incomplete(Some("stop")));
    assert!(google_output_incomplete(Some("MAX_TOKENS")));
    assert!(!google_output_incomplete(Some("STOP")));
}

#[test]
fn blocked_google_candidates_deserialize_without_content_or_parts() {
    // SAFETY/RECITATION-blocked candidates omit `content` entirely.
    let blocked: GoogleResponse = serde_json::from_value(serde_json::json!({
        "candidates": [{"finishReason": "SAFETY"}]
    }))
    .unwrap();
    let candidate = &blocked.candidates.unwrap()[0];
    assert!(candidate.content.parts.is_empty());
    assert!(google_output_incomplete(candidate.finish_reason.as_deref()));

    // MAX_TOKENS during thinking omits `parts`.
    let truncated: GoogleResponse = serde_json::from_value(serde_json::json!({
        "candidates": [{"content": {"role": "model"}, "finishReason": "MAX_TOKENS"}]
    }))
    .unwrap();
    assert!(truncated.candidates.unwrap()[0].content.parts.is_empty());
}

#[test]
fn transient_statuses_are_retryable() {
    assert!(transient_api_status(429));
    assert!(transient_api_status(503));
    assert!(transient_api_status(529));
    assert!(!transient_api_status(400));
    assert!(!transient_api_status(500));
}

#[test]
fn exhausted_api_quota_survives_429_formatting() {
    let error = format_api_error(
        "OpenAI",
        429,
        r#"{"error":{"message":"insufficient_quota: credit balance is too low"}}"#,
    );
    assert!(error.contains("insufficient_quota"));
    assert!(super::super::provider_error::is_usage_limit_error(&error));

    let transient = format_api_error("Anthropic", 429, r#"{"error":{"message":"slow down"}}"#);
    assert_eq!(
        transient,
        "Anthropic: Rate limited. Wait a moment and try again."
    );
}

#[test]
fn media_budget_replaces_overflowing_results_with_tool_errors() {
    let mut budget = ToolBudget::default();
    let policy = MediaPolicy {
        pdf_reads: true,
        encoded_media_budget: 10,
    };
    let accepted = enforce_media_budget(
        ToolResult::ImageBase64 {
            data: "12345678".to_string(),
            media_type: "image/png".to_string(),
        },
        &mut budget,
        &policy,
    );
    assert!(matches!(accepted, ToolResult::ImageBase64 { .. }));
    assert_eq!(budget.encoded_media_bytes, 8);

    let rejected = enforce_media_budget(
        ToolResult::PdfBase64("123".to_string()),
        &mut budget,
        &policy,
    );
    let ToolResult::Error(message) = rejected else {
        panic!("expected budget error")
    };
    assert!(message.contains("budget"), "{message}");
    assert_eq!(budget.encoded_media_bytes, 8);

    // Text results are never charged against the media budget.
    let text = enforce_media_budget(ToolResult::Text("t".repeat(100)), &mut budget, &policy);
    assert!(matches!(text, ToolResult::Text(_)));
    assert_eq!(budget.encoded_media_bytes, 8);
}

#[test]
fn tool_budget_caps_calls_and_argument_bytes() {
    let mut calls = ToolBudget::default();
    for _ in 0..MAX_TOOL_CALLS {
        calls.reserve_tool_call(0).unwrap();
    }
    assert!(calls.reserve_tool_call(0).unwrap_err().contains("count"));

    let mut single = ToolBudget::default();
    assert!(single
        .reserve_tool_call(MAX_TOOL_ARGUMENT_BYTES + 1)
        .unwrap_err()
        .contains("per-call"));

    let mut cumulative = ToolBudget::default();
    for _ in 0..4 {
        cumulative
            .reserve_tool_call(MAX_TOOL_ARGUMENT_BYTES)
            .unwrap();
    }
    assert!(cumulative
        .reserve_tool_call(1)
        .unwrap_err()
        .contains("cumulative"));
}

#[test]
fn tool_history_and_json_measurement_are_bounded() {
    let messages = (0..=MAX_TOOL_HISTORY_MESSAGES)
        .map(|_| OpenAIMessage {
            role: "tool".to_string(),
            content: Some(serde_json::json!("ok")),
            tool_calls: None,
            tool_call_id: Some("call".to_string()),
        })
        .collect::<Vec<_>>();
    assert!(validate_tool_history(&messages, "OpenAI")
        .unwrap_err()
        .contains("message"));

    let oversized = "x".repeat(1025);
    assert!(serialized_size_limited(&oversized, 1024, "test history")
        .unwrap_err()
        .contains("safety limit"));
}

#[test]
fn openai_usage_parses_cache_read_and_write_partitions() {
    let usage: OpenAIUsage = serde_json::from_value(serde_json::json!({
        "prompt_tokens": 1200,
        "completion_tokens": 80,
        "prompt_tokens_details": {
            "cached_tokens": 900,
            "cache_write_tokens": 200
        }
    }))
    .unwrap();
    let details = usage.prompt_tokens_details.unwrap();
    assert_eq!(details.cached_tokens, Some(900));
    assert_eq!(details.cache_write_tokens, Some(200));
}

#[test]
fn direct_usage_exposes_round_trips_and_tool_counts() {
    let mut usage = Usage {
        requests: 3,
        ..Default::default()
    };
    usage
        .tool_calls
        .add_kind(crate::models::ToolCallKind::TextFile, 2);
    usage
        .tool_calls
        .add_kind(crate::models::ToolCallKind::Web, 4);
    let emitted = usage.call_usage();
    assert_eq!(emitted.model_round_trips, 3);
    assert_eq!(emitted.tool_calls.text_file, 2);
    assert_eq!(emitted.tool_calls.web, 4);
    assert_eq!(
        direct_tool_kind("ReadDocumentAssetsBatch"),
        crate::models::ToolCallKind::Image
    );
}

#[test]
fn provider_hosted_search_counts_are_parsed() {
    let anthropic: AnthropicResponse = serde_json::from_value(serde_json::json!({
        "content": [{
            "type": "text",
            "text": "answer",
            "citations": [{"type": "web_search_result_location", "encrypted_index": "secret"}]
        }],
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 10,
            "output_tokens": 2,
            "server_tool_use": {"web_search_requests": 3}
        }
    }))
    .unwrap();
    assert_eq!(
        anthropic_hosted_search_count(anthropic.usage.as_ref().unwrap()),
        3
    );
    assert_eq!(anthropic_extract_text(&anthropic.content), "answer");
    let echoed = serde_json::to_value(AnthropicMessage {
        role: "assistant".to_string(),
        content: serde_json::Value::Array(anthropic.content),
    })
    .unwrap();
    assert_eq!(
        echoed["content"][0]["citations"][0]["encrypted_index"],
        "secret"
    );

    let google: GoogleResponse = serde_json::from_value(serde_json::json!({
        "candidates": [{
            "content": {"role": "model", "parts": [
                {
                    "toolCall": {
                        "toolType": "GOOGLE_SEARCH_WEB",
                        "args": {"queries": ["one", "two"]},
                        "id": "search-id"
                    },
                    "thoughtSignature": "encrypted"
                },
                {"text": "answer"}
            ]},
            "finishReason": "STOP",
            "groundingMetadata": {
                "webSearchQueries": ["one", "two", "one", ""]
            }
        }]
    }))
    .unwrap();
    let mut candidates = google.candidates.unwrap();
    let candidate = &candidates[0];
    assert_eq!(google_hosted_search_count(candidate), 2);
    let echoed = serde_json::to_value(&candidate.content).unwrap();
    assert_eq!(echoed["parts"][0]["thoughtSignature"], "encrypted");
    assert_eq!(echoed["parts"][0]["toolCall"]["id"], "search-id");
    candidates[0].grounding_metadata = None;
    assert_eq!(google_hosted_search_count(&candidates[0]), 2);
}

#[test]
fn batch_tool_schemas_keep_single_tools_and_bounded_ranges() {
    let text = ReadTextBatchToolDef::default();
    assert_eq!(text.name, "ReadTextBatch");
    let requests = &text.input_schema["properties"]["requests"];
    assert_eq!(requests["maxItems"], MAX_BATCH_TEXT_REQUESTS);
    let properties = &requests["items"]["properties"];
    assert_eq!(properties["start_line"]["minimum"], 1);
    assert_eq!(
        properties["max_bytes"]["maximum"],
        MAX_BATCH_TEXT_ITEM_BYTES
    );

    let images = DocumentAssetsBatchToolDef::default();
    assert_eq!(images.name, "ReadDocumentAssetsBatch");
    assert_eq!(
        images.input_schema["properties"]["file_paths"]["maxItems"],
        MAX_BATCH_ASSET_REQUESTS
    );
}

#[test]
fn utf8_and_line_range_bounds_are_deterministic() {
    let text = "one\nβeta\nthree\n";
    let beta_start = text.find('β').unwrap();
    let range = slice_text_range(text, beta_start, text.len(), 2).unwrap();
    assert_eq!(range.content, "β");
    assert_eq!(range.next_offset, Some(beta_start + 2));
    assert!(slice_text_range(text, beta_start + 1, text.len(), 8)
        .unwrap_err()
        .contains("UTF-8"));
    assert!(slice_text_range(text, beta_start, text.len(), 1)
        .unwrap_err()
        .contains("too small"));

    let (start, end) = text_line_bounds(text, Some(2), Some(2)).unwrap();
    assert_eq!(&text[start..end], "βeta\n");
    assert!(text_line_bounds(text, Some(4), Some(4)).is_err());
    assert!(text_line_bounds(text, Some(3), Some(2)).is_err());
}

#[test]
fn text_batch_handles_lines_duplicates_and_existing_budgets() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let paper = dir.path().join("document.md");
    std::fs::write(&paper, "one\nβeta\nthree\nfour\n").unwrap();
    let root = dir.path().to_string_lossy().to_string();
    let path = paper.to_string_lossy().to_string();
    let access = ToolAccess::new(&[root.as_str()], None);
    let input = serde_json::json!({
        "requests": [
            {"file_path": path, "start_line": 2, "end_line": 2},
            {"file_path": path, "start_line": 4, "end_line": 4},
            {"file_path": path, "start_line": 2, "end_line": 2},
            {"file_path": path, "offset": 0, "start_line": 1}
        ]
    });
    let mut budget = ToolBudget::default();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_read_text_batch(&input, &mut budget, &access));
    let ToolResult::Text(result) = result else {
        panic!("expected text batch result")
    };
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    let items = result["items"].as_array().unwrap();
    assert_eq!(items[0]["content"], "βeta\n");
    assert_eq!(items[1]["content"], "four\n");
    assert_eq!(items[2]["duplicate_of"], 0);
    assert!(items[3]["error"]
        .as_str()
        .unwrap()
        .contains("mutually exclusive"));
    assert_eq!(budget.read_calls, 2);
    assert_eq!(budget.read_bytes, "βeta\nfour\n".len());
}

#[test]
fn image_batch_deduplicates_and_serializes_for_each_provider() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.png");
    let second = dir.path().join("second.jpg");
    std::fs::write(&first, b"png").unwrap();
    std::fs::write(&second, b"jpeg").unwrap();
    let root = dir.path().to_string_lossy().to_string();
    let first_path = first.to_string_lossy().to_string();
    let second_path = second.to_string_lossy().to_string();
    let access = ToolAccess::new(&[root.as_str()], None);
    let input = serde_json::json!({
        "file_paths": [first_path, first_path, second_path]
    });
    let mut budget = ToolBudget::default();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_read_assets_batch(&input, &mut budget, &access));
    let ToolResult::ImageBatch { metadata, images } = result else {
        panic!("expected image batch")
    };
    assert_eq!(images.len(), 2);
    assert_eq!(budget.read_calls, 2);
    assert_eq!(budget.read_bytes, 7);
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["items"][1]["duplicate_of"], 0);

    let batch = || ToolResult::ImageBatch {
        metadata: "{\"items\":[]}".to_string(),
        images: vec![
            ToolImage {
                data: "cG5n".to_string(),
                media_type: "image/png".to_string(),
            },
            ToolImage {
                data: "anBlZw==".to_string(),
                media_type: "image/jpeg".to_string(),
            },
        ],
    };
    let (anthropic, is_error) = anthropic_tool_result_content(batch());
    assert!(!is_error);
    assert_eq!(anthropic.as_array().unwrap().len(), 3);

    let mut messages = Vec::new();
    let mut pending_images = Vec::new();
    append_openai_tool_result(&mut messages, "call-1", batch(), &mut pending_images);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, "tool");
    append_openai_image_message(&mut messages, pending_images);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[1]
            .content
            .as_ref()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let google_call = GoogleFunctionCall {
        name: "assets".to_string(),
        args: serde_json::json!({}),
        id: Some("call-id".to_string()),
    };
    let (response, image_parts) = google_tool_result_parts(&google_call, batch());
    assert_eq!(response["functionResponse"]["name"], "assets");
    assert_eq!(response["functionResponse"]["id"], "call-id");
    assert_eq!(image_parts.len(), 2);
    assert!(image_parts
        .iter()
        .all(|part| part.get("inlineData").is_some()));
}

#[test]
fn image_batch_enforces_cross_provider_raw_byte_cap() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.png");
    let second = dir.path().join("second.png");
    std::fs::File::create(&first)
        .unwrap()
        .set_len(7 * 1024 * 1024)
        .unwrap();
    std::fs::File::create(&second)
        .unwrap()
        .set_len(6 * 1024 * 1024)
        .unwrap();
    let root = dir.path().to_string_lossy().to_string();
    let access = ToolAccess::new(&[root.as_str()], None);
    let input = serde_json::json!({
        "file_paths": [
            first.to_string_lossy(),
            second.to_string_lossy()
        ]
    });
    let mut budget = ToolBudget::default();
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_read_assets_batch(&input, &mut budget, &access));
    let ToolResult::ImageBatch { metadata, images } = result else {
        panic!("expected partial image batch")
    };
    assert_eq!(images.len(), 1);
    assert_eq!(budget.read_calls, 2);
    assert_eq!(budget.read_bytes, 7 * 1024 * 1024);
    let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    assert!(metadata["items"][1]["error"]
        .as_str()
        .unwrap()
        .contains("too large"));
}

#[test]
fn validate_tool_path_allows_explicit_private_temp_root() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(b"paper text").unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_string_lossy().to_string();
    let root = tmp.path().parent().unwrap().to_string_lossy().to_string();
    let access = ToolAccess::new(&[root.as_str()], None);
    validate_tool_path(&access, &path, 1024).expect("temp file should be readable");
}

#[test]
fn validate_tool_path_rejects_outside_allowed_dirs() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    // Cargo.toml in the crate root exists but is neither in the temp dir
    // nor in the call's read grants, so it must be denied.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let err = validate_tool_path(&ToolAccess::default(), path, usize::MAX).unwrap_err();
    assert!(err.contains("Access denied"), "{err}");
}

#[test]
fn per_call_access_grants_remain_disjoint() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_file = first.path().join("first.md");
    let second_file = second.path().join("second.md");
    std::fs::write(&first_file, "first").unwrap();
    std::fs::write(&second_file, "second").unwrap();
    let first_root = first.path().to_string_lossy().to_string();
    let second_root = second.path().to_string_lossy().to_string();
    let first_access = ToolAccess::new(&[first_root.as_str()], None);
    let second_access = ToolAccess::new(&[second_root.as_str()], None);

    assert!(validate_tool_path(&first_access, &first_file.to_string_lossy(), 1024).is_ok());
    assert!(validate_tool_path(&first_access, &second_file.to_string_lossy(), 1024).is_err());
    assert!(validate_tool_path(&second_access, &second_file.to_string_lossy(), 1024).is_ok());
    assert!(validate_tool_path(&second_access, &first_file.to_string_lossy(), 1024).is_err());
}

#[test]
fn document_asset_reader_returns_multimodal_image_data() {
    let _guard = READ_DIR_TEST_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("figure.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\nvisual-bytes").unwrap();
    let root = dir.path().to_string_lossy().to_string();
    let access = ToolAccess::new(&[root.as_str()], None);
    let (data, media_type, raw_bytes) =
        read_image_for_tool(&access, &image.to_string_lossy(), MAX_IMAGE_SIZE).unwrap();
    assert_eq!(media_type, "image/png");
    assert_eq!(raw_bytes, b"\x89PNG\r\n\x1a\nvisual-bytes".len());
    assert_eq!(
        STANDARD.decode(data).unwrap(),
        b"\x89PNG\r\n\x1a\nvisual-bytes"
    );
}

#[test]
fn in_flight_future_is_interrupted_by_pass_cancellation() {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(async {
            let key = "api-common-cancellation-test";
            crate::commands::cancel_pass(key.to_string()).await.unwrap();
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                await_or_cancel(std::future::pending::<()>(), Some(key)),
            )
            .await
            .expect("cancellation should resolve promptly");
            assert!(result.unwrap_err().contains("cancelled"));
        });
}

#[test]
fn api_body_limit_rejects_a_chunk_before_growth() {
    let mut bytes = vec![0u8; 4];
    assert!(append_api_chunk(&mut bytes, &[1, 2], 6).is_ok());
    assert_eq!(bytes.len(), 6);
    assert!(append_api_chunk(&mut bytes, &[3], 6).is_err());
    assert_eq!(bytes.len(), 6);
}

#[test]
fn disk_reader_enforces_limit_even_after_metadata_validation() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), b"123456789").unwrap();
    assert_eq!(read_bytes_limited(temp.path(), 9).unwrap(), b"123456789");
    assert!(read_bytes_limited(temp.path(), 8).is_err());
}

#[cfg(unix)]
#[test]
fn disk_reader_rejects_fifo_without_blocking() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("pipe");
    let path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let started = std::time::Instant::now();
    let error = read_bytes_limited(&fifo, 1024).unwrap_err();
    assert!(error.contains("not a regular file"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn write_tool_confinement() {
    let dir = tempfile::tempdir().unwrap();
    reset_write_budget();
    let root = dir.path().to_string_lossy().to_string();
    let access = ToolAccess::new(&[], Some(&root));

    // Relative path lands inside the dir, subdirs created.
    let msg = write_file_for_tool(&access, "steps/report.md", "# hi").unwrap();
    assert!(msg.contains("report.md"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("steps/report.md")).unwrap(),
        "# hi"
    );

    // Absolute path inside the dir is accepted (raw, non-canonical form).
    let abs = dir.path().join("notes.md");
    write_file_for_tool(&access, &abs.to_string_lossy(), "n").unwrap();
    assert!(abs.exists());

    // Traversal and outside-absolute paths are rejected.
    assert!(write_file_for_tool(&access, "../escape.md", "x")
        .unwrap_err()
        .contains("Access denied"));
    assert!(write_file_for_tool(&access, "a/../../escape.md", "x")
        .unwrap_err()
        .contains("Access denied"));
    let outside = std::env::temp_dir().join("pipeline_write_escape.md");
    assert!(write_file_for_tool(&access, &outside.to_string_lossy(), "x").is_err());
    assert!(!outside.exists());

    // Oversized content is rejected.
    let big = "x".repeat(MAX_WRITE_SIZE + 1);
    assert!(write_file_for_tool(&access, "big.md", &big)
        .unwrap_err()
        .contains("too large"));

    // Symlinked destination is refused.
    #[cfg(unix)]
    {
        let target = dir.path().join("target.md");
        std::fs::write(&target, "t").unwrap();
        let link = dir.path().join("link.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_file_for_tool(&access, "link.md", "x")
            .unwrap_err()
            .contains("symlink"));

        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt as _;
        let fifo = dir.path().join("fifo.md");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert!(write_file_for_tool(&access, "fifo.md", "x")
            .unwrap_err()
            .contains("not a regular file"));
    }

    // Rejected writes do not consume either quota.
    assert_eq!(WRITE_COUNT.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(WRITE_BYTES.load(std::sync::atomic::Ordering::SeqCst), 5);

    // Disabled state rejects everything.
    assert!(write_file_for_tool(&ToolAccess::default(), "steps/report.md", "x").is_err());
}
