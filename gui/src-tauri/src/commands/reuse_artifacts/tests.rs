use super::*;

#[test]
fn resume_leaves_behind_a_unit_the_parent_never_finished() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent");
    let step = parent
        .join("artifacts/by-step")
        .join(crate::pipeline::executor::step_slug("producer"));
    for unit in ["finished", "cut-off"] {
        std::fs::create_dir_all(step.join(unit).join("files")).unwrap();
        std::fs::write(step.join(unit).join("files/table.csv"), "1,2").unwrap();
    }
    crate::pipeline::unit_files::begin_call(step.join("cut-off").to_str().unwrap()).unwrap();

    let child = temp.path().join("child/artifacts");
    restore_producers(&parent, &child, std::iter::once("producer".into())).unwrap();

    let restored = child
        .join("by-step")
        .join(crate::pipeline::executor::step_slug("producer"));
    assert!(restored.join("finished/files/table.csv").is_file());
    assert!(!restored.join("cut-off").exists());
    assert!(!child.join("checkpoints").exists());
}

#[test]
fn restored_sources_and_producers_are_independent_of_originals() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original.tex");
    std::fs::write(&original, "original revision").unwrap();
    let parent = temp.path().join("parent");
    let staged = snapshot_source(
        &ScopedSourceContext {
            source_path: Some(original.clone()),
            read_root: None,
        },
        &parent,
    )
    .unwrap();
    std::fs::create_dir_all(parent.join("context")).unwrap();
    let relative = staged
        .source_path
        .as_ref()
        .unwrap()
        .strip_prefix(&parent)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    std::fs::write(
        parent.join("context/source-snapshot.json"),
        serde_json::to_vec(&SourceRecord {
            path: Some(relative),
            fingerprint: source_fingerprint(staged.source_path.as_ref().unwrap()).unwrap(),
        })
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&original, "changed live revision").unwrap();
    let child = temp.path().join("child");
    let restored = restore_source(&parent, &child, true).unwrap();
    assert_eq!(
        std::fs::read_to_string(restored.source_path.unwrap()).unwrap(),
        "original revision"
    );
    let named_parent = temp.path().join("named-parent");
    let named_root = named_parent
        .join("named-sources")
        .join(crate::pipeline::executor::step_slug("appendix"));
    copy_tree(&parent, &named_root).unwrap();
    let named = restore_named_source(&named_parent, "appendix", &child, true).unwrap();
    assert_eq!(
        std::fs::read_to_string(named.source_path.unwrap()).unwrap(),
        "original revision"
    );
    let slug = crate::pipeline::executor::step_slug("producer");
    let files = parent
        .join("artifacts/by-step")
        .join(&slug)
        .join("unit/files");
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("results.csv"), "1,2").unwrap();
    restore_producers(
        &parent,
        &child.join("artifacts"),
        std::iter::once("producer".into()),
    )
    .unwrap();
    std::fs::write(
        child
            .join("artifacts/by-step")
            .join(slug)
            .join("unit/files/results.csv"),
        "3,4",
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(files.join("results.csv")).unwrap(),
        "1,2"
    );
    assert!(restore_source(temp.path(), &child, true).is_err());
    std::fs::write(
        staged.source_path.as_ref().unwrap(),
        "tampered retained revision",
    )
    .unwrap();
    assert!(
        restore_source(&parent, &temp.path().join("tampered-child"), true)
            .unwrap_err()
            .contains("integrity check")
    );
}
#[test]
fn retained_source_keeps_large_files_and_round_trips_trees() {
    let temp = tempfile::tempdir().unwrap();
    let big = temp.path().join("scan.pdf");
    std::fs::write(&big, vec![0u8; 51_000_000]).unwrap();
    let mut writer = crate::runs::RunWriter::create_in(temp.path(), "big_run").unwrap();
    retain_source(
        &ScopedSourceContext {
            source_path: Some(big.clone()),
            read_root: None,
        },
        &mut writer,
    )
    .unwrap();
    let run_dir = writer.dir().to_path_buf();
    assert!(run_dir.join("source-snapshot/scan.pdf").is_file());
    let restored = restore_source(&run_dir, &temp.path().join("big_child"), true).unwrap();
    assert_eq!(
        std::fs::metadata(restored.source_path.unwrap())
            .unwrap()
            .len(),
        51_000_000
    );

    // A modest tree round-trips; the 500-entry registration cap is
    // covered without per-file fsync cost in runs::tests.
    let tree = temp.path().join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    for index in 0..24 {
        std::fs::write(tree.join(format!("{index:04}.tex")), "x").unwrap();
    }
    let mut writer = crate::runs::RunWriter::create_in(temp.path(), "tree_run").unwrap();
    retain_source(
        &ScopedSourceContext {
            source_path: Some(tree.clone()),
            read_root: Some(tree.clone()),
        },
        &mut writer,
    )
    .unwrap();
    let run_dir = writer.dir().to_path_buf();
    assert_eq!(
        std::fs::read_dir(run_dir.join("source-snapshot/tree"))
            .unwrap()
            .count(),
        24
    );
    let restored = restore_source(&run_dir, &temp.path().join("tree_child"), true).unwrap();
    assert_eq!(
        std::fs::read_dir(restored.source_path.unwrap())
            .unwrap()
            .count(),
        24
    );
}

