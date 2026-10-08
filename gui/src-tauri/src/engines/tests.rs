use super::*;

#[test]
fn checksums_are_well_formed() {
    for checksum in [
        PADDLE_MODEL_SHA256,
        PADDLE_MMPROJ_SHA256,
        PADDLE_LAYOUT_MODEL_SHA256,
    ] {
        assert_eq!(checksum.len(), 64);
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }
    for artifact in LLAMA_ARTIFACTS {
        assert_eq!(artifact.sha256.len(), 64, "{}", artifact.filename);
        assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }
    for artifact in UV_ARTIFACTS {
        assert_eq!(artifact.sha256.len(), 64, "{}", artifact.target);
        assert!(artifact.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }
    for artifact in PYTHON_ARTIFACTS {
        for checksum in [artifact.sha256, artifact.lock_sha256] {
            assert_eq!(checksum.len(), 64, "{}", artifact.target);
            assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(
            format!("{:x}", Sha256::digest(artifact.lock.as_bytes())),
            artifact.lock_sha256,
            "{}",
            artifact.target
        );
    }
}

#[test]
fn managed_bootstrap_digest_is_bounded() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(b"abc").unwrap();
    assert_eq!(
        sha256_regular_file(file.path(), 3).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(sha256_regular_file(file.path(), 2).is_err());
}

#[test]
fn registry_has_unique_ids() {
    let mut ids = std::collections::HashSet::new();
    for e in ENGINES {
        assert!(ids.insert(e.id), "duplicate engine id {}", e.id);
    }
    assert_eq!(
        ids,
        std::collections::HashSet::from(["paddleocr-vl-parser"])
    );
    assert!(engine("paddleocr-vl").is_ok());
}

#[test]
fn managed_engine_cleanup_removes_only_installer_owned_leftovers() {
    let native = tempfile::tempdir().unwrap();
    let native_root = native.path();
    let parser_root = native_root.join("paddleocr-parser");
    let uv_name = exe("uv");
    let uv_backup_name = format!(".{uv_name}-backup");
    let versions = parser_root.join("versions");
    let current = versions.join(PADDLE_PARSER_RELEASE);
    let current_backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    let old_release = versions.join("paddleocr-3.7.0-paddle-3.2.1-r1");

    for path in [
        native_root.join("paddleocr-vl"),
        native_root.join(".paddleocr-vl-backup"),
        native_root.join(".paddleocr-vl-staging-abandoned"),
        current.clone(),
        current_backup.clone(),
        old_release.clone(),
        parser_root.join("uv-cache"),
        parser_root.join("python"),
        parser_root.join("models"),
        parser_root.join("runtime/.uv-staging-abandoned"),
    ] {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("fixture"), b"fixture").unwrap();
    }
    std::fs::write(parser_root.join("runtime").join(&uv_name), b"uv").unwrap();
    std::fs::write(parser_root.join("runtime").join(&uv_backup_name), b"old uv").unwrap();
    std::fs::create_dir_all(versions.join("notes")).unwrap();
    std::fs::create_dir_all(native_root.join("unrelated-engine-data")).unwrap();

    let removed = cleanup_managed_engine_storage_at(native_root, true).unwrap();
    assert!(removed >= 7);
    assert!(native_root.join("paddleocr-vl").is_dir());
    assert!(!native_root.join(".paddleocr-vl-backup").exists());
    assert!(!native_root.join(".paddleocr-vl-staging-abandoned").exists());
    assert!(current.is_dir());
    assert!(!current_backup.exists());
    assert!(!old_release.exists());
    assert!(!parser_root.join("uv-cache").exists());
    assert!(!parser_root.join("python").exists());
    assert!(!parser_root.join("runtime/.uv-staging-abandoned").exists());
    assert!(!parser_root.join("runtime").join(&uv_backup_name).exists());

    // Runtime data and names outside the installer's namespaces survive.
    assert!(parser_root.join("models/fixture").is_file());
    assert!(parser_root.join("runtime").join(&uv_name).is_file());
    assert!(versions.join("notes").is_dir());
    assert!(native_root.join("unrelated-engine-data").is_dir());
}

#[test]
fn managed_engine_cleanup_recovers_interrupted_swaps() {
    let native = tempfile::tempdir().unwrap();
    let native_root = native.path();
    let uv_name = exe("uv");
    let uv_backup_name = format!(".{uv_name}-backup");
    let base_backup = native_root.join(".paddleocr-vl-backup");
    std::fs::create_dir_all(&base_backup).unwrap();
    std::fs::write(base_backup.join("previous"), b"base").unwrap();

    let parser_root = native_root.join("paddleocr-parser");
    let versions = parser_root.join("versions");
    let current = versions.join(PADDLE_PARSER_RELEASE);
    let current_backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    std::fs::create_dir_all(&current).unwrap();
    std::fs::write(current.join("partial"), b"partial").unwrap();
    std::fs::create_dir_all(&current_backup).unwrap();
    std::fs::write(current_backup.join("previous"), b"parser").unwrap();
    std::fs::create_dir_all(parser_root.join("runtime")).unwrap();
    std::fs::write(parser_root.join("runtime").join(&uv_backup_name), b"uv").unwrap();

    cleanup_managed_engine_storage_at(native_root, false).unwrap();

    assert!(native_root.join("paddleocr-vl/previous").is_file());
    assert!(!base_backup.exists());
    assert!(current.join("previous").is_file());
    assert!(!current.join("partial").exists());
    assert!(!current_backup.exists());
    assert!(parser_root.join("runtime").join(&uv_name).is_file());
    assert!(!parser_root.join("runtime").join(&uv_backup_name).exists());
    assert_eq!(
        cleanup_managed_engine_storage_at(native_root, true).unwrap(),
        0
    );
}

#[test]
fn managed_engine_cleanup_drops_unrecoverable_partial_parser() {
    let native = tempfile::tempdir().unwrap();
    let current = native
        .path()
        .join("paddleocr-parser/versions")
        .join(PADDLE_PARSER_RELEASE);
    std::fs::create_dir_all(&current).unwrap();
    std::fs::write(current.join("partial"), b"partial").unwrap();

    cleanup_managed_engine_storage_at(native.path(), false).unwrap();

    assert!(!current.exists());
}

#[test]
fn parser_cleanup_commit_marker_is_bounded_and_release_specific() {
    let parser = tempfile::tempdir().unwrap();
    let version = parser.path().join("versions").join(PADDLE_PARSER_RELEASE);
    std::fs::create_dir_all(&version).unwrap();
    let manifest = version.join("install.json");
    std::fs::write(&manifest, b"not json").unwrap();
    assert!(!parser_install_committed(parser.path()));

    std::fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": "obsolete",
            "integrity_sha256": "0".repeat(64),
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(!parser_install_committed(parser.path()));

    std::fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "integrity_sha256": "0".repeat(64),
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(parser_install_committed(parser.path()));
}

#[test]
fn parser_status_probe_uses_committed_file_shapes() {
    let native = tempfile::tempdir().unwrap();
    let base = native.path().join("paddleocr-vl");
    let base_models = base.join("models");
    let base_runtime = base.join("runtime");
    std::fs::create_dir_all(&base_models).unwrap();
    std::fs::create_dir_all(&base_runtime).unwrap();
    std::fs::write(base_models.join(PADDLE_MODEL_FILE), b"model").unwrap();
    std::fs::write(base_models.join(PADDLE_MMPROJ_FILE), b"projector").unwrap();
    std::fs::write(base_runtime.join(exe("llama-server")), b"server").unwrap();
    let llama = llama_artifact().unwrap();
    std::fs::write(
        base.join("install.json"),
        serde_json::to_vec(&serde_json::json!({
            "engine": "paddleocr-vl",
            "model_version": "1.6",
            "model_revision": PADDLE_MODEL_REVISION,
            "llama_cpp_version": LLAMA_CPP_VERSION,
            "runtime_sha256": llama.sha256,
            "model_sha256": PADDLE_MODEL_SHA256,
            "mmproj_sha256": PADDLE_MMPROJ_SHA256,
        }))
        .unwrap(),
    )
    .unwrap();

    let parser = native.path().join("paddleocr-parser");
    let version = parser.join("versions").join(PADDLE_PARSER_RELEASE);
    let layout = version.join("models/PP-DocLayoutV3_infer");
    let python = venv_python(&version.join("venv"));
    std::fs::create_dir_all(python.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&layout).unwrap();
    std::fs::write(&python, b"python").unwrap();
    for (path, contents) in [
        (
            version.join("paddle_parser_sidecar.py"),
            b"script".as_slice(),
        ),
        (version.join("packages.txt"), b"packages".as_slice()),
        (version.join("pylock.toml"), b"lock".as_slice()),
        (version.join("runtime-lock.json"), b"runtime".as_slice()),
        (version.join("integrity.json"), b"integrity".as_slice()),
        (layout.join("inference.json"), b"json".as_slice()),
        (layout.join("inference.yml"), b"yaml".as_slice()),
        (layout.join("inference.pdiparams"), b"params".as_slice()),
    ] {
        std::fs::write(path, contents).unwrap();
    }
    std::fs::write(
        version.join("install.json"),
        serde_json::to_vec(&serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "integrity_sha256": "0".repeat(64),
        }))
        .unwrap(),
    )
    .unwrap();

    let status = paddle_full_parser_status_at(native.path()).unwrap();
    assert_eq!(status.release, PADDLE_PARSER_RELEASE);
    assert_eq!(status.script, version.join("paddle_parser_sidecar.py"));

    std::fs::remove_file(base_models.join(PADDLE_MMPROJ_FILE)).unwrap();
    assert!(paddle_full_parser_status_at(native.path()).is_none());
}

