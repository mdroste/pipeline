//! Fixed output and managed-process failure contracts for PDF extraction.

use super::*;

#[test]
fn full_parser_preserves_exact_rendering_and_quality_note_order() {
    let structure: PaddleStructure = serde_json::from_value(serde_json::json!({
        "schema_version": PADDLE_STRUCTURE_SCHEMA,
        "parser": "paddleocr-vl-full",
        "parser_version": "fixture",
        "quality_notes": ["first note", "second note", "first note"],
        "pages": [
            {
                "number": 1,
                "markdown": "# Introduction\n\n![Plot](assets/plot.png)\n<img src=\"assets/inline.png\"><img src='assets/single.png'>",
                "source_text_chars": 120
            },
            {"number": 2, "source_text_chars": 120}
        ]
    }))
    .unwrap();
    let baseline = [PdfPageTextBaseline {
        total_chars: 120,
        prose_chars: 120,
    }; 2];
    let extraction = full_parser_extraction_from_structure(
        &structure,
        Path::new("/fixture/paper.pdf"),
        "0123456789abcdef",
        Some(&baseline),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(extraction).unwrap(),
        serde_json::json!({
            "text": "<!-- PAGE 1 -->\n\n# Introduction\n\n![Plot](../artifacts/figures/plot.png)\n<img src=\"../artifacts/figures/inline.png\"><img src='../artifacts/figures/single.png'>\n\n<!-- PAGE 2 -->",
            "method": "paddleocr-vl-full",
            "source_path": "/fixture/paper.pdf",
            "paper_hash": "0123456789abcdef",
            "quality_notes": [
                "first note",
                "second note",
                "first note",
                "PaddleOCR cross-page restructuring moved all recognized content from page(s) 2 into adjacent structured blocks."
            ]
        })
    );
}

/// One 144 DPI letter page with the given recognized character count.
fn single_page_structure(source_text_chars: usize, blocks: serde_json::Value) -> PaddleStructure {
    serde_json::from_value(serde_json::json!({
        "schema_version": PADDLE_STRUCTURE_SCHEMA,
        "parser": "paddleocr-vl-full",
        "parser_version": "fixture",
        "pages": [{
            "number": 1,
            "markdown": "Recognized page content.",
            "source_text_chars": source_text_chars,
            "width": 1224,
            "height": 1584,
            "blocks": blocks,
        }]
    }))
    .unwrap()
}

fn block(role: &str, bbox: [f64; 4]) -> serde_json::Value {
    serde_json::json!({ "block_id": role, "role": role, "bbox": bbox })
}

fn validate_page(
    structure: &PaddleStructure,
    total_chars: usize,
) -> Result<ExtractionResult, String> {
    full_parser_extraction_from_structure(
        structure,
        Path::new("/fixture/paper.pdf"),
        "0123456789abcdef",
        Some(&[PdfPageTextBaseline {
            total_chars,
            // Side-by-side panel labels read as prose lines, so this page is
            // not recognized as one with fragmented figure labels.
            prose_chars: total_chars,
        }]),
    )
}

#[test]
fn full_parser_keeps_figure_only_pages_whose_text_stays_in_the_figures() {
    let title = block("figure_title", [375.0, 204.0, 848.0, 232.0]);
    let chart = block("chart", [245.0, 252.0, 988.0, 788.0]);
    let lower_chart = block("chart", [242.0, 863.0, 982.0, 1397.0]);
    let figure_page = single_page_structure(
        80,
        serde_json::json!([title.clone(), chart.clone(), lower_chart.clone()]),
    );
    assert!(validate_page(&figure_page, 883)
        .unwrap()
        .quality_notes
        .is_empty());

    let with_body_text = single_page_structure(
        80,
        serde_json::json!([
            title.clone(),
            chart,
            lower_chart,
            block("text", [100.0, 1400.0, 1100.0, 1500.0])
        ]),
    );
    assert!(validate_page(&with_body_text, 883)
        .unwrap_err()
        .contains("recognized 80 substantive characters"));

    // A small chart cannot account for a large text layer.
    let small_chart = single_page_structure(
        80,
        serde_json::json!([title, block("chart", [300.0, 300.0, 700.0, 600.0])]),
    );
    assert!(validate_page(&small_chart, 883)
        .unwrap_err()
        .contains("recognized 80 substantive characters"));
}

