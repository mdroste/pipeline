use super::*;

#[test]
fn converters_have_fixed_safe_arguments_and_support_space_paths() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile with spaces");
    let args = tools::office_args(
        &root.path().join("deck with spaces.pptx"),
        root.path(),
        &profile,
    )
    .unwrap();
    assert!(args[0]
        .to_string_lossy()
        .contains("profile%20with%20spaces"));
    assert!(args
        .iter()
        .any(|arg| arg.to_string_lossy().contains("ExportHiddenSlides")));
    let args = tools::tex_args("lualatex", std::ffi::OsStr::new("-talk.tex"), root.path());
    assert!(args.contains(&"-no-shell-escape".into()));
    assert!(args.contains(&"--nosocket".into()));
    assert_eq!(
        args.last().unwrap(),
        &Path::new(".").join("-talk.tex").into_os_string()
    );
    assert_eq!(
        tools::tex_engine("% !TeX program = XeLaTeX\n\\documentclass{beamer}").unwrap(),
        "xelatex"
    );
    assert_eq!(
        tools::tex_engine("\\usepackage{fontspec}").unwrap(),
        "xelatex"
    );
    assert!(tools::tex_engine("% !TeX program = arbitrary-command").is_err());
}

#[test]
fn copied_beamer_references_use_only_the_captured_tree() {
    let live = tempfile::tempdir().unwrap();
    let project = live.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(
        project.join("talk.tex"),
        "\\documentclass{beamer}\n\\usetheme{Local}\n\\input{../result}\n",
    )
    .unwrap();
    fs::write(project.join("beamerthemeLocal.sty"), "% local theme\n").unwrap();
    fs::write(
        live.path().join("result.tex"),
        "\\begin{frame}Captured result\\end{frame}",
    )
    .unwrap();
    let captured = tempfile::tempdir().unwrap();
    latex::stage_latex_project(&project.join("talk.tex"), &project, captured.path()).unwrap();
    assert!(captured.path().join("beamerthemeLocal.sty").is_file());
    fs::write(live.path().join("result.tex"), "CHANGED").unwrap();
    let build = tempfile::tempdir().unwrap();
    let destination = build.path().join("source");
    latex_source::copy_project(captured.path(), &destination).unwrap();
    latex_source::rebase_references(captured.path(), &destination).unwrap();
    let source = fs::read_to_string(destination.join("talk.tex")).unwrap();
    assert!(
        source.contains("\\input{_external/up1/result.tex}"),
        "{source}"
    );
    assert!(
        fs::read_to_string(destination.join("_external/up1/result.tex"))
            .unwrap()
            .contains("Captured result")
    );
    assert!(fs::read_to_string(project.join("talk.tex"))
        .unwrap()
        .contains("../result"));
}

#[test]
fn renderer_rejects_invalid_outputs_and_reports_failure_without_a_pdf() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("bad.pdf");
    fs::write(&pdf, "%PDF-1.7\nnot a complete PDF").unwrap();
    assert!(validate_pdf(&pdf).is_err());
    let source = temp.path().join("talk.tex");
    fs::write(
        &source,
        "% !TeX program = forbidden\n\\documentclass{beamer}",
    )
    .unwrap();
    let extraction = ExtractionResult {
        text: "\\documentclass{beamer}".into(),
        method: "latex".into(),
        source_path: source.to_string_lossy().into_owned(),
        paper_hash: "fixture".into(),
        quality_notes: vec![],
    };
    let output = tempfile::tempdir().unwrap();
    let rendered = render_slide_pdf(&extraction, output.path()).unwrap();
    assert!(rendered.pdf.is_none());
    assert!(rendered.note.contains("Unsupported TeX engine"));
    assert!(rendered.log.is_file());
}

#[test]
#[ignore = "requires installed TeX engines and Poppler"]
fn real_beamer_compiles_overlays_and_renders_page_images() {
    for engine in ["pdflatex", "xelatex", "lualatex"] {
        compile_beamer_fixture(engine);
    }
}

fn compile_beamer_fixture(engine: &str) {
    let live = tempfile::tempdir().unwrap();
    let source = live.path().join("talk.tex");
    fs::write(
        &source,
        format!("% !TeX program = {engine}\n")
            + r"\documentclass{beamer}
\begin{document}
\begin{frame}{Model}$y=\alpha x$\pause\par Second overlay\end{frame}
\input{result}
\end{document}",
    )
    .unwrap();
    fs::write(
        live.path().join("result.tex"),
        r"\begin{frame}{Result}A result\end{frame}",
    )
    .unwrap();
    let captured = tempfile::tempdir().unwrap();
    let scoped = stage_selected_source(&source, "document", captured.path()).unwrap();
    let main = find_main_tex(&scoped.source_path.unwrap()).unwrap();
    let extraction = ExtractionResult {
        text: "\\documentclass{beamer}".into(),
        method: "latex".into(),
        source_path: main.to_string_lossy().into_owned(),
        paper_hash: "fixture".into(),
        quality_notes: vec![],
    };
    let output = tempfile::tempdir().unwrap();
    let rendered = render_slide_pdf(&extraction, output.path()).unwrap();
    let pdf = rendered.pdf.unwrap_or_else(|| {
        panic!(
            "{}\n{}",
            rendered.note,
            fs::read_to_string(&rendered.log).unwrap()
        )
    });
    let document = lopdf::Document::load(&pdf).unwrap();
    assert_eq!(document.get_pages().len(), 3);
    let pages = render_pdf_pages(&pdf, &output.path().join("pages"), 20).unwrap();
    assert_eq!(pages.names.len(), 3);
    assert!(!source.with_extension("pdf").exists());
    retain_probe(&pdf, &format!("beamer-{engine}"));
}