/// Capture the way a run does, in a private scope so that cancellation in
/// other tests cannot reach it.
fn capture(selected: &Path, mode: &str, root: &Path) -> ScopedSourceContext {
    capture_up_to(selected, mode, root, MAX_FOLDER_COPY_FILES)
}

fn capture_up_to(
    selected: &Path,
    mode: &str,
    root: &Path,
    max_files: usize,
) -> ScopedSourceContext {
    let staged = crate::pipeline::extract::stage_selected_source(selected, mode, root).unwrap();
    let run = crate::run_scope::ActiveScope::begin(format!("capture-{}", root.display()));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(crate::run_scope::scoped(
            run.scope().clone(),
            capture_source(staged, mode, root, max_files),
        ))
        .unwrap()
}

fn sparse(path: &Path, bytes: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::File::create(path).unwrap().set_len(bytes).unwrap();
}

const GB: u64 = 1024 * 1024 * 1024;

/// Without selective copying, any one of these fails the launch: the
/// symlink, the pipe, the data file, or a copy that flushes every file.
#[test]
#[cfg(unix)]
fn a_replication_folder_reaches_inventory_with_its_omissions_noted() {
    use std::os::unix::ffi::OsStrExt as _;
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    std::fs::create_dir_all(package.join("code")).unwrap();
    for index in 0..600 {
        std::fs::write(package.join(format!("code/{index:04}.do")), "display 1").unwrap();
    }
    std::fs::write(package.join("README.md"), "# Replication").unwrap();
    std::fs::write(package.join(".DS_Store"), "x").unwrap();
    for name in ["config", "objects/aa/object", "refs/heads/main"] {
        let path = package.join(".git").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "git").unwrap();
    }
    std::fs::create_dir_all(package.join("node_modules/pkg")).unwrap();
    std::fs::write(package.join("node_modules/pkg/index.js"), "x").unwrap();
    std::os::unix::fs::symlink(package.join("README.md"), package.join("link.md")).unwrap();
    let fifo = package.join("blocked.pipe");
    let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
    sparse(&package.join("data/panel.dta"), 3 * GB);

    let private = temp.path().join("private");
    let captured = capture(&package, "folder", &private);
    let tree = captured.source_path.as_ref().unwrap();
    let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();

    assert_eq!(std::fs::read_dir(tree.join("code")).unwrap().count(), 600);
    assert!(
        inventory.text.contains("601 files"),
        "{}",
        &inventory.text[..200]
    );
    assert!(inventory.text.contains("| code/0599.do |"));
    assert!(!tree.join("data/panel.dta").exists());
    assert!(!tree.join(".git").exists());
    assert!(!tree.join("node_modules").exists());
    assert!(!tree.join("link.md").exists());
    let notes = inventory.quality_notes.join("\n");
    assert!(notes.contains("data/panel.dta (3.0 GB)"), "{notes}");
    assert!(notes.contains(".git, node_modules"), "{notes}");
    for skipped in ["1 hidden item", "1 symbolic link", "1 special file"] {
        assert!(notes.contains(skipped), "{notes}");
    }
    assert!(inventory.text.contains("| data/panel.dta | 3221225472 |"));

    // The retained copy and a rerun's restored copy repeat the same files
    // exactly, so reruns verify and report the same omissions.
    let mut writer = crate::runs::RunWriter::create_in(temp.path(), "folder_run").unwrap();
    retain_source(&captured, &mut writer).unwrap();
    let restored = restore_source(writer.dir(), &temp.path().join("rerun"), true).unwrap();
    let reread =
        crate::pipeline::extract::ingest_folder(restored.source_path.unwrap().to_str().unwrap())
            .unwrap();
    assert_eq!(reread.quality_notes, inventory.quality_notes);
}

