use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/gpu-runtime.json")).unwrap()
}

fn prepared() -> (tempfile::TempDir, String) {
    let fixture = fixture();
    let variant = &fixture[if cfg!(windows) { "windows" } else { "unix" }];
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("runtime")).unwrap();
    std::fs::write(root.path().join(SERVER), "signed-server").unwrap();
    std::fs::write(
        root.path().join("runtime/ggml-vulkan.dll"),
        "signed-library",
    )
    .unwrap();
    std::fs::write(
        root.path().join("integrity.json"),
        variant["integrity"].as_str().unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("manifest.json"),
        variant["manifest"].as_str().unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.path().join("manifest.sig"),
        variant["signature"].as_str().unwrap(),
    )
    .unwrap();
    (root, fixture["public_key"].as_str().unwrap().into())
}

#[test]
fn signed_runtime_verifies_every_installed_library_and_binds_cache_identity() {
    let (root, key) = prepared();
    let runtime = verified_runtime_with_key(root.path(), &key).unwrap();
    assert_eq!(runtime.path, root.path().join(SERVER));
    assert_eq!(runtime.identity.len(), 64);
    std::fs::write(
        root.path().join("runtime/ggml-vulkan.dll"),
        "changed-library",
    )
    .unwrap();
    assert!(verified_runtime_with_key(root.path(), &key).is_err());
}

#[test]
fn extra_or_missing_dlls_invalidate_runtime_instead_of_executing_them() {
    let (root, key) = prepared();
    std::fs::write(root.path().join("runtime/extra.dll"), "injected").unwrap();
    assert!(verified_runtime_with_key(root.path(), &key).is_err());
    std::fs::remove_file(root.path().join("runtime/extra.dll")).unwrap();
    std::fs::remove_file(root.path().join(SERVER)).unwrap();
    assert!(verified_runtime_with_key(root.path(), &key).is_err());
}

#[test]
fn manifest_must_be_signed_and_match_requested_release_and_runtime() {
    let fixture = fixture();
    let variant = &fixture["windows"];
    let key = fixture["public_key"].as_str().unwrap();
    let bytes = variant["manifest"].as_str().unwrap().as_bytes();
    let signature = variant["signature"].as_str().unwrap();
    assert!(verify_manifest(bytes, signature, key, Some("1.5.3")).is_ok());
    assert!(verify_manifest(bytes, signature, key, Some("9.9.9")).is_err());
    assert!(verify_manifest(b"{}", signature, key, None).is_err());
    assert!(verify_manifest(bytes, "unsigned", key, None).is_err());
    for name in ["wrong_platform", "wrong_runtime", "wrong_archive"] {
        let case = &fixture[name];
        assert!(
            verify_manifest(
                case["manifest"].as_str().unwrap().as_bytes(),
                case["signature"].as_str().unwrap(),
                key,
                None
            )
            .is_err(),
            "{name}"
        );
    }
}

#[test]
fn signed_inventory_cannot_be_replaced_with_locally_rehashed_files() {
    let (root, key) = prepared();
    std::fs::write(root.path().join(SERVER), "different-server").unwrap();
    write_install_integrity(root.path(), &["runtime"]).unwrap();
    assert!(verified_runtime_with_key(root.path(), &key).is_err());
}

#[test]
fn packaged_runtime_round_trips_through_the_production_archive_reader() {
    let (source, key) = prepared();
    let download = tempfile::tempdir().unwrap();
    let archive_path = download.path().join("runtime.zip");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&archive_path).unwrap());
    for name in [SERVER, "runtime/ggml-vulkan.dll", "integrity.json"] {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(&std::fs::read(source.path().join(name)).unwrap())
            .unwrap();
    }
    archive.finish().unwrap();
    let staged = download.path().join("staged");
    unpack_llama_archive(&archive_path, &staged, true).unwrap();
    for name in ["manifest.json", "manifest.sig"] {
        std::fs::copy(source.path().join(name), staged.join(name)).unwrap();
    }
    assert!(verified_runtime_with_key(&staged, &key).is_ok());
}

#[test]
fn interrupted_runtime_activation_restores_backup_and_removes_partial_downloads() {
    let native = tempfile::tempdir().unwrap();
    let backup = native.path().join(GPU_RUNTIME_BACKUP);
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::write(backup.join("marker"), "old").unwrap();
    let stage = native.path().join(format!("{GPU_RUNTIME_STAGE}partial"));
    std::fs::create_dir_all(&stage).unwrap();
    cleanup_gpu_runtime_at(native.path()).unwrap();
    assert_eq!(
        std::fs::read_to_string(native.path().join(GPU_RUNTIME_DIR).join("marker")).unwrap(),
        "old"
    );
    assert!(!stage.exists());
    assert!(!backup.exists());
}
