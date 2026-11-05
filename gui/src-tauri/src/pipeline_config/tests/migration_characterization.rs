//! Complete directory-byte contracts for isolated catalog migration stages.

use super::*;

fn migration_directory_bytes(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut output = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let mut relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if entry.file_type().unwrap().is_dir() {
                relative.push('/');
                output.insert(relative, Vec::new());
                pending.push(path);
            } else {
                assert!(entry.file_type().unwrap().is_file());
                output.insert(relative, fs::read(path).unwrap());
            }
        }
    }
    output
}

#[test]
fn fresh_catalog_markers_preserve_complete_directory_bytes_and_are_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join(".builtin-catalog-v3"), b"existing marker bytes\n").unwrap();
    fs::write(root.join("custom.json"), b"custom profile bytes\n").unwrap();
    fs::create_dir(root.join(".retired-builtins")).unwrap();
    fs::write(
        root.join(".retired-builtins/older.json"),
        b"older archive\n",
    )
    .unwrap();
    let mut expected = migration_directory_bytes(root);
    for marker in [
        ".builtin-catalog-v4",
        ".builtin-catalog-v6",
        ".builtin-catalog-v7",
        ".builtin-catalog-v8",
        ".builtin-catalog-v9",
        ".builtin-catalog-v10",
        ".builtin-catalog-v15",
        ".structured-auto-review-contract-v2",
        ".modular-auto-review-contract-v1",
        ".compact-auto-review-prompt-v1",
        ".prompt-parsimony-v1",
        ".structured-grant-review-contract-v1",
        ".structured-grant-review-contract-v2",
        ".paper-review-ids-v1",
    ] {
        expected.insert(marker.to_string(), b"fresh-install\n".to_vec());
    }

    // These isolated helpers only receive the temporary directory. Recording
    // every marker first also skips the legacy settings-retargeting branches.
    crate::pipeline_config::migrations::record_fresh_install_markers(root).unwrap();
    migrate_builtin_catalog(root).unwrap();
    assert_eq!(migration_directory_bytes(root), expected);
    crate::pipeline_config::migrations::record_fresh_install_markers(root).unwrap();
    migrate_builtin_catalog(root).unwrap();
    assert_eq!(migration_directory_bytes(root), expected);
}

#[test]
fn partial_catalog_migration_preserves_complete_custom_tree_and_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    crate::pipeline_config::migrations::record_fresh_install_markers(root).unwrap();
    fs::remove_file(root.join(".compact-auto-review-prompt-v1")).unwrap();
    let template = crate::auto_review::orientation_prompt();
    let mut stock = auto_review_profile();
    stock.orientation_prompt = crate::auto_review::expand_orientation_prompt(&template);
    stock.steps[0].prompt.push_str("\nKeep this user edit.");
    fs::write(
        root.join("auto-paper-review.json"),
        serde_json::to_vec_pretty(&stock).unwrap(),
    )
    .unwrap();
    let mut custom = quick_auto_review_profile();
    custom.orientation_prompt = "Custom router {paper_text}".into();
    fs::write(
        root.join("auto-paper-review-fast.json"),
        serde_json::to_vec_pretty(&custom).unwrap(),
    )
    .unwrap();
    fs::write(root.join("custom-invalid.json"), b"unparsed custom data\n").unwrap();
    fs::create_dir(root.join(".retired-builtins")).unwrap();
    fs::write(root.join(".retired-builtins/old.json"), b"old archive\n").unwrap();

    let mut expected = migration_directory_bytes(root);
    stock.orientation_prompt = template;
    expected.insert(
        "auto-paper-review.json".to_string(),
        serde_json::to_vec_pretty(&stock).unwrap(),
    );
    expected.insert(
        ".compact-auto-review-prompt-v1".to_string(),
        b"runtime-expanded-auto-review-catalog\n".to_vec(),
    );

    migrate_builtin_catalog(root).unwrap();
    assert_eq!(migration_directory_bytes(root), expected);
    migrate_builtin_catalog(root).unwrap();
    assert_eq!(migration_directory_bytes(root), expected);
}
