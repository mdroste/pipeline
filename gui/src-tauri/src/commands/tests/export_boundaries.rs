//! Package-level regressions for run authority and finite report reads.
use super::*;

#[cfg(unix)]
#[test]
fn custom_export_refuses_an_ancestor_link_and_discards_its_stage() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("private.txt"), "outside fixture").unwrap();
    std::os::unix::fs::symlink(outside.path(), source.path().join("nested")).unwrap();
    let mut manifest = export_test_manifest();
    manifest.artifacts.truncate(1);
    manifest.artifacts[0].rel_path = "nested/private.txt".into();
    manifest.artifacts[0].group = "files".into();
    let selection = ExportSelection {
        supporting_artifacts: true,
        ..Default::default()
    };
    let stage = create_export_stage(destination.path()).unwrap();
    assert!(write_custom_payload(source.path(), stage.path(), &manifest, &selection).is_err());
    assert!(!stage.path().join("nested/private.txt").exists());
    drop(stage);
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn shareable_report_refuses_a_final_symlink() {
    let source = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    write_export_test_run(outside.path());
    std::os::unix::fs::symlink(
        outside.path().join("report.json"),
        source.path().join("report.json"),
    )
    .unwrap();
    assert!(
        write_shareable_package(source.path(), destination.path(), &export_test_manifest())
            .is_err()
    );
    assert_eq!(std::fs::read_dir(destination.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_fifo_report_fails_without_waiting_for_a_writer() {
    const CHILD: &str = "PIPELINE_EXPORT_FIFO_FIXTURE";
    if let Some(path) = std::env::var_os(CHILD) {
        let destination = tempfile::tempdir().unwrap();
        assert!(write_shareable_package(
            std::path::Path::new(&path),
            destination.path(),
            &export_test_manifest()
        )
        .is_err());
        return;
    }
    use std::os::unix::ffi::OsStrExt as _;
    let source = tempfile::tempdir().unwrap();
    let name =
        std::ffi::CString::new(source.path().join("report.json").as_os_str().as_bytes()).unwrap();
    // SAFETY: the fixture path is NUL-terminated and lives through the call.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "commands::tests::export_boundaries::a_fifo_report_fails_without_waiting_for_a_writer",
            "--nocapture",
        ])
        .env(CHILD, source.path())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Export blocked on a FIFO instead of rejecting it");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn a_custom_report_keeps_large_supported_text_without_truncation() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    // Single-format exports have a 16 MiB limit; package exports have never
    // shared that limit. Preserve that distinction for ordinary regular files.
    let report = "ordinary text\n".repeat(1_300_000);
    std::fs::write(source.path().join("report.md"), &report).unwrap();
    let selection = ExportSelection {
        report: true,
        ..Default::default()
    };
    write_custom_payload(
        source.path(),
        destination.path(),
        &export_test_manifest(),
        &selection,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(destination.path().join("report.md")).unwrap(),
        output::clean_export_markdown(&report)
    );
}