#[test]
fn an_unchanged_folder_keeps_its_fingerprint_from_run_to_run() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    std::fs::create_dir_all(folder.join("code")).unwrap();
    std::fs::write(folder.join("notes.md"), "notes").unwrap();
    std::fs::write(folder.join("code/main.do"), "use data").unwrap();
    let fingerprint = |run: &str| {
        let captured = capture(&folder, "folder", &temp.path().join(run));
        let tree = captured.source_path.unwrap();
        crate::pipeline::extract::ingest_folder(tree.to_str().unwrap())
            .unwrap()
            .paper_hash
    };

    let first = fingerprint("first-run");
    assert_eq!(first, fingerprint("second-run"));

    // A same-size edit is still a different folder.
    std::fs::write(folder.join("notes.md"), "edits").unwrap();
    assert_ne!(first, fingerprint("third-run"));
}

#[test]
fn a_complete_folder_copy_leaves_no_record() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("folder");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("notes.md"), "notes").unwrap();
    let captured = capture(&folder, "folder", &temp.path().join("private"));
    let tree = captured.source_path.unwrap();
    assert_eq!(std::fs::read_dir(&tree).unwrap().count(), 1);
    let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();
    assert!(inventory.quality_notes.is_empty());
}

#[test]
fn a_latex_project_keeps_files_a_folder_copy_would_leave_out() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    for (name, text) in [
        (
            "main.tex",
            "\\documentclass{article}\n\\input{build/macros}\n\\input{.shared/defs}\n\\includegraphics{target/plot.pdf}\n",
        ),
        ("build/macros.tex", "macros"),
        (".shared/defs.tex", "definitions"),
        ("target/plot.pdf", "%PDF-1.4"),
    ] {
        let path = project.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let captured = capture(&project, "document", &temp.path().join("private"));
    let tree = captured.source_path.unwrap();
    for name in [
        "main.tex",
        "build/macros.tex",
        ".shared/defs.tex",
        "target/plot.pdf",
    ] {
        assert!(tree.join(name).is_file(), "{name} was not captured");
    }
}

#[tokio::test]
async fn launch_notices_name_only_folders_that_will_not_fit() {
    let temp = tempfile::tempdir().unwrap();
    let package = temp.path().join("package");
    sparse(&package.join("data/panel.dta"), 3 * GB);
    std::fs::write(package.join("README.md"), "# Replication").unwrap();
    let tidy = temp.path().join("tidy");
    std::fs::create_dir_all(&tidy).unwrap();
    std::fs::write(tidy.join("notes.md"), "notes").unwrap();
    let mut config = crate::pipeline_config::PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    config.extraction.extra_inputs = vec![crate::pipeline_config::InputSlot {
        key: "appendix".into(),
        mode: "folder".into(),
        ..Default::default()
    }];
    let package = package.to_str().unwrap();
    let tidy = tidy.to_str().unwrap();
    let named = |path: &str| std::collections::HashMap::from([("appendix".into(), path.into())]);

    let settings = crate::settings::Settings::default();
    let primary =
        folder_capture_notices(&config, &settings, Some(package), true, &named(tidy)).await;
    assert_eq!(primary.len(), 1, "{primary:?}");
    assert!(
        primary[0].starts_with("package holds 3.0 GB in 2 files."),
        "{}",
        primary[0]
    );
    assert!(primary[0].contains("1 file (3.0 GB) will be left out"));
    let slot = folder_capture_notices(&config, &settings, Some(tidy), true, &named(package)).await;
    assert_eq!(slot, primary);
    let document =
        folder_capture_notices(&config, &settings, Some(package), false, &named(tidy)).await;
    assert!(document.is_empty());
}

fn config_with_folders(slots: &[&str]) -> crate::pipeline_config::PipelineConfig {
    let mut config = crate::pipeline_config::PipelineConfig {
        steps: Vec::new(),
        merge: Default::default(),
        outputs: Default::default(),
        context_cache: Default::default(),
        use_orientation: true,
        orientation_prompt: String::new(),
        orientation_schema: None,
        extraction: Default::default(),
        parallel_context_template: String::new(),
        variables: Vec::new(),
    };
    config.extraction.extra_inputs = slots
        .iter()
        .map(|key| crate::pipeline_config::InputSlot {
            key: key.to_string(),
            mode: "folder".into(),
            ..Default::default()
        })
        .collect();
    config
}

fn selected(slots: &[&str]) -> HashMap<String, String> {
    slots
        .iter()
        .map(|key| (key.to_string(), "/selected".to_string()))
        .collect()
}

