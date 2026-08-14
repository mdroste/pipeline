use super::*;

#[test]
fn full_parser_cache_identity_uses_verified_content_digests() {
    let paddle = crate::engines::PaddleEnginePaths {
        server: PathBuf::from("/managed/runtime/llama-server"),
        model: PathBuf::from("/managed/models/model.gguf"),
        mmproj: PathBuf::from("/managed/models/projector.gguf"),
        integrity_sha256: "recognition-integrity-a".to_string(),
    };
    let mut paths = crate::engines::PaddleFullParserPaths {
        python: PathBuf::from("/managed/parser/python"),
        script: PathBuf::from("/managed/parser/sidecar.py"),
        model_cache: PathBuf::from("/managed/parser/cache"),
        layout_model: PathBuf::from("/managed/parser/layout"),
        paddle,
        release: "parser-release".to_string(),
        integrity_sha256: "parser-integrity-a".to_string(),
        sidecar_sha256: "sidecar-a".to_string(),
    };
    let settings = crate::settings::Settings::default();
    let original = full_parser_cache_fingerprint(&paths, &settings);

    // Mutable paths and timestamps are not cache provenance.
    paths.python = PathBuf::from("/different/path/python");
    paths.layout_model = PathBuf::from("/different/path/layout");
    assert_eq!(full_parser_cache_fingerprint(&paths, &settings), original);

    paths.integrity_sha256 = "parser-integrity-b".to_string();
    assert_ne!(full_parser_cache_fingerprint(&paths, &settings), original);
    paths.integrity_sha256 = "parser-integrity-a".to_string();
    paths.paddle.integrity_sha256 = "recognition-integrity-b".to_string();
    assert_ne!(full_parser_cache_fingerprint(&paths, &settings), original);
}

#[test]
fn direct_pdf_attachment_requires_the_matching_cloud_api_key() {
    let mut settings = crate::settings::Settings {
        preferred_provider: "claude".to_string(),
        ..Default::default()
    };
    assert!(!provider_uses_direct_api(&settings));
    settings.anthropic_api_key = "configured".to_string();
    assert!(provider_uses_direct_api(&settings));

    settings.preferred_provider = "codex".to_string();
    assert!(!provider_uses_direct_api(&settings));
    settings.openai_api_key = "configured".to_string();
    assert!(provider_uses_direct_api(&settings));

    settings.preferred_provider = "antigravity".to_string();
    assert!(!provider_uses_direct_api(&settings));
    settings.google_api_key = "configured".to_string();
    assert!(provider_uses_direct_api(&settings));

    settings.preferred_provider = "local".to_string();
    settings.local_api_key = "configured".to_string();
    assert!(!provider_uses_direct_api(&settings));
}

#[test]
fn selected_file_is_staged_without_its_siblings() {
    let selected_dir = tempfile::tempdir().unwrap();
    let selected = selected_dir.path().join("paper.pdf");
    fs::write(&selected, b"selected").unwrap();
    fs::write(selected_dir.path().join("private-notes.txt"), b"secret").unwrap();
    let private = tempfile::tempdir().unwrap();

    let scoped = stage_selected_source(&selected, "document", private.path()).unwrap();
    let staged = scoped.source_path.unwrap();
    assert_eq!(fs::read(&staged).unwrap(), b"selected");
    assert_eq!(scoped.read_root.as_deref(), staged.parent());
    assert!(!private.path().join("source/private-notes.txt").exists());
}

#[test]
fn oversized_primary_document_is_staged_under_the_pdf_budget() {
    let selected_dir = tempfile::tempdir().unwrap();
    let selected = selected_dir.path().join("scanned.pdf");
    // Above the 32 MiB LaTeX-closure per-file cap; well below the 256 MiB
    // PDF staging budget.
    fs::write(&selected, vec![0u8; 33 * 1024 * 1024]).unwrap();
    let private = tempfile::tempdir().unwrap();

    let scoped = stage_selected_source(&selected, "document", private.path()).unwrap();
    let staged = scoped.source_path.unwrap();
    assert_eq!(
        fs::metadata(&staged).unwrap().len(),
        33 * 1024 * 1024,
        "primary document must stage completely"
    );
}

