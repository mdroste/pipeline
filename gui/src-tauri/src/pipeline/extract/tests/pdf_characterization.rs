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
        paddle: crate::engines::PaddleEnginePaths {
            server: directory.path().join("unused-server"),
            model: directory.path().join("unused-model"),
            mmproj: directory.path().join("unused-projector"),
            integrity_sha256: "fixture".into(),
        },
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
            "http://127.0.0.1:1/v1",
            "sidecar-fixture-key",
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
