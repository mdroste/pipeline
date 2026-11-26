use super::*;

fn fixture() -> (tempfile::TempDir, DevelopmentLock) {
    let source = tempfile::tempdir().unwrap();
    let expected = source.path().join("expected");
    std::fs::create_dir_all(expected.join("runtime/licenses")).unwrap();
    let license = b"test license";
    std::fs::write(source.path().join("license.txt"), license).unwrap();
    std::fs::write(
        expected.join("runtime/licenses/LICENSE-llama.cpp.txt"),
        license,
    )
    .unwrap();
    let archive = source.path().join("llama.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for name in [
        "llama-server.exe",
        "ggml-vulkan.dll",
        "ggml-rpc.dll",
        "llama-cli.exe",
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(name.as_bytes()).unwrap();
        if name == "llama-server.exe" || name == "ggml-vulkan.dll" {
            std::fs::write(expected.join("runtime").join(name), name).unwrap();
        }
    }
    zip.finish().unwrap();
    let lock = DevelopmentLock {
        version: LLAMA_CPP_VERSION.into(),
        archive: Artifact {
            url: "https://example.invalid/llama.zip".into(),
            sha256: sha256_regular_file(&archive, MAX_LLAMA_ARCHIVE_BYTES).unwrap(),
        },
        license_text: Artifact {
            url: "https://example.invalid/LICENSE".into(),
            sha256: format!("{:x}", Sha256::digest(license)),
        },
        development_integrity_sha256: write_install_integrity(&expected, &["runtime"]).unwrap(),
    };
    (source, lock)
}

fn stage(source: &Path, lock: &DevelopmentLock) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    prepare_runtime_with_lock(
        root.path(),
        &source.join("llama.zip"),
        &source.join("license.txt"),
        lock,
    )
    .unwrap();
    root
}

#[test]
fn dev_runtime_is_pinned_and_does_not_satisfy_release_verification() {
    let (source, lock) = fixture();
    let root = stage(source.path(), &lock);
    let server = verify_runtime(root.path(), lock.development_integrity_sha256).unwrap();
    assert_eq!(server.path, root.path().join(SERVER));
    assert!(!root.path().join("runtime/ggml-rpc.dll").exists());
    assert!(!root.path().join("runtime/llama-cli.exe").exists());
    assert!(is_development_runtime(root.path()));
    // This is the verifier used by all release builds, regardless of profile
    // used to execute this test. Development inventory is never a signature.
    assert!(super::super::verified_runtime_at(root.path()).is_err());
    std::fs::write(root.path().join("manifest.sig"), "broken signature").unwrap();
    assert!(!is_development_runtime(root.path()));
    assert!(super::super::installed_runtime_at(root.path()).is_err());
}

#[test]
fn dev_archive_and_license_must_match_compiled_pins() {
    for file in ["llama.zip", "license.txt"] {
        let (source, lock) = fixture();
        std::fs::write(source.path().join(file), "changed").unwrap();
        let root = tempfile::tempdir().unwrap();
        assert!(prepare_runtime_with_lock(
            root.path(),
            &source.path().join("llama.zip"),
            &source.path().join("license.txt"),
            &lock
        )
        .is_err());
        assert!(!root.path().join(SERVER).exists());
    }
}

#[test]
fn dev_runtime_rejects_changed_missing_and_injected_libraries() {
    let (source, lock) = fixture();
    for change in ["modified", "missing", "extra", "rehashed"] {
        let root = stage(source.path(), &lock);
        let dll = root.path().join("runtime/ggml-vulkan.dll");
        match change {
            "missing" => std::fs::remove_file(dll).unwrap(),
            "extra" => std::fs::write(root.path().join("runtime/extra.dll"), "injected").unwrap(),
            _ => std::fs::write(dll, "modified").unwrap(),
        }
        if change == "rehashed" {
            write_install_integrity(root.path(), &["runtime"]).unwrap();
        }
        assert!(
            verify_runtime(root.path(), lock.development_integrity_sha256.clone()).is_err(),
            "{change}"
        );
    }
}

#[tokio::test]
#[ignore = "downloads the pinned Windows Vulkan runtime and probes local hardware"]
async fn dev_download_verifies_and_probes_vulkan() {
    struct Events;
    impl crate::emit::Events for Events {
        fn emit_event(
            &self,
            event: &str,
            payload: serde_json::Value,
        ) -> Result<(), crate::emit::EmitError> {
            if event == "engines:log" {
                eprintln!("{payload}");
            }
            Ok(())
        }
    }
    let app: crate::emit::EventBus = std::sync::Arc::new(Events);
    let root = tempfile::tempdir().unwrap();
    // Exercise the same source selection and downloader as the Settings action.
    super::super::stage_runtime(&app, root.path())
        .await
        .unwrap();
    let server = super::super::installed_runtime_at(root.path()).unwrap();
    assert!(super::super::verified_runtime_at(root.path()).is_err());
    assert!(!root.path().join("manifest.json").exists());
    let devices = probe_devices(&server.path).unwrap();
    eprintln!(
        "Verified development runtime {}: {devices:?}",
        server.identity
    );
}