#[test]
fn oversized_referenced_source_file_is_skipped_with_a_staging_note() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("main.tex"),
        "\\documentclass{article}\n\\includegraphics{big.png}\n",
    )
    .unwrap();
    fs::write(project.path().join("big.png"), vec![0u8; 33 * 1024 * 1024]).unwrap();
    let private = tempfile::tempdir().unwrap();

    let scoped = stage_selected_source(project.path(), "document", private.path()).unwrap();
    let staged_root = scoped.source_path.unwrap();
    assert!(staged_root.join("main.tex").is_file());
    assert!(!staged_root.join("big.png").exists());
    let notes = fs::read_to_string(staged_root.join(super::latex::STAGING_NOTES_FILE)).unwrap();
    assert!(notes.contains("big.png"));
    assert!(notes.contains("32 MB per-file staging limit"));
}

#[test]
fn latex_root_selection_ignores_commented_documentclass_commands() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("main.tex"),
        "% \\documentclass{article}\n\\input{paper}\n",
    )
    .unwrap();
    fs::write(
        project.path().join("article.tex"),
        "\\documentclass{article}\n\\begin{document}Paper\\end{document}\n",
    )
    .unwrap();

    assert_eq!(
        find_main_tex(project.path()),
        Some(project.path().join("article.tex")),
    );
}

#[test]
fn latex_source_staging_copies_only_the_bounded_dependency_closure() {
    let parent = tempfile::tempdir().unwrap();
    let project = parent.path().join("paper");
    fs::create_dir_all(project.join("sections")).unwrap();
    fs::create_dir_all(project.join("figures")).unwrap();
    fs::write(
        project.join("main.tex"),
        "\\documentclass{localclass}\n\\input{sections/model}\n\
             \\graphicspath{{figures/}}\n\\includegraphics{irf}\n\\bibliography{refs}\n\
             % \\input{unused}\n\\input{../secret}\n",
    )
    .unwrap();
    fs::write(
        project.join("sections/model.tex"),
        "\\input{details}\nModel.",
    )
    .unwrap();
    fs::write(project.join("sections/details.tex"), "Details.").unwrap();
    fs::write(project.join("figures/irf.png"), b"image").unwrap();
    fs::write(project.join("refs.bib"), "@article{x}").unwrap();
    fs::write(project.join("localclass.cls"), "\\ProvidesClass{x}").unwrap();
    fs::write(project.join("unused.tex"), "not selected").unwrap();
    fs::write(parent.path().join("secret.tex"), "referenced sibling").unwrap();
    fs::write(parent.path().join("unreferenced.tex"), "never referenced").unwrap();
    let private = tempfile::tempdir().unwrap();

    let scoped = stage_selected_source(&project, "document", private.path()).unwrap();
    let staged_root = scoped.source_path.unwrap();
    assert!(staged_root.is_dir());
    for relative in [
        "main.tex",
        "sections/model.tex",
        "sections/details.tex",
        "figures/irf.png",
        "refs.bib",
        "localclass.cls",
    ] {
        assert!(
            staged_root.join(relative).is_file(),
            "{relative} was omitted"
        );
    }
    assert!(!staged_root.join("unused.tex").exists());
    // An explicitly referenced nearby external file is staged into the
    // private view under _external; unreferenced siblings never are.
    assert_eq!(
        fs::read(staged_root.join("_external/up1/secret.tex")).unwrap(),
        b"referenced sibling"
    );
    assert!(!staged_root.join("secret.tex").exists());
    assert!(!staged_root.join("_external/up1/unreferenced.tex").exists());
    assert_eq!(scoped.read_root.as_deref(), Some(staged_root.as_path()));
}