#[test]
fn folders_share_what_the_manifest_can_hold() {
    let settings = crate::settings::Settings::default();
    let limit =
        |config: &_, primary, named: &_| folder_file_limit(config, &settings, primary, named);
    let two = config_with_folders(&["a", "b"]);
    assert_eq!(limit(&two, true, &HashMap::new()), MAX_FOLDER_COPY_FILES);
    assert_eq!(limit(&two, true, &selected(&["a"])), 2_000);
    assert_eq!(limit(&two, true, &selected(&["a", "b"])), 1_333);
    assert_eq!(limit(&two, false, &selected(&["a", "b"])), 2_000);

    // Each planned step output also takes an entry, and a single folder
    // keeps the full limit at the largest run the safety limits allow.
    let mut fan_out = config_with_folders(&["a"]);
    fan_out.steps = vec![crate::pipeline_config::StepConfig {
        id: "fan".into(),
        enabled: true,
        phase: crate::pipeline_config::Phase::Parallel,
        for_each: Some(crate::pipeline_config::ForEach {
            glob: "**/*".into(),
            max: 1_000,
            artifact: None,
        }),
        ..Default::default()
    }];
    assert_eq!(limit(&fan_out, true, &selected(&["a"])), 1_500);
    fan_out.steps[0].for_each.as_mut().unwrap().max = 2_000;
    assert_eq!(limit(&fan_out, true, &HashMap::new()), 2_000);
}

#[test]
fn three_full_folders_fit_one_run_only_at_their_shared_limit() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("package");
    std::fs::create_dir_all(&folder).unwrap();
    for index in 0..MAX_FOLDER_COPY_FILES {
        std::fs::write(folder.join(format!("{index:04}.do")), "x").unwrap();
    }
    let retain_three = |captured: &ScopedSourceContext, run: &str| {
        let mut writer = crate::runs::RunWriter::create_in(temp.path(), run).unwrap();
        retain_source(captured, &mut writer)?;
        retain_named_source("appendix", captured, &mut writer)?;
        retain_named_source("archive", captured, &mut writer)?;
        writer.add_text("report.md", "Report", "report", "finished")
    };

    let config = config_with_folders(&["appendix", "archive"]);
    let named = selected(&["appendix", "archive"]);
    let shared = folder_file_limit(&config, &crate::settings::Settings::default(), true, &named);
    let private = temp.path().join("shared");
    let fitted = capture_up_to(&folder, "folder", &private, shared);
    retain_three(&fitted, "shared_run").unwrap();
    let tree = fitted.source_path.unwrap();
    let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();
    assert!(
        inventory.quality_notes[0].contains("up to 1,333 files"),
        "{:?}",
        inventory.quality_notes
    );

    let full = capture(&folder, "folder", &temp.path().join("full"));
    let error = retain_three(&full, "full_run").unwrap_err();
    assert!(error.contains("could not be fully registered"), "{error}");
}

#[tokio::test]
async fn a_preview_scan_that_runs_out_of_time_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().to_str().unwrap().to_string();
    let name = temp.path().file_name().unwrap().to_string_lossy();

    let notices = notices_within(vec![folder], MAX_FOLDER_COPY_FILES, Duration::ZERO).await;

    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].starts_with(&format!("{name} took too long to scan")));
    assert!(notices[0].contains("up to 2,000 files"), "{}", notices[0]);
}

#[tokio::test]
async fn a_preview_stops_waiting_for_a_scan_stuck_in_the_system() {
    let started = Instant::now();
    let stuck = || std::thread::sleep(Duration::from_millis(600));

    let finished = wait_for_blocking(stuck, Duration::from_millis(20)).await;

    assert!(finished.is_none());
    assert!(started.elapsed() < Duration::from_millis(400));
    assert_eq!(
        wait_for_blocking(|| 7, Duration::from_secs(5)).await,
        Some(7)
    );
}

#[test]
fn legacy_bundle_rebuild_never_opens_the_live_document() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("changed.docx");
    std::fs::write(&original, "a changed and invalid Word document").unwrap();
    let extraction = crate::models::ExtractionResult {
        text: "captured text".into(),
        method: "resumed-document".into(),
        source_path: original.to_string_lossy().into_owned(),
        paper_hash: "captured-hash".into(),
        quality_notes: Vec::new(),
    };
    let build = build_bundle(&extraction, None, temp.path()).unwrap();
    assert_eq!(build.bundle.source_kind, "text");
    assert_eq!(build.bundle.extraction.source_path, extraction.source_path);
}
