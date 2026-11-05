//! Artifacts regression coverage.

use super::*;

#[test]
fn selected_artifacts_are_staged_without_exposing_unselected_inputs() {
    let runtime_dir = tempfile::tempdir().unwrap();
    let paper = runtime_dir.path().join("paper.md");
    let survey = runtime_dir.path().join("survey.json");
    let named = runtime_dir.path().join("rubric.md");
    let source_dir = runtime_dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(&paper, "paper").unwrap();
    std::fs::write(&survey, "{}").unwrap();
    std::fs::write(&named, "rubric").unwrap();
    std::fs::write(source_dir.join("main.tex"), "source").unwrap();

    let mut step = make_step("reader", Phase::Parallel);
    step.context.include = vec![
        ArtifactSelector::Primary {
            parts: vec![PrimaryArtifactPart::Text],
        },
        ArtifactSelector::NamedInput {
            key: "rubric".into(),
            parts: vec![NamedInputArtifactPart::Text],
        },
    ];
    let named_inputs = std::collections::HashMap::from([(
        "rubric".to_string(),
        named.to_string_lossy().to_string(),
    )]);
    let named_sources = std::collections::HashMap::from([(
        "rubric".to_string(),
        source_dir.to_string_lossy().to_string(),
    )]);
    let resolved = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: survey.to_str().unwrap(),
            paper_text_path: paper.to_str().unwrap(),
            document_bundle_path: "",
            source_path: source_dir.to_str().unwrap(),
            extra_inputs: &named_inputs,
            extra_input_sources: &named_sources,
            outputs: &[],
            run_artifact_dir: None,
            context_pool: &Default::default(),
        },
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&resolved.paper_text_path).unwrap(),
        "paper"
    );
    assert_eq!(
        std::fs::read_to_string(resolved.extra_inputs.get("rubric").unwrap()).unwrap(),
        "rubric"
    );
    assert!(resolved.orientation_path.is_empty());
    assert!(resolved.source_path.is_empty());
    assert!(!resolved
        .read_dirs
        .iter()
        .any(|root| root == &normalized_path(&source_dir)));
}

#[test]
fn selected_source_folder_grants_only_that_folder() {
    let folder = tempfile::tempdir().unwrap();
    let source_dir = folder.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = normalized_path(&source_dir);
    let parent = normalized_path(folder.path());
    let mut step = make_step("source-reader", Phase::Parallel);
    step.context.include = vec![ArtifactSelector::Primary {
        parts: vec![PrimaryArtifactPart::Source],
    }];

    let resolved = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: source_dir.to_str().unwrap(),
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: None,
            context_pool: &Default::default(),
        },
    )
    .unwrap();

    assert!(resolved.read_dirs.iter().any(|root| root == &expected));
    assert!(!resolved.read_dirs.iter().any(|root| root == &parent));
}

#[test]
fn supporting_file_glob_stages_only_matching_producer_files() {
    let run = tempfile::tempdir().unwrap();
    let artifacts = run.path().join("artifacts");
    let files = artifacts
        .join("by-step")
        .join(step_slug("producer"))
        .join(step_slug("producer"))
        .join("files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("selected.csv"), "selected").unwrap();
    std::fs::write(files.join("unselected.txt"), "unselected").unwrap();

    let mut step = make_step("consumer", Phase::Sequential);
    step.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Files],
        glob: "*.csv".into(),
    }];
    let resolved = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: "",
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: Some(artifacts.to_str().unwrap()),
            context_pool: &Default::default(),
        },
    )
    .unwrap();

    assert!(resolved.manifest.contains("selected.csv"));
    assert!(!resolved.manifest.contains("unselected.txt"));
    assert!(resolved
        ._view
        .path()
        .join("steps")
        .join(step_slug("producer"))
        .join("files")
        .join("selected.csv")
        .is_file());
    assert!(!resolved
        ._view
        .path()
        .join("steps")
        .join(step_slug("producer"))
        .join("files")
        .join("unselected.txt")
        .exists());
}

#[test]
fn multi_unit_supporting_files_stage_into_per_unit_namespaces() {
    let run = tempfile::tempdir().unwrap();
    let artifacts = run.path().join("artifacts");
    let producer_root = artifacts.join("by-step").join(step_slug("producer"));
    let unit_a = step_slug("producer/claude");
    let unit_b = step_slug("producer/codex");
    for (unit, content) in [(&unit_a, "from claude"), (&unit_b, "from codex")] {
        let files = producer_root.join(unit).join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("table.csv"), content).unwrap();
    }

    let mut step = make_step("consumer", Phase::Sequential);
    step.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Files],
        glob: "*.csv".into(),
    }];
    let resolved = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: "",
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: Some(artifacts.to_str().unwrap()),
            context_pool: &Default::default(),
        },
    )
    .unwrap();

    let staged_root = resolved
        ._view
        .path()
        .join("steps")
        .join(step_slug("producer"));
    let staged_a = staged_root.join(&unit_a).join("files").join("table.csv");
    let staged_b = staged_root.join(&unit_b).join("files").join("table.csv");
    assert_eq!(std::fs::read_to_string(&staged_a).unwrap(), "from claude");
    assert_eq!(std::fs::read_to_string(&staged_b).unwrap(), "from codex");
    // Both staged paths are listed, each exactly once.
    assert_eq!(
        resolved
            .manifest
            .matches(&normalized_path(&staged_a))
            .count(),
        1
    );
    assert_eq!(
        resolved
            .manifest
            .matches(&normalized_path(&staged_b))
            .count(),
        1
    );
}