#[test]
fn latex_extraction_inlines_explicitly_referenced_nearby_external_files() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().join("draft").join("dev");
    let output = workspace.path().join("draft").join("output");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&output).unwrap();
    fs::write(
        root.join("main.tex"),
        "Intro\n\\input{../output/numbers}\nAfter\n\\input{../output/numbers}\n",
    )
    .unwrap();
    fs::write(output.join("numbers.tex"), "External numbers.").unwrap();

    let mut warnings = Vec::new();
    let canonical_root = root.canonicalize().unwrap();
    let text = extract_latex(&root.join("main.tex"), &canonical_root, &mut warnings).unwrap();
    assert!(text.contains("External numbers."));
    assert!(!text.contains("\\input{../output/numbers}"));
    // The quality note is recorded once even though the file is included twice.
    assert_eq!(
        warnings
            .iter()
            .filter(|note| note.contains("read from outside the selected directory"))
            .count(),
        1,
        "expected one deduplicated external-read note: {warnings:?}"
    );
}

#[test]
fn latex_extraction_blocks_distant_or_untyped_external_references() {
    let workspace = tempfile::tempdir().unwrap();
    let root = workspace.path().join("l1/l2/l3/l4");
    fs::create_dir_all(&root).unwrap();
    fs::write(workspace.path().join("far.tex"), "Too far.").unwrap();
    fs::write(workspace.path().join("l1/l2/l3/data.txt"), "not tex").unwrap();
    fs::write(
        root.join("main.tex"),
        "\\input{../../../../far}\n\\input{../data.txt}\n\\input{../data.txt}\n",
    )
    .unwrap();

    let mut warnings = Vec::new();
    let canonical_root = root.canonicalize().unwrap();
    let text = extract_latex(&root.join("main.tex"), &canonical_root, &mut warnings).unwrap();
    assert!(text.contains("\\input{../../../../far}"));
    assert!(text.contains("\\input{../data.txt}"));
    assert!(!text.contains("Too far."));
    assert!(!text.contains("not tex"));
    let blocked = warnings
        .iter()
        .filter(|note| note.contains("blocked"))
        .count();
    assert_eq!(
        blocked, 2,
        "expected one deduplicated note per blocked target: {warnings:?}"
    );
}

#[test]
fn latex_extraction_skips_circular_includes_precisely() {
    let project = tempfile::tempdir().unwrap();
    fs::write(project.path().join("a.tex"), "A begins\n\\input{b}\nA ends\n").unwrap();
    fs::write(project.path().join("b.tex"), "B begins\n\\input{a}\nB ends\n").unwrap();

    let mut warnings = Vec::new();
    let root = project.path().canonicalize().unwrap();
    let text = extract_latex(&project.path().join("a.tex"), &root, &mut warnings).unwrap();
    assert!(text.contains("A begins"));
    assert!(text.contains("B begins"));
    assert!(text.contains("B ends"));
    assert!(text.contains("A ends"));
    assert_eq!(
        warnings,
        vec!["\\input{a} — circular include skipped.".to_string()]
    );
}

#[test]
fn latex_extraction_ignores_commented_out_includes() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("main.tex"),
        "Current text\n% \\input{old_draft}\nEscaped percent \\% then \\input{real}\n",
    )
    .unwrap();
    fs::write(project.path().join("old_draft.tex"), "STALE DRAFT\n").unwrap();
    fs::write(project.path().join("real.tex"), "REAL SECTION\n").unwrap();

    let mut warnings = Vec::new();
    let root = project.path().canonicalize().unwrap();
    let text = extract_latex(&project.path().join("main.tex"), &root, &mut warnings).unwrap();
    assert!(!text.contains("STALE DRAFT"));
    assert!(text.contains("% \\input{old_draft}"));
    assert!(text.contains("REAL SECTION"));
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
}

#[test]
fn latex_extraction_notes_unreadable_includes() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("main.tex"),
        "Before\n\\input{chapter}\nAfter\n",
    )
    .unwrap();
    // Invalid UTF-8 makes the include unreadable without being missing.
    fs::write(project.path().join("chapter.tex"), [0xFFu8, 0xFE, 0x00, 0x41]).unwrap();

    let mut warnings = Vec::new();
    let root = project.path().canonicalize().unwrap();
    let text = extract_latex(&project.path().join("main.tex"), &root, &mut warnings).unwrap();
    assert!(text.contains("\\input{chapter}"));
    assert!(
        warnings
            .iter()
            .any(|note| note.contains("\\input{chapter}") && note.contains("could not be read")),
        "expected an unreadable-include note: {warnings:?}"
    );
}

