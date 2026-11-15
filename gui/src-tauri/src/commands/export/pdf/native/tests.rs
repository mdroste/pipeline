use super::*;

#[test]
fn helper_cannot_overwrite_a_file_or_escape_its_staging_directory() {
    let stage = tempfile::tempdir().unwrap();
    let input = stage.path().join("report.html");
    let output = stage.path().join("report.pdf");
    std::fs::write(&input, "report").unwrap();
    assert!(validate_paths(&input, &output).is_ok());
    assert!(validate_paths(&input, &stage.path().join("other.pdf")).is_err());
    assert!(validate_paths(&input, &stage.path().join("outside/report.pdf")).is_err());
    std::fs::write(&output, "existing").unwrap();
    assert!(validate_paths(&input, &output).is_err());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "existing");
}

#[test]
fn helper_rejects_oversized_input() {
    let stage = tempfile::tempdir().unwrap();
    let input = stage.path().join("report.html");
    std::fs::File::create(&input)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(validate_paths(&input, &stage.path().join("report.pdf"))
        .unwrap_err()
        .contains("limit"));
}