#[test]
fn full_parser_reports_withheld_image_text_once_per_document() {
    let plot = |id: &str| {
        serde_json::json!({
            "block_id": id,
            "role": "image",
            "bbox": [321.0, 157.0, 908.0, 570.0],
            "raw": { "unverified_image_text": "Month | Baseline\n1 | -0.1\n5 | -0.8" },
        })
    };
    let structure: PaddleStructure = serde_json::from_value(serde_json::json!({
        "schema_version": PADDLE_STRUCTURE_SCHEMA,
        "parser": "paddleocr-vl-full",
        "parser_version": "fixture",
        "pages": [
            { "number": 1, "markdown": "Plots.", "blocks": [plot("a"), plot("b")] },
            { "number": 2, "markdown": "Prose.", "blocks": [block("image", [0.0; 4])] },
            { "number": 3, "markdown": "Plot.", "blocks": [plot("c")] },
        ]
    }))
    .unwrap();
    let extraction = full_parser_extraction_from_structure(
        &structure,
        Path::new("/fixture/paper.pdf"),
        "0123456789abcdef",
        None,
    )
    .unwrap();
    assert_eq!(
        extraction.quality_notes,
        vec![
            "Image text withheld as unverified: 3 image region(s) on page(s) 1, 3 returned the kind of data the recognizer makes up for plots. Read those figures from their images."
                .to_string()
        ]
    );
    assert!(crate::pipeline::extract::is_withheld_image_text_note(
        &extraction.quality_notes[0]
    ));

    let without_recognition = single_page_structure(
        80,
        serde_json::json!([block("image", [321.0, 157.0, 908.0, 570.0])]),
    );
    assert!(validate_page(&without_recognition, 80)
        .unwrap()
        .quality_notes
        .is_empty());
}

#[test]
fn full_parser_notes_pages_that_recognize_far_less_than_the_text_layer() {
    let body = block("text", [100.0, 100.0, 1100.0, 1000.0]);
    assert_eq!(
        validate_page(
            &single_page_structure(1_963, serde_json::json!([body.clone()])),
            2_529
        )
        .unwrap()
        .quality_notes,
        vec![
            "Page 1: recognized 1963 of 2529 characters in the PDF text layer; some text may be missing, so consult the rendered page."
        ]
    );
    assert!(validate_page(
        &single_page_structure(2_100, serde_json::json!([body.clone()])),
        2_529
    )
    .unwrap()
    .quality_notes
    .is_empty());

    // Plot labels sit inside the chart, so text-layer coverage says nothing here.
    assert!(validate_page(
        &single_page_structure(
            1_000,
            serde_json::json!([body, block("chart", [100.0, 1020.0, 700.0, 1500.0])])
        ),
        2_529
    )
    .unwrap()
    .quality_notes
    .is_empty());
}

#[cfg(unix)]
fn write_fake_executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn managed_paddle_server_exit_preserves_error_and_forgets_child() {
    let directory = tempfile::tempdir().unwrap();
    let server = directory.path().join("fake-server");
    let pid_file = directory.path().join("server.pid");
    write_fake_executable(
        &server,
        "#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$2\"\nexit 23\n",
    );
    let paths = crate::engines::PaddleEnginePaths {
        server,
        model: pid_file.clone(),
        mmproj: directory.path().join("unused-projector"),
        integrity_sha256: "fixture".into(),
    };
    let active = crate::run_scope::ActiveScope::begin("pdf-server-exit-characterization");
    let settings = crate::settings::Settings::default();
    let result = crate::run_scope::scoped(active.scope().clone(), async {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            start_paddle_server(&paths, &settings),
        )
        .await
        .expect("fake server exit should be observed promptly")
    })
    .await;
    match result {
        Err(error) => assert_eq!(
            error,
            "Managed llama-server exited during model loading (exit 23)"
        ),
        Ok(_) => panic!("fake server must not become ready"),
    }
    let pid = fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    assert!(!active.scope().has_child(pid));
}