#[test]
fn paddle_readiness_requires_the_managed_model_alias() {
    let expected = serde_json::json!({
        "data": [{"id": PADDLE_MODEL_ALIAS, "object": "model"}]
    });
    let wrong = serde_json::json!({
        "data": [{"id": "unrelated-server", "object": "model"}]
    });
    assert!(paddle_model_list_has_alias(&expected));
    assert!(!paddle_model_list_has_alias(&wrong));
    assert!(!paddle_model_list_has_alias(&serde_json::json!({})));
}

#[test]
fn paddle_readiness_polling_has_a_bounded_backoff() {
    assert!(PADDLE_READINESS_POLL_INTERVAL >= std::time::Duration::from_millis(100));
    assert!(PADDLE_READINESS_POLL_INTERVAL <= std::time::Duration::from_millis(250));
}

#[test]
fn paddle_server_credentials_are_high_entropy_hex() {
    let token = paddle_server_token().unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn full_parser_structure_drives_compatibility_text_and_page_verification() {
    let structure = PaddleStructure {
        schema_version: PADDLE_STRUCTURE_SCHEMA,
        parser: "paddleocr-vl-full".to_string(),
        parser_version: "3.7.0".to_string(),
        settings: serde_json::json!({ "merge_tables": true }),
        quality_notes: vec!["fixture note".to_string()],
        pages: vec![
            PaddleStructuredPage {
                number: 1,
                markdown: "# Introduction\n\n![Plot](assets/page-0001-plot.png)".to_string(),
                source_text_chars: None,
                width: Some(1200),
                height: Some(1600),
                blocks: Vec::new(),
            },
            PaddleStructuredPage {
                number: 2,
                markdown: "## Results\n\nThe estimate is positive.".to_string(),
                source_text_chars: None,
                width: Some(1200),
                height: Some(1600),
                blocks: Vec::new(),
            },
        ],
    };
    let path = Path::new("/tmp/paper.pdf");
    let extraction =
        full_parser_extraction_from_structure(&structure, path, "0123456789abcdef", Some(&[0, 0]))
            .unwrap();
    assert_eq!(extraction.method, "paddleocr-vl-full");
    assert!(extraction.text.contains("<!-- PAGE 2 -->"));
    assert!(extraction
        .text
        .contains("../artifacts/figures/page-0001-plot.png"));
    assert!(extraction
        .quality_notes
        .contains(&"fixture note".to_string()));

    let mut moved_page = structure.clone();
    moved_page.pages[0].markdown.clear();
    moved_page.pages[0].source_text_chars = Some(1_000);
    let moved_extraction = full_parser_extraction_from_structure(
        &moved_page,
        path,
        "0123456789abcdef",
        Some(&[1_000, 0]),
    )
    .unwrap();
    assert!(moved_extraction
        .quality_notes
        .iter()
        .any(|note| note.contains("moved all recognized content from page(s) 1")));

    let mut incomplete_page = structure.clone();
    incomplete_page.pages[0].source_text_chars = Some(10);
    let incomplete_error = full_parser_extraction_from_structure(
        &incomplete_page,
        path,
        "0123456789abcdef",
        Some(&[1_000, 0]),
    )
    .unwrap_err();
    assert!(incomplete_error.contains("recognized 10 substantive characters"));

    let mut missing_page = structure;
    missing_page.pages[1].number = 3;
    assert!(full_parser_extraction_from_structure(
        &missing_page,
        path,
        "0123456789abcdef",
        Some(&[0, 0]),
    )
    .unwrap_err()
    .contains("page 3 where page 2 was expected"));
}

#[test]
fn paddle_diagnostics_remove_terminal_codes_and_expected_dependency_warnings() {
    let stderr = concat!(
            "\u{1b}[32mCreating model: ('PP-DocLayoutV3', None, None)\u{1b}[0m\n",
            "/managed/extension_utils.py:718: UserWarning: No ccache found.\n",
            "warnings.warn(warning_message)\n",
            "/managed/predictor.py:545: UserWarning: 'llama-cpp-server' does not support `min_pixels`.\n",
            "warnings.warn(\n",
            "Page 39: retrying incomplete layout recognition.\n",
            "Page 39: retrying incomplete layout recognition.\n",
        );
    assert_eq!(
        paddle_diagnostic_lines(stderr, 50),
        vec![
            "Creating model: ('PP-DocLayoutV3', None, None)",
            "Page 39: retrying incomplete layout recognition.",
        ]
    );
}

#[test]
fn full_parser_image_labels_use_captions_and_disambiguate_repeated_numbers() {
    let block = |id: &str, text: &str, order: u32, assets: &[&str]| PaddleStructuredBlock {
        block_id: id.to_string(),
        role: if assets.is_empty() {
            "figure_title"
        } else {
            "chart"
        }
        .to_string(),
        block_label: if assets.is_empty() {
            "figure_title"
        } else {
            "chart"
        }
        .to_string(),
        markdown: text.to_string(),
        text: text.to_string(),
        boundary: None,
        note_marker: None,
        order: Some(order),
        bbox: Vec::new(),
        polygon: Vec::new(),
        confidence: None,
        asset_files: assets.iter().map(|asset| asset.to_string()).collect(),
        raw: serde_json::Value::Null,
    };
    let page = |number: u32, blocks: Vec<PaddleStructuredBlock>| PaddleStructuredPage {
        number,
        markdown: String::new(),
        source_text_chars: None,
        width: None,
        height: None,
        blocks,
    };
    let structure = PaddleStructure {
        schema_version: PADDLE_STRUCTURE_SCHEMA,
        parser: "paddleocr-vl-full".to_string(),
        parser_version: "3.7.0".to_string(),
        settings: serde_json::Value::Null,
        quality_notes: Vec::new(),
        pages: vec![
            page(
                2,
                vec![
                    block("caption-1a", "Figure 1: Baseline", 1, &[]),
                    block("image-1a", "", 2, &["assets/page-0002-plot.jpg"]),
                ],
            ),
            page(
                3,
                vec![
                    block("caption-1b", "Figure 1: Appendix version", 1, &[]),
                    block("image-1b", "", 2, &["assets/page-0003-plot.jpg"]),
                ],
            ),
            page(
                4,
                vec![
                    block("caption-2", "Figure 2: Responses", 1, &[]),
                    block(
                        "image-2",
                        "",
                        2,
                        &[
                            "assets/page-0004-panel-a.png",
                            "assets/page-0004-panel-b.png",
                        ],
                    ),
                ],
            ),
            page(
                5,
                vec![
                    PaddleStructuredBlock {
                        role: "text".to_string(),
                        block_label: "text".to_string(),
                        ..block(
                            "prose-reference",
                            "Figure 9 discusses an earlier result.",
                            1,
                            &[],
                        )
                    },
                    block(
                        "image-before-caption",
                        "",
                        2,
                        &["assets/page-0005-plot.webp"],
                    ),
                    block("caption-below", "Figure 3: Caption below image", 3, &[]),
                ],
            ),
        ],
    };
    let images = [
        "/cache/page-0002-plot.jpg",
        "/cache/page-0003-plot.jpg",
        "/cache/page-0004-panel-a.png",
        "/cache/page-0004-panel-b.png",
        "/cache/page-0005-plot.webp",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    let inventory = paddle_full_image_inventory_from(&structure, images);
    assert_eq!(inventory.disambiguated_labels, 1);
    assert_eq!(
        inventory
            .artifacts
            .iter()
            .map(|artifact| artifact.display_name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "figure_1_page_0002.jpg",
            "figure_1_page_0003.jpg",
            "figure_2_1.png",
            "figure_2_2.png",
            "figure_3.webp",
        ]
    );
}

#[test]
fn full_parser_uses_richer_blocks_when_page_markdown_is_partial() {
    let page = PaddleStructuredPage {
        number: 1,
        markdown: "References".to_string(),
        source_text_chars: None,
        width: None,
        height: None,
        blocks: vec![PaddleStructuredBlock {
            block_id: "reference-block".to_string(),
            role: "reference_content".to_string(),
            block_label: "reference_content".to_string(),
            markdown: "A complete bibliography entry with substantially more recognized text."
                .repeat(8),
            text: String::new(),
            boundary: None,
            note_marker: None,
            order: Some(1),
            bbox: Vec::new(),
            polygon: Vec::new(),
            confidence: Some(0.9),
            asset_files: Vec::new(),
            raw: serde_json::Value::Null,
        }],
    };
    let rendered = render_paddle_full_page(&page);
    assert!(rendered.contains("complete bibliography entry"));
    assert!(rendered.len() > page.markdown.len() * 2);
}

#[test]
fn rendered_page_limit_matches_documented_cap() {
    assert_eq!(rendered_page_limit(0), 1);
    assert_eq!(rendered_page_limit(50), 50);
    assert_eq!(rendered_page_limit(500), MAX_RENDERED_PDF_PAGES);
    assert_eq!(MAX_RENDERED_PDF_PAGES, 300);
}

#[test]
fn pdf_artifact_preview_renders_a_jpeg_when_poppler_is_available() {
    if find_command("pdftoppm").is_none() {
        return;
    }
    let stream = "BT /F1 18 Tf 72 720 Td (Pipeline PDF preview) Tj ET\n";
    let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
    let mut pdf = "%PDF-1.4\n".to_string();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));

    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("figure.pdf");
    let output = temp.path().join("rendered");
    fs::write(&input, pdf).unwrap();
    let preview = render_pdf_page_preview(&input, &output, 1).unwrap();
    assert!(!preview.has_next);
    assert!(preview.next_name.is_none());
    assert!(!preview.next_has_next);
    let jpeg = fs::read(output.join(preview.name)).unwrap();
    assert!(jpeg.starts_with(&[0xff, 0xd8, 0xff]));
}