#[cfg(unix)]
#[test]
fn managed_engine_cleanup_unlinks_staging_symlinks_without_following_them() {
    let native = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let protected = outside.path().join("keep");
    std::fs::write(&protected, b"keep").unwrap();
    let staging_link = native.path().join(".paddleocr-vl-staging-symlink");
    std::os::unix::fs::symlink(outside.path(), &staging_link).unwrap();

    cleanup_managed_engine_storage_at(native.path(), false).unwrap();

    assert!(!staging_link.exists());
    assert!(protected.is_file());
}

#[test]
fn llama_artifacts_cover_desktop_release_platforms() {
    for (os, arch) in [
        ("macos", "aarch64"),
        ("macos", "x86_64"),
        ("linux", "aarch64"),
        ("linux", "x86_64"),
        ("windows", "aarch64"),
        ("windows", "x86_64"),
    ] {
        assert!(
            llama_artifact_for(os, arch).is_some(),
            "missing llama.cpp artifact for {os}/{arch}"
        );
    }
    assert!(llama_artifact_for("linux", "riscv64").is_none());
}

#[test]
fn full_parser_support_is_explicit_for_release_platforms() {
    assert!(full_parser_support_for("macos", "aarch64").is_ok());
    assert!(full_parser_support_for("windows", "x86_64").is_ok());
    assert!(full_parser_support_for("linux", "x86_64").is_ok());
    assert!(full_parser_support_for("macos", "x86_64")
        .unwrap_err()
        .contains("Intel macOS"));
    assert!(full_parser_support_for("windows", "aarch64").is_err());
}