#[cfg(unix)]
#[test]
fn built_in_recognition_runs_the_sidecar_without_a_server() {
    let directory = tempfile::tempdir().unwrap();
    let python = directory.path().join("fake-python");
    write_fake_executable(
        &python,
        "#!/bin/sh\nprintf 'arg=%s\\n' \"$@\"\nprintf 'allocator=%s key=%s\\n' \"$FLAGS_use_system_allocator\" \"$PIPELINE_PADDLE_API_KEY\" >&2\nexit 23\n",
    );
    let model_dir = directory.path().join("recognition-model");
    let paths = crate::engines::PaddleFullParserPaths {
        python,
        script: directory.path().join("sidecar.py"),
        model_cache: directory.path().join("cache"),
        layout_model: directory.path().join("layout"),
        recognition: crate::engines::PaddleRecognition::BuiltIn {
            model_dir: model_dir.clone(),
        },
        release: "fixture".into(),
        integrity_sha256: "fixture".into(),
        sidecar_sha256: "fixture".into(),
    };
    let active = crate::run_scope::ActiveScope::begin("pdf-sidecar-built-in-characterization");
    let output = crate::run_scope::enter_blocking(active.scope().clone(), || {
        run_paddle_full_sidecar(
            &paths,
            &crate::settings::Settings::default(),
            &directory.path().join("input.pdf"),
            &directory.path().join("output.json"),
            &directory.path().join("assets"),
            &SidecarRecognition::BuiltIn { model_dir },
            &[],
            std::time::Duration::from_secs(2),
        )
    })
    .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stderr, b"allocator=1 key=\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("arg=--native-model-dir"));
    assert!(stdout.contains("recognition-model"));
    assert!(!stdout.contains("arg=--server-url"));
}

#[cfg(unix)]
#[test]
fn managed_paddle_sidecar_preserves_failure_capture_and_private_key_environment() {
    let directory = tempfile::tempdir().unwrap();
    let python = directory.path().join("fake-python");
    write_fake_executable(
        &python,
        "#!/bin/sh\nprintf 'pid=%s\\n' \"$$\"\nprintf 'arg=%s\\n' \"$@\"\nprintf 'fixture-key=%s\\n' \"$PIPELINE_PADDLE_API_KEY\" >&2\nexit 23\n",
    );
    let paths = crate::engines::PaddleFullParserPaths {
        python,
        script: directory.path().join("sidecar.py"),
        model_cache: directory.path().join("cache"),
        layout_model: directory.path().join("layout"),
        recognition: crate::engines::PaddleRecognition::Llama(crate::engines::PaddleEnginePaths {
            server: directory.path().join("unused-server"),
            model: directory.path().join("unused-model"),
            mmproj: directory.path().join("unused-projector"),
            integrity_sha256: "fixture".into(),
        }),
        release: "fixture".into(),
        integrity_sha256: "fixture".into(),
        sidecar_sha256: "fixture".into(),
    };
    let active = crate::run_scope::ActiveScope::begin("pdf-sidecar-failure-characterization");
    let output = crate::run_scope::enter_blocking(active.scope().clone(), || {
        run_paddle_full_sidecar(
            &paths,
            &crate::settings::Settings::default(),
            &directory.path().join("input.pdf"),
            &directory.path().join("output.json"),
            &directory.path().join("assets"),
            &SidecarRecognition::Server {
                base_url: "http://127.0.0.1:1/v1".into(),
                api_key: "sidecar-fixture-key".into(),
            },
            &[PdfPageTextBaseline {
                total_chars: 200,
                prose_chars: 120,
            }],
            std::time::Duration::from_secs(2),
        )
    })
    .unwrap();
    assert_eq!(output.status.code(), Some(23));
    assert!(!output.stdout_truncated);
    assert!(!output.stderr_truncated);
    assert_eq!(output.stderr, b"fixture-key=sidecar-fixture-key\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    let pid = lines
        .next()
        .unwrap()
        .strip_prefix("pid=")
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let arguments: Vec<_> = lines.collect();
    assert_eq!(&arguments[..2], &["arg=-I", "arg=-B"]);
    assert_eq!(arguments[2], format!("arg={}", paths.script.display()));
    assert!(arguments
        .windows(2)
        .any(|pair| pair == ["arg=--baseline-chars", "arg=[200]"]));
    assert!(arguments
        .windows(2)
        .any(|pair| pair == ["arg=--baseline-prose-chars", "arg=[120]"]));
    assert!(!stdout.contains("sidecar-fixture-key"));
    assert!(!active.scope().has_child(pid));
}