#[test]
#[cfg(unix)]
fn bounded_subprocess_times_out_promptly() {
    let mut command = StdCommand::new("sleep");
    command.arg("5");
    let started = std::time::Instant::now();
    let error = run_bounded_output(
        command,
        "sleep test",
        std::time::Duration::from_millis(20),
        1024,
        None,
    )
    .unwrap_err();
    assert!(error.contains("timed out"));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn file_hash_is_streamed_and_matches_sha256() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut file, b"abc").unwrap();
    assert_eq!(compute_hash(file.path()).unwrap(), "ba7816bf8f01cfea");
}

// ── scan_math_quality ──────────────────────────────────────────

#[test]
fn scan_clean_text_no_warnings() {
    let text = "This is a normal academic paper with standard English text. \
                    The results are reported in Table 1. We find a significant effect.";
    let notes = scan_math_quality(text);
    assert!(notes.is_empty());
}

#[test]
fn scan_short_text_skipped() {
    let notes = scan_math_quality("short");
    assert!(notes.is_empty());
}

#[test]
fn scan_isolated_math_symbols() {
    // Simulate garbled PDF extraction with many isolated non-ASCII chars
    let mut text = "Normal text here. ".repeat(10);
    for _ in 0..50 {
        text.push_str("β ");
    }
    let notes = scan_math_quality(&text);
    assert!(!notes.is_empty());
    assert!(notes[0].contains("isolated math symbols"));
}