#[test]
fn uv_artifacts_cover_supported_full_parser_platforms() {
    for (os, arch) in [
        ("macos", "aarch64"),
        ("windows", "x86_64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
    ] {
        assert!(
            uv_artifact_for(os, arch).is_some(),
            "missing uv for {os}/{arch}"
        );
    }
}

#[test]
fn python_artifacts_and_locks_cover_supported_full_parser_platforms() {
    let runtime_lock: serde_json::Value = serde_json::from_str(PADDLE_PARSER_RUNTIME_LOCK).unwrap();
    assert_eq!(
        runtime_lock
            .get("release")
            .and_then(serde_json::Value::as_str),
        Some(PADDLE_PARSER_RELEASE)
    );
    assert_eq!(
        runtime_lock
            .pointer("/python/version")
            .and_then(serde_json::Value::as_str),
        Some(PYTHON_VERSION)
    );
    assert_eq!(
        runtime_lock
            .pointer("/layoutModel/url")
            .and_then(serde_json::Value::as_str),
        Some(PADDLE_LAYOUT_MODEL_URL)
    );
    assert_eq!(
        runtime_lock
            .pointer("/layoutModel/sha256")
            .and_then(serde_json::Value::as_str),
        Some(PADDLE_LAYOUT_MODEL_SHA256)
    );
    for (os, arch) in [
        ("macos", "aarch64"),
        ("windows", "x86_64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
    ] {
        let artifact = python_artifact_for(os, arch)
            .unwrap_or_else(|| panic!("missing Python artifact for {os}/{arch}"));
        assert!(artifact.lock.contains("requires-python = \">=3.12.13\""));
        assert!(artifact
            .lock
            .contains("name = \"paddleocr\"\nversion = \"3.7.0\""));
        assert!(artifact
            .lock
            .contains("name = \"paddlepaddle\"\nversion = \"3.2.1\""));
        assert!(artifact.lock.contains("https://files.pythonhosted.org/"));
        let lock_arch = if os == "macos" && arch == "aarch64" {
            "arm64"
        } else {
            arch
        };
        let key = format!("{os}-{lock_arch}");
        let locked = runtime_lock
            .pointer(&format!("/python/artifacts/{key}"))
            .unwrap();
        assert_eq!(
            locked.get("sha256").and_then(serde_json::Value::as_str),
            Some(artifact.sha256)
        );
        assert_eq!(
            locked.get("lockSha256").and_then(serde_json::Value::as_str),
            Some(artifact.lock_sha256)
        );
        let uv = uv_artifact_for(os, arch).unwrap();
        let locked_uv = runtime_lock
            .pointer(&format!("/uv/artifacts/{key}"))
            .unwrap();
        assert_eq!(
            locked_uv.get("sha256").and_then(serde_json::Value::as_str),
            Some(uv.sha256)
        );
    }
}

#[test]
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn parser_sidecar_refresh_reuses_only_a_verified_pinned_runtime() {
    let root = tempfile::tempdir().unwrap();
    let version_root = root.path().join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    std::fs::create_dir_all(python.parent().unwrap()).unwrap();
    std::fs::write(&python, b"managed python fixture").unwrap();
    let packages = b"paddleocr==3.7.0\npaddlepaddle==3.2.1\n";
    std::fs::write(version_root.join("packages.txt"), packages).unwrap();
    let artifact = python_artifact().unwrap();
    std::fs::write(version_root.join("pylock.toml"), artifact.lock).unwrap();
    std::fs::write(
        version_root.join("runtime-lock.json"),
        PADDLE_PARSER_RUNTIME_LOCK,
    )
    .unwrap();
    let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
    std::fs::create_dir_all(&layout_model).unwrap();
    for name in ["inference.json", "inference.yml", "inference.pdiparams"] {
        std::fs::write(layout_model.join(name), b"fixture").unwrap();
    }
    let integrity_sha256 = write_install_integrity(
        &version_root,
        &[
            "venv",
            "models",
            "packages.txt",
            "pylock.toml",
            "runtime-lock.json",
        ],
    )
    .unwrap();
    let manifest = serde_json::json!({
        "engine": "paddleocr-vl-parser",
        "release": PADDLE_PARSER_RELEASE,
        "paddleocr": PADDLE_PARSER_VERSION,
        "paddlepaddle": PADDLE_RUNTIME_VERSION,
        "python": PYTHON_VERSION,
        "python_build_release": PYTHON_BUILD_RELEASE,
        "python_artifact_sha256": artifact.sha256,
        "uv": UV_VERSION,
        "lock_sha256": artifact.lock_sha256,
        "runtime_lock_sha256": format!("{:x}", Sha256::digest(PADDLE_PARSER_RUNTIME_LOCK.as_bytes())),
        "layout_model_artifact_sha256": PADDLE_LAYOUT_MODEL_SHA256,
        "layout_ready": true,
        "offline_model_runtime": true,
        "recognition_backend": "managed llama.cpp",
        "packages_sha256": format!("{:x}", Sha256::digest(packages)),
        "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
        "integrity_sha256": integrity_sha256,
    });
    std::fs::write(
        version_root.join("install.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();

    assert!(parser_runtime_reusable_for_sidecar_refresh(root.path()));
    assert!(refresh_paddle_parser_sidecar_locked(root.path()).unwrap());
    assert!(verified_parser_sidecar_at(root.path()).is_some());
    assert!(!refresh_paddle_parser_sidecar_locked(root.path()).unwrap());
    std::fs::write(version_root.join("packages.txt"), b"changed").unwrap();
    assert!(!parser_runtime_reusable_for_sidecar_refresh(root.path()));
}

#[test]
fn parser_runtime_provisioned_by_an_older_app_build_still_verifies() {
    // A runtime installed by an older app build carries the lock contents and
    // provisioning provenance of that build. The release name is the
    // compatibility contract; everything else must verify against the
    // install-time record, not this binary's constants.
    let root = tempfile::tempdir().unwrap();
    let version_root = root.path().join("versions").join(PADDLE_PARSER_RELEASE);
    let python = venv_python(&version_root.join("venv"));
    std::fs::create_dir_all(python.parent().unwrap()).unwrap();
    std::fs::write(&python, b"managed python fixture").unwrap();
    let packages = b"paddleocr==3.7.0\npaddlepaddle==3.2.1\n";
    std::fs::write(version_root.join("packages.txt"), packages).unwrap();
    let legacy_lock = b"# pylock written by an older app build\n";
    std::fs::write(version_root.join("pylock.toml"), legacy_lock).unwrap();
    let legacy_runtime_lock = br#"{"note": "runtime lock from an older app build"}"#;
    std::fs::write(version_root.join("runtime-lock.json"), legacy_runtime_lock).unwrap();
    let layout_model = version_root.join("models").join("PP-DocLayoutV3_infer");
    std::fs::create_dir_all(&layout_model).unwrap();
    for name in ["inference.json", "inference.yml", "inference.pdiparams"] {
        std::fs::write(layout_model.join(name), b"fixture").unwrap();
    }
    let integrity_sha256 = write_install_integrity(
        &version_root,
        &[
            "venv",
            "models",
            "packages.txt",
            "pylock.toml",
            "runtime-lock.json",
        ],
    )
    .unwrap();
    let manifest = serde_json::json!({
        "engine": "paddleocr-vl-parser",
        "release": PADDLE_PARSER_RELEASE,
        "paddleocr": PADDLE_PARSER_VERSION,
        "paddlepaddle": PADDLE_RUNTIME_VERSION,
        "python": PYTHON_VERSION,
        "python_build_release": "20240101",
        "python_artifact_sha256": "0".repeat(64),
        "uv": "0.0.1",
        "lock_sha256": format!("{:x}", Sha256::digest(legacy_lock)),
        "runtime_lock_sha256": format!("{:x}", Sha256::digest(legacy_runtime_lock)),
        "layout_model_artifact_sha256": "1".repeat(64),
        "layout_ready": true,
        "offline_model_runtime": true,
        "recognition_backend": "managed llama.cpp",
        "packages_sha256": format!("{:x}", Sha256::digest(packages)),
        "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
        "integrity_sha256": integrity_sha256,
    });
    let manifest_path = version_root.join("install.json");
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

    assert!(parser_runtime_reusable_for_sidecar_refresh(root.path()));

    // Tampered lock contents must still fail: on-disk files are pinned to the
    // digests recorded at install time.
    std::fs::write(version_root.join("runtime-lock.json"), b"{}").unwrap();
    assert!(!parser_runtime_reusable_for_sidecar_refresh(root.path()));
    std::fs::write(version_root.join("runtime-lock.json"), legacy_runtime_lock).unwrap();
    assert!(parser_runtime_reusable_for_sidecar_refresh(root.path()));
    std::fs::write(version_root.join("pylock.toml"), b"# tampered").unwrap();
    assert!(!parser_runtime_reusable_for_sidecar_refresh(root.path()));
    std::fs::write(version_root.join("pylock.toml"), legacy_lock).unwrap();
    assert!(parser_runtime_reusable_for_sidecar_refresh(root.path()));

    // A different release is a different contract and never reused.
    let mut renamed = manifest.clone();
    renamed["release"] = serde_json::Value::String("some-other-release".to_string());
    std::fs::write(&manifest_path, serde_json::to_vec(&renamed).unwrap()).unwrap();
    assert!(!parser_runtime_reusable_for_sidecar_refresh(root.path()));
}

#[test]
fn install_integrity_detects_same_size_file_replacement() {
    let root = tempfile::tempdir().unwrap();
    let runtime = root.path().join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    let program = runtime.join("program");
    std::fs::write(&program, b"trusted").unwrap();
    // Opening ~/.pipeline/ in Finder may add this inert metadata file.
    // It must neither enter nor invalidate the executable inventory.
    std::fs::write(runtime.join(".DS_Store"), b"finder metadata").unwrap();
    let expected_file_sha256 = format!("{:x}", Sha256::digest(b"trusted"));
    let manifest_sha256 = write_install_integrity(root.path(), &["runtime"]).unwrap();
    verify_install_integrity(
        root.path(),
        &manifest_sha256,
        &[("runtime/program", expected_file_sha256.as_str())],
    )
    .unwrap();
    // Exercise the metadata-gated verification cache before replacing the
    // file with a different inode and the same byte length.
    verify_install_integrity(root.path(), &manifest_sha256, &[]).unwrap();
    let injected = runtime.join("injected.py");
    std::fs::write(&injected, b"import me").unwrap();
    assert!(verify_install_integrity(root.path(), &manifest_sha256, &[])
        .unwrap_err()
        .contains("file inventory changed"));
    std::fs::remove_file(injected).unwrap();
    verify_install_integrity(root.path(), &manifest_sha256, &[]).unwrap();
    let replacement = runtime.join("replacement");
    std::fs::write(&replacement, b"changed").unwrap();
    std::fs::remove_file(&program).unwrap();
    std::fs::rename(&replacement, &program).unwrap();
    assert!(verify_install_integrity(root.path(), &manifest_sha256, &[])
        .unwrap_err()
        .contains("content changed"));
}

#[test]
fn install_space_is_incremental_and_includes_staging_headroom() {
    assert_eq!(
        install_space_requirement("paddleocr-vl", false, false, false).unwrap(),
        (InstallSpacePlan::BaseInstallOrRepair, 2900)
    );
    assert_eq!(
        install_space_requirement("paddleocr-vl-parser", false, false, false).unwrap(),
        (InstallSpacePlan::ParserFreshStack, 4500)
    );
    let add_on = install_space_requirement("paddleocr-vl-parser", true, false, false).unwrap();
    assert_eq!(add_on, (InstallSpacePlan::ParserAddOn, 2200));
    assert!(add_on.1 < engine("paddleocr-vl-parser").unwrap().est_disk_mb);
    assert_eq!(
        install_space_requirement("paddleocr-vl-parser", true, true, false).unwrap(),
        (InstallSpacePlan::ParserRepairOrUpgrade, 2200)
    );
    assert_eq!(
        install_space_requirement("paddleocr-vl-parser", true, true, true).unwrap(),
        (InstallSpacePlan::ParserSidecarRefresh, 16)
    );
    assert!(check_install_space(2_200_000_000, add_on.0, add_on.1).is_ok());
    assert!(check_install_space(2_199_999_999, add_on.0, add_on.1).is_err());
}

#[test]
fn archive_paths_reject_traversal_and_absolute_paths() {
    assert!(safe_archive_path(Path::new("build/bin/llama-server")));
    assert!(!safe_archive_path(Path::new("../llama-server")));
    assert!(!safe_archive_path(Path::new("/tmp/llama-server")));
    assert!(archive_link_stays_within_root(
        Path::new("python/share/terminfo/x/xterm"),
        Path::new("../78/xterm")
    ));
    assert!(!archive_link_stays_within_root(
        Path::new("python/link"),
        Path::new("../../outside")
    ));
}

#[test]
fn llama_archive_entry_limit_rejects_inode_exhaustion() {
    assert!(ensure_llama_archive_entry_count(MAX_LLAMA_ARCHIVE_ENTRIES).is_ok());
    assert!(ensure_llama_archive_entry_count(MAX_LLAMA_ARCHIVE_ENTRIES + 1).is_err());
}

#[test]
fn managed_environment_drops_ambient_python_and_package_manager_overrides() {
    let mut command = std::process::Command::new("managed-program");
    command
        .env("PYTHONPATH", "/tmp/injected")
        .env("PYTHONHOME", "/tmp/injected-python")
        .env("PIP_INDEX_URL", "https://attacker.invalid/simple")
        .env("UV_INDEX", "https://attacker.invalid/simple");
    apply_managed_environment(
        &mut command,
        &[("UV_NO_CONFIG".to_string(), "1".to_string())],
    );
    let environment = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().to_string(),
                value.map(|item| item.to_string_lossy().to_string()),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    for forbidden in ["PYTHONPATH", "PYTHONHOME", "PIP_INDEX_URL", "UV_INDEX"] {
        assert!(!environment.contains_key(forbidden));
    }
    assert_eq!(
        environment.get("UV_NO_CONFIG").and_then(Option::as_deref),
        Some("1")
    );
    assert!(environment.get("PATH").and_then(Option::as_deref).is_some());
}

#[test]
fn paddle_parser_environment_has_an_app_owned_home_on_every_platform() {
    let cache = Path::new("managed-parser-cache");
    let environment = paddle_full_parser_env_at(cache)
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    let expected = cache.to_string_lossy();
    assert_eq!(
        environment.get("HOME").map(String::as_str),
        Some(expected.as_ref())
    );
    assert_eq!(
        environment.get("USERPROFILE").map(String::as_str),
        Some(expected.as_ref())
    );
    assert_eq!(
        environment.get("PADDLE_PDX_CACHE_HOME").map(String::as_str),
        Some(expected.as_ref())
    );
}

#[test]
fn installer_output_drain_bounds_lines_and_total_log_bytes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncWriteExt as _;

        let (mut writer, reader) = tokio::io::duplex(32 * 1024);
        let writer_task = tokio::spawn(async move {
            let chunk = vec![b'x'; 32 * 1024];
            for _ in 0..160 {
                writer.write_all(&chunk).await.unwrap();
            }
            writer.write_all(b"\n").await.unwrap();
        });
        let emitted = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured = emitted.clone();
        let stats = drain_install_output(reader, move |line| {
            captured.lock().unwrap().push(line);
        })
        .await
        .unwrap();
        writer_task.await.unwrap();
        assert_eq!(stats.bytes_read, 160 * 32 * 1024 + 1);
        assert!(stats.bytes_emitted <= INSTALL_LOG_BYTES_PER_STREAM);
        assert!(stats.truncated);
        let lines = emitted.lock().unwrap();
        assert!(lines.iter().all(|line| {
            line.len() <= INSTALL_LOG_LINE_BYTES
                || line == "[installer output truncated; remaining bytes were drained]"
        }));
    });
}

#[test]
fn installer_output_join_has_a_post_exit_deadline() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut stdout = tokio::spawn(async {
            std::future::pending::<Result<InstallDrainStats, String>>().await
        });
        let mut stderr = tokio::spawn(async { Ok(InstallDrainStats::default()) });
        let started = std::time::Instant::now();
        let error = finish_install_drains(
            0,
            &mut stdout,
            &mut stderr,
            std::time::Duration::from_millis(20),
            std::time::Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(error.contains("did not close"));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(stdout.await.unwrap_err().is_cancelled());
    });
}