#[test]
#[cfg(not(windows))]
#[ignore = "requires a local TeX distribution and OS sandbox"]
fn real_beamer_cannot_read_a_file_outside_the_build_copy() {
    let live = tempfile::tempdir().unwrap();
    let secret = live.path().join("unrelated.tex");
    fs::write(&secret, "UNRELATED PRIVATE CONTENT").unwrap();
    let source = live.path().join("talk.tex");
    let secret_path = secret.to_string_lossy().into_owned();
    // Engine read filtering is disabled for system fonts; check the OS boundary.
    fs::write(&source, format!(r#"% !TeX program = lualatex
\documentclass{{beamer}}
\begin{{document}}\begin{{frame}}
\directlua{{local f = io.open("{}", "r"); if f then tex.print(f:read("*a")); f:close() else error("PIPELINE_READ_DENIED") end}}
\end{{frame}}\end{{document}}"#, secret_path)).unwrap();
    let captured = tempfile::tempdir().unwrap();
    let scoped = stage_selected_source(&source, "document", captured.path()).unwrap();
    let main = find_main_tex(&scoped.source_path.unwrap()).unwrap();
    let extraction = ExtractionResult {
        text: "\\documentclass{beamer}".into(),
        method: "latex".into(),
        source_path: main.to_string_lossy().into_owned(),
        paper_hash: "fixture".into(),
        quality_notes: vec![],
    };
    let output = tempfile::tempdir().unwrap();
    let rendered = render_slide_pdf(&extraction, output.path()).unwrap();
    assert!(rendered.pdf.is_none(), "TeX read outside the staged source");
    let log = fs::read_to_string(rendered.log).unwrap();
    assert!(log.contains("PIPELINE_READ_DENIED"), "{log}");
    assert!(!log.contains("UNRELATED PRIVATE CONTENT"));
}

#[test]
#[ignore = "requires local LibreOffice and Poppler"]
fn real_powerpoint_renders_hidden_slides_in_native_order() {
    let input = tempfile::tempdir().unwrap();
    let source = input.path().join("deck with spaces.pptx");
    fs::write(
        &source,
        include_bytes!("../../../../tests/fixtures/slides/render.pptx"),
    )
    .unwrap();
    let extraction = document::extract_pptx(&source).unwrap();
    let output = tempfile::tempdir().unwrap();
    let rendered = render_slide_pdf(&extraction, output.path()).unwrap();
    let pdf = rendered.pdf.unwrap_or_else(|| {
        panic!(
            "{}\n{}",
            rendered.note,
            fs::read_to_string(&rendered.log).unwrap()
        )
    });
    assert_eq!(lopdf::Document::load(&pdf).unwrap().get_pages().len(), 2);
    let pages = render_pdf_pages(&pdf, &output.path().join("pages"), 20).unwrap();
    assert_eq!(pages.names.len(), 2);
    let text = extract_pdftotext(&pdf).unwrap();
    assert!(text.find("Model").unwrap() < text.find("Hidden robustness slide").unwrap());
    assert!(!text.contains("Speaker notes are not visible"));
    assert!(!source.with_extension("pdf").exists());
    retain_probe(&pdf, "powerpoint");
}

fn retain_probe(pdf: &Path, name: &str) {
    if let Some(root) = std::env::var_os("PIPELINE_SLIDE_TEST_ARTIFACTS") {
        let root = PathBuf::from(root);
        fs::create_dir_all(&root).unwrap();
        fs::copy(pdf, root.join(format!("{name}.pdf"))).unwrap();
        render_pdf_pages(pdf, &root.join(name), 20).unwrap();
    }
}

#[test]
#[cfg(unix)]
fn converter_stops_on_the_review_deadline_and_on_cancellation() {
    let active = crate::run_scope::ActiveScope::begin("slide-budget-test");
    crate::run_scope::enter_blocking(active.scope().clone(), || {
        let settings = crate::settings::Settings {
            review_max_minutes: 1,
            ..Default::default()
        };
        let _budget = crate::pipeline::review_budget::begin(
            &settings,
            Instant::now() - Duration::from_millis(59_800),
        );
        let mut command = StdCommand::new("/bin/sh");
        command.args(["-c", "sleep 5"]);
        let output = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let error = run_converter(
            command,
            "test converter",
            output.path(),
            started,
            &mut String::new(),
        )
        .unwrap_err();
        assert!(crate::pipeline::review_budget::stopped(&error), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
    });
    let active = crate::run_scope::ActiveScope::begin("slide-cancel-test");
    active.scope().cancel();
    crate::run_scope::enter_blocking(active.scope().clone(), || {
        let output = tempfile::tempdir().unwrap();
        let error = run_converter(
            StdCommand::new("not-a-real-executable"),
            "test converter",
            output.path(),
            Instant::now(),
            &mut String::new(),
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
    });
}