#[test]
fn scan_math_unicode_runs() {
    // 6+ runs of 3+ consecutive math Unicode characters
    let mut text = "Normal text. ".repeat(20);
    for _ in 0..8 {
        text.push_str("∀∈≥ normal text ");
    }
    let notes = scan_math_quality(&text);
    assert!(notes.iter().any(|n| n.contains("clusters")));
}

#[test]
fn scan_ligature_artifacts() {
    let mut text = "Normal text. ".repeat(20);
    // 15 fi ligatures
    for _ in 0..15 {
        text.push_str("the \u{FB01}rst ");
    }
    let notes = scan_math_quality(&text);
    assert!(notes.iter().any(|n| n.contains("ligature")));
}

// ── LLM extraction verification helpers ───────────────────────

#[test]
fn baseline_splits_on_form_feeds_and_drops_trailing_empty() {
    let text = "page   one text\u{0C}page two\u{0C}";
    let pages = baseline_page_lengths(text);
    assert_eq!(pages, vec!["pageonetext".len(), "pagetwo".len()]);
}

#[test]
fn parse_sections_returns_none_without_markers() {
    assert!(parse_page_sections("just some markdown, no markers").is_none());
}

#[test]
fn parse_sections_splits_preamble_and_pages() {
    let text = "intro\n<!-- PAGE 1 -->\nfirst page\n<!-- page 2 -->\nsecond page";
    let (preamble, sections) = parse_page_sections(text).unwrap();
    assert_eq!(preamble, "intro");
    assert_eq!(sections.len(), 2);
    assert!(sections[&1].contains("first page"));
    // Page-delimiter matching is case-insensitive.
    assert!(sections[&2].contains("second page"));
}