#[cfg(unix)]
#[test]
fn llama_unpack_preserves_safe_runtime_symlinks() {
    use std::io::Cursor;

    let temp = tempfile::tempdir().unwrap();
    let archive_path = temp.path().join("runtime.tar.gz");
    let archive_file = std::fs::File::create(&archive_path).unwrap();
    let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, bytes, mode) in [
        ("bundle/llama-server", b"server".as_slice(), 0o755),
        ("bundle/libfoo.1.dylib", b"library".as_slice(), 0o644),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder
            .append_data(&mut header, name, Cursor::new(bytes))
            .unwrap();
    }
    let mut link = tar::Header::new_gnu();
    link.set_size(0);
    link.set_mode(0o777);
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_link_name("libfoo.1.dylib").unwrap();
    link.set_cksum();
    builder
        .append_data(
            &mut link,
            "bundle/libfoo.dylib",
            Cursor::new(Vec::<u8>::new()),
        )
        .unwrap();
    builder.into_inner().unwrap().finish().unwrap();

    let destination = temp.path().join("unpacked");
    unpack_llama_archive(&archive_path, &destination, false).unwrap();
    assert_eq!(
        std::fs::read(destination.join("bundle/libfoo.dylib")).unwrap(),
        b"library"
    );
    assert!(
        std::fs::symlink_metadata(destination.join("bundle/libfoo.dylib"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn dir_size_sums_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), vec![0u8; 1000]).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/b.bin"), vec![0u8; 500]).unwrap();
    assert_eq!(dir_size(dir.path()), 1500);
    assert_eq!(dir_size(&dir.path().join("missing")), 0);
}

#[cfg(unix)]
#[test]
fn dir_size_does_not_follow_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("inside"), vec![0u8; 10]).unwrap();
    std::fs::write(outside.path().join("outside"), vec![0u8; 1_000]).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(root.path(), root.path().join("cycle")).unwrap();
    assert_eq!(dir_size(root.path()), 10);
}