fn select_csv_files(count: usize) -> (String, Vec<String>) {
    let run = tempfile::tempdir().unwrap();
    let artifacts = run.path().join("artifacts");
    let files = artifacts
        .join("by-step")
        .join(step_slug("producer"))
        .join(step_slug("producer"))
        .join("files");
    std::fs::create_dir_all(&files).unwrap();
    for index in 0..count {
        std::fs::write(files.join(format!("{index:04}.csv")), "x").unwrap();
    }
    let mut step = make_step("consumer", Phase::Sequential);
    step.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Files],
        glob: "*.csv".into(),
    }];
    let resolved = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: "",
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: Some(artifacts.to_str().unwrap()),
            context_pool: &Default::default(),
        },
    )
    .unwrap();
    (resolved.manifest.clone(), resolved.warnings.clone())
}

#[test]
fn a_truncated_file_selection_says_so_in_the_manifest_and_the_log() {
    let (manifest, warnings) = select_csv_files(501);
    assert_eq!(manifest.matches("Supporting file from").count(), 500);
    assert!(
        manifest.contains("matched more than 500 files"),
        "{manifest}"
    );
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("'*.csv' matched more than 500 files"));

    let (manifest, warnings) = select_csv_files(500);
    assert!(!manifest.contains("matched more than"), "{manifest}");
    assert!(warnings.is_empty());
}

#[test]
fn parallel_artifact_resolution_rejects_step_outputs() {
    let mut step = make_step("parallel-consumer", Phase::Parallel);
    step.context.include = vec![ArtifactSelector::Step {
        step: "producer".into(),
        parts: vec![StepArtifactPart::Report],
        glob: String::new(),
    }];
    let error = resolve_artifact_context(
        &step,
        ArtifactRuntime {
            orientation_path: "",
            paper_text_path: "",
            document_bundle_path: "",
            source_path: "",
            extra_inputs: &Default::default(),
            extra_input_sources: &Default::default(),
            outputs: &[],
            run_artifact_dir: None,
            context_pool: &Default::default(),
        },
    )
    .err()
    .unwrap();
    assert!(error.contains("cannot consume another step's output"));
}

#[test]
fn ingest_report_file_reads_and_removes() {
    let dir = tempfile::tempdir().unwrap();
    let wd = dir.path().to_string_lossy().to_string();

    // Absent file → None.
    assert!(ingest_report_file_blocking(Some(&wd), "steps/a.md").is_none());
    // Disabled write dir → None.
    assert!(ingest_report_file_blocking(None, "steps/a.md").is_none());

    // Present file → content, and the file is consumed.
    std::fs::create_dir_all(dir.path().join("steps")).unwrap();
    std::fs::write(dir.path().join("steps/a.md"), "# Report\nbody\n").unwrap();
    assert_eq!(
        ingest_report_file_blocking(Some(&wd), "steps/a.md").unwrap(),
        "# Report\nbody"
    );
    assert!(!dir.path().join("steps/a.md").exists());

    // Empty file → None (falls back to stdout).
    std::fs::write(dir.path().join("steps/b.md"), "  \n").unwrap();
    assert!(ingest_report_file_blocking(Some(&wd), "steps/b.md").is_none());
}

#[test]
fn every_unit_shares_one_document_index() {
    use crate::document_bundle::{DocumentBundle, DocumentExtraction};
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bundle.json");
    let bundle = DocumentBundle {
        schema_version: crate::document_bundle::DOCUMENT_BUNDLE_SCHEMA_VERSION.into(),
        bundle_id: "bundle".into(),
        source_kind: "pdf".into(),
        origins: vec![],
        pages: vec![],
        nodes: vec![],
        assets: vec![],
        links: vec![],
        extraction: DocumentExtraction {
            method: "fixture".into(),
            source_path: "paper.pdf".into(),
            paper_hash: "hash".into(),
        },
        quality: vec![],
    };
    std::fs::write(&path, serde_json::to_string(&bundle).unwrap()).unwrap();
    let mut step = make_step("reader", Phase::Parallel);
    step.context.include = vec![ArtifactSelector::Primary {
        parts: vec![PrimaryArtifactPart::Structure],
    }];
    let pool = crate::pipeline::context_cache::PreparedContextPool::default();
    let runtime = |bundle: &'_ str| -> Result<_, String> {
        resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: bundle,
                source_path: "",
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: None,
                context_pool: &pool,
            },
        )
    };
    let first = runtime(path.to_str().unwrap()).unwrap();
    // Later units never read the bundle again, so its absence cannot matter.
    std::fs::remove_file(&path).unwrap();
    let second = runtime(path.to_str().unwrap()).unwrap();
    assert_ne!(first.document_bundle_path, second.document_bundle_path);
    assert_eq!(
        std::fs::read(&first.document_bundle_path).unwrap(),
        std::fs::read(&second.document_bundle_path).unwrap()
    );
    assert!(second.manifest.contains("document structure index"));

    // A bundle that cannot be read fails each unit in the same words.
    let broken = crate::pipeline::context_cache::PreparedContextPool::default();
    let failed = |pool| {
        resolve_artifact_context(
            &step,
            ArtifactRuntime {
                orientation_path: "",
                paper_text_path: "",
                document_bundle_path: "/missing/bundle.json",
                source_path: "",
                extra_inputs: &Default::default(),
                extra_input_sources: &Default::default(),
                outputs: &[],
                run_artifact_dir: None,
                context_pool: pool,
            },
        )
        .err()
        .unwrap()
    };
    assert_eq!(failed(&broken), failed(&broken));
}