#[test]
fn parse_sections_appends_repeated_markers() {
    let text = "<!-- PAGE 1 -->\nstart\n<!-- PAGE 1 -->\ncontinued";
    let (_, sections) = parse_page_sections(text).unwrap();
    assert!(sections[&1].contains("start"));
    assert!(sections[&1].contains("continued"));
}

#[test]
fn suspects_flags_missing_and_short_pages() {
    let mut sections = std::collections::BTreeMap::new();
    sections.insert(1, "x".repeat(500));
    sections.insert(3, "tiny".to_string());
    sections.insert(4, "y".repeat(50));
    // page 2 missing; page 3 short vs a 1000-char baseline;
    // page 4 short but baseline below the 200-char floor → not suspect.
    let baseline = vec![500, 800, 1000, 150];
    assert_eq!(find_suspect_pages(&sections, &baseline), vec![2, 3]);
}

#[test]
fn ranges_group_contiguous_pages() {
    assert_eq!(
        group_into_ranges(&[2, 3, 4, 7, 9, 10]),
        vec![(2, 4), (7, 7), (9, 10)]
    );
    assert!(group_into_ranges(&[]).is_empty());
}

#[test]
fn llm_initial_ranges_bound_pages_and_expected_output() {
    assert_eq!(
        llm_initial_ranges(&[1_000; 20]),
        vec![(1, 8), (9, 16), (17, 20)]
    );
    assert_eq!(
        llm_initial_ranges(&[30_000, 30_000, 1_000]),
        vec![(1, 1), (2, 3)]
    );
}

#[test]
fn retry_ranges_are_split_into_small_requests() {
    assert_eq!(
        split_ranges(&[(2, 6), (10, 10)], 2),
        vec![(2, 3), (4, 5), (6, 6), (10, 10)]
    );
}

#[test]
fn extraction_prompt_preserves_typos_as_evidence() {
    assert!(extraction_requirements().contains("typographical errors verbatim"));
}

#[test]
fn rebuild_orders_pages_and_keeps_preamble() {
    let mut sections = std::collections::BTreeMap::new();
    sections.insert(2, "two".to_string());
    sections.insert(1, "one".to_string());
    let out = rebuild_from_sections("title", &sections);
    assert!(out.starts_with("title"));
    let one_pos = out.find("<!-- PAGE 1 -->").unwrap();
    let two_pos = out.find("<!-- PAGE 2 -->").unwrap();
    assert!(one_pos < two_pos);
    assert!(out.contains("one") && out.contains("two"));
}

#[test]
fn fence_stripping() {
    assert_eq!(strip_markdown_fence("```markdown\n# Title\n```"), "# Title");
    assert_eq!(strip_markdown_fence("```\ntext\n```"), "text");
    // Not a wrapping fence — inner fences stay untouched.
    let mixed = "prose\n```python\ncode\n```\nmore";
    assert_eq!(strip_markdown_fence(mixed), mixed);
    assert_eq!(strip_markdown_fence("plain"), "plain");
}
