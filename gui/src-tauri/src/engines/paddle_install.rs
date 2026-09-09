use super::*;

pub(super) async fn install_paddle_engine(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
) -> Result<(), String> {
    let artifact = llama_artifact()?;
    let native_dir = pipeline_home()?.join("native");
    std::fs::create_dir_all(&native_dir)
        .map_err(|error| format!("Failed to create native engine directory: {error}"))?;
    let staging = tempfile::Builder::new()
        .prefix(".paddleocr-vl-staging-")
        .tempdir_in(&native_dir)
        .map_err(|error| format!("Failed to create PaddleOCR-VL staging directory: {error}"))?;

    emit_phase(app, spec.id, "runtime", "running");
    let runtime_archive = staging.path().join(if artifact.filename.ends_with(".zip") {
        "llama-runtime.zip"
    } else {
        "llama-runtime.tar.gz"
    });
    if let Err(error) = download_verified(
        app,
        &llama_download_url(artifact),
        &runtime_archive,
        artifact.sha256,
        MAX_LLAMA_ARCHIVE_BYTES,
        "llama.cpp runtime",
    )
    .await
    {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err(error);
    }
    let runtime_dir = staging.path().join("runtime");
    let archive_for_task = runtime_archive.clone();
    let runtime_for_task = runtime_dir.clone();
    let is_zip = artifact.filename.ends_with(".zip");
    let unpacked = tokio::task::spawn_blocking(move || {
        unpack_llama_archive(&archive_for_task, &runtime_for_task, is_zip)
    })
    .await
    .map_err(|error| format!("Runtime extraction task failed: {error}"))?;
    if let Err(error) = unpacked {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err(error);
    }
    let _ = std::fs::remove_file(&runtime_archive);
    let staged_server = find_file_named(&runtime_dir, &exe("llama-server"))
        .ok_or("llama-server disappeared after extraction")?;
    let mut probe = std::process::Command::new(&staged_server);
    probe.arg("--version");
    let output =
        crate::process::run_bounded(&mut probe, std::time::Duration::from_secs(15), 256 * 1024)
            .map_err(|error| format!("llama-server validation failed: {error}"))?;
    if !output.status.success() || output.stdout_truncated || output.stderr_truncated {
        emit_phase(app, spec.id, "runtime", "failed");
        return Err("The downloaded llama-server failed its validation check".to_string());
    }
    emit_phase(app, spec.id, "runtime", "done");

    // This engine deliberately has no Python/package-manager layer.
    emit_phase(app, spec.id, "packages", "running");
    log(app, "Native engine: no Python packages required");
    emit_phase(app, spec.id, "packages", "done");

    emit_phase(app, spec.id, "models", "running");
    let model_dir = staging.path().join("models");
    std::fs::create_dir_all(&model_dir)
        .map_err(|error| format!("Failed to create model directory: {error}"))?;
    for (filename, sha256, label) in [
        (
            PADDLE_MODEL_FILE,
            PADDLE_MODEL_SHA256,
            "PaddleOCR-VL Q8 model",
        ),
        (
            PADDLE_MMPROJ_FILE,
            PADDLE_MMPROJ_SHA256,
            "PaddleOCR-VL vision projector",
        ),
    ] {
        if let Err(error) = download_verified(
            app,
            &paddle_model_url(filename),
            &model_dir.join(filename),
            sha256,
            MAX_PADDLE_MODEL_BYTES,
            label,
        )
        .await
        {
            emit_phase(app, spec.id, "models", "failed");
            return Err(error);
        }
    }
    emit_phase(app, spec.id, "models", "done");

    let integrity_sha256 = write_install_integrity(staging.path(), &["runtime", "models"])?;

    let manifest = serde_json::json!({
        "engine": "paddleocr-vl",
        "model_version": "1.6",
        "quantization": "Q8",
        "model_revision": PADDLE_MODEL_REVISION,
        "llama_cpp_version": LLAMA_CPP_VERSION,
        "runtime_archive": artifact.filename,
        "runtime_sha256": artifact.sha256,
        "model_sha256": PADDLE_MODEL_SHA256,
        "mmproj_sha256": PADDLE_MMPROJ_SHA256,
        "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
        "integrity_sha256": integrity_sha256,
    });
    std::fs::write(
        staging.path().join("install.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("Failed to serialize install manifest: {error}"))?,
    )
    .map_err(|error| format!("Failed to write install manifest: {error}"))?;
    if paddle_paths_at(staging.path()).is_none() {
        return Err("The staged PaddleOCR-VL installation is incomplete".to_string());
    }
    if INSTALL_CANCEL.load(Ordering::Acquire) {
        return Err("Install cancelled".to_string());
    }

    // Swap only after every checksum and executable check succeeds. If a prior
    // install exists, retain it as a same-filesystem backup until the rename.
    let target = paddle_root()?;
    let backup = native_dir.join(".paddleocr-vl-backup");
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to clean an old engine backup: {error}"))?;
    }
    if target.exists() {
        std::fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage the previous engine version: {error}"))?;
    }
    let staged_path = staging.keep();
    if let Err(error) = std::fs::rename(&staged_path, &target) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target);
        }
        return Err(format!("Failed to activate PaddleOCR-VL: {error}"));
    }
    if backup.exists() {
        let _ = std::fs::remove_dir_all(&backup);
    }
    let installed = paddle_engine_paths()?;
    log(
        app,
        format!("{} installed at {}", spec.label, installed.server.display()),
    );
    Ok(())
}

pub(super) async fn install_paddle_full_parser(
    app: &crate::emit::EventBus,
    spec: &EngineSpec,
) -> Result<(), String> {
    full_parser_support()?;
    let python_artifact = python_artifact()?;

    // The parser is an add-on to the native Q8 stack. Installing it is one
    // user action even on a fresh machine.
    if paddle_engine_paths().is_err() {
        let base = engine("paddleocr-vl")?;
        log(
            app,
            "Installing the managed PaddleOCR-VL model and llama.cpp dependency first",
        );
        install_paddle_engine(app, base).await?;
    }

    let parser_root = paddle_parser_root()?;
    match refresh_paddle_parser_sidecar_locked(&parser_root) {
        Ok(true) => {
            for phase in ["runtime", "packages", "models"] {
                emit_phase(app, spec.id, phase, "done");
            }
            let installed = paddle_full_parser_paths_at(&parser_root)
                .ok_or("The refreshed parser sidecar failed its integrity check")?;
            log(
                app,
                format!(
                    "{} sidecar updated without reinstalling its managed runtime ({})",
                    spec.label,
                    installed.script.display()
                ),
            );
            return Ok(());
        }
        Ok(false) => {}
        Err(error) => log(
            app,
            format!("Parser sidecar refresh was unavailable; rebuilding the runtime ({error})"),
        ),
    }

    emit_phase(app, spec.id, "runtime", "running");
    let uv = match ensure_uv(app).await {
        Ok(uv) => uv,
        Err(error) => {
            emit_phase(app, spec.id, "runtime", "failed");
            return Err(error);
        }
    };
    emit_phase(app, spec.id, "runtime", "done");

    let versions = parser_root.join("versions");
    let target = paddle_parser_version_root()?;
    let backup = versions.join(format!(".{PADDLE_PARSER_RELEASE}-backup"));
    std::fs::create_dir_all(&versions)
        .map_err(|error| format!("Failed to create parser versions directory: {error}"))?;
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|error| format!("Failed to remove an old parser backup: {error}"))?;
    }
    if target.exists() {
        std::fs::rename(&target, &backup)
            .map_err(|error| format!("Failed to stage the previous parser version: {error}"))?;
    }
    std::fs::create_dir_all(&target)
        .map_err(|error| format!("Failed to create parser version directory: {error}"))?;

    let provision_result: Result<(), String> = async {
        emit_phase(app, spec.id, "packages", "running");
        let mut environment = uv_env()?;
        let model_cache = target.join("cache");
        std::fs::create_dir_all(&model_cache)
            .map_err(|error| format!("Failed to create parser cache: {error}"))?;
        environment.extend(paddle_full_parser_env_at(&model_cache));

        let lock = target.join("pylock.toml");
        std::fs::write(&lock, python_artifact.lock)
            .map_err(|error| format!("Failed to write parser lock: {error}"))?;
        let lock_sha256 = sha256_regular_file(&lock, 4 * 1024 * 1024)?;
        if lock_sha256 != python_artifact.lock_sha256 {
            return Err("Embedded parser lock failed its integrity check".to_string());
        }
        let runtime_lock = target.join("runtime-lock.json");
        std::fs::write(&runtime_lock, PADDLE_PARSER_RUNTIME_LOCK)
            .map_err(|error| format!("Failed to write parser runtime lock: {error}"))?;
        let runtime_lock_sha256 = sha256_regular_file(&runtime_lock, 1024 * 1024)?;

        let python_archive = target.join("python.tar.gz");
        download_verified(
            app,
            &python_download_url(python_artifact),
            &python_archive,
            python_artifact.sha256,
            MAX_PYTHON_ARCHIVE_BYTES,
            "CPython runtime",
        )
        .await?;
        let cpython = target.join("cpython");
        let archive_for_task = python_archive.clone();
        let cpython_for_task = cpython.clone();
        let base_python = tokio::task::spawn_blocking(move || {
            unpack_python_archive(&archive_for_task, &cpython_for_task)
        })
        .await
        .map_err(|error| format!("Python extraction task failed: {error}"))??;
        std::fs::remove_file(&python_archive)
            .map_err(|error| format!("Failed to remove staged Python archive: {error}"))?;

        let venv = target.join("venv");
        let python = venv_python(&venv);
        let venv_args = vec![
            "venv".to_string(),
            venv.to_string_lossy().to_string(),
            "--python".to_string(),
            base_python.to_string_lossy().to_string(),
            "--no-python-downloads".to_string(),
            "--no-managed-python".to_string(),
            "--no-project".to_string(),
        ];
        log(
            app,
            format!("Creating a private verified Python {PYTHON_VERSION} runtime"),
        );
        run_install_step(app, &uv, &venv_args, &environment, "parser runtime install").await?;

        let sync_args = vec![
            "pip".to_string(),
            "sync".to_string(),
            "--python".to_string(),
            python.to_string_lossy().to_string(),
            "--strict".to_string(),
            "--require-hashes".to_string(),
            "--no-build".to_string(),
            "--no-index".to_string(),
            "--no-sources".to_string(),
            "--no-python-downloads".to_string(),
            "--no-managed-python".to_string(),
            lock.to_string_lossy().to_string(),
        ];
        log(
            app,
            format!(
                "Installing the checksum-locked PaddleOCR {PADDLE_PARSER_VERSION} runtime"
            ),
        );
        run_install_step(app, &uv, &sync_args, &environment, "parser package sync").await?;

        let check_args = vec![
            "pip".to_string(),
            "check".to_string(),
            "--python".to_string(),
            python.to_string_lossy().to_string(),
        ];
        run_install_step(app, &uv, &check_args, &environment, "parser dependency check").await?;

        let package_inventory = target.join("packages.txt");
        let inventory_code = r#"from importlib.metadata import distributions
from pathlib import Path
import sys
items = sorted(f"{name}=={dist.version}" for dist in distributions() if (name := dist.metadata.get("Name")))
Path(sys.argv[1]).write_text("\n".join(items) + "\n", encoding="utf-8")"#;
        let inventory_args = vec![
            "-I".to_string(),
            "-B".to_string(),
            "-c".to_string(),
            inventory_code.to_string(),
            package_inventory.to_string_lossy().to_string(),
        ];
        run_install_step(
            app,
            &python,
            &inventory_args,
            &environment,
            "parser package inventory",
        )
        .await?;
        let inventory_metadata = std::fs::metadata(&package_inventory)
            .map_err(|error| format!("Failed to inspect parser package inventory: {error}"))?;
        if inventory_metadata.len() > 2 * 1024 * 1024 {
            return Err("Parser package inventory exceeds its safety limit".to_string());
        }
        let inventory_bytes = std::fs::read(&package_inventory)
            .map_err(|error| format!("Failed to read parser package inventory: {error}"))?;
        let inventory_sha256 = format!("{:x}", Sha256::digest(&inventory_bytes));

        let script = target.join("paddle_parser_sidecar.py");
        std::fs::write(&script, PADDLE_PARSER_SCRIPT)
            .map_err(|error| format!("Failed to write parser sidecar: {error}"))?;
        let probe_args = vec![
            "-I".to_string(),
            "-B".to_string(),
            script.to_string_lossy().to_string(),
            "--probe".to_string(),
        ];
        run_install_step(app, &python, &probe_args, &environment, "parser import check").await?;
        emit_phase(app, spec.id, "packages", "done");

        emit_phase(app, spec.id, "models", "running");
        let model_archive = target.join("PP-DocLayoutV3_infer.tar");
        download_verified(
            app,
            PADDLE_LAYOUT_MODEL_URL,
            &model_archive,
            PADDLE_LAYOUT_MODEL_SHA256,
            MAX_LAYOUT_MODEL_ARCHIVE_BYTES,
            "PP-DocLayoutV3 model",
        )
        .await?;
        let models = target.join("models");
        let archive_for_task = model_archive.clone();
        let models_for_task = models.clone();
        let layout_model = tokio::task::spawn_blocking(move || {
            unpack_layout_model_archive(&archive_for_task, &models_for_task)
        })
        .await
        .map_err(|error| format!("Layout-model extraction task failed: {error}"))??;
        std::fs::remove_file(&model_archive)
            .map_err(|error| format!("Failed to remove staged model archive: {error}"))?;

        // A successful construction proves that the exact local weights can
        // initialize without a first-use download or recognition request.
        let warm_args = vec![
            "-I".to_string(),
            "-B".to_string(),
            script.to_string_lossy().to_string(),
            "--warm-layout".to_string(),
            "--layout-model-dir".to_string(),
            layout_model.to_string_lossy().to_string(),
            "--server-url".to_string(),
            "http://127.0.0.1:9/v1".to_string(),
        ];
        run_install_step(app, &python, &warm_args, &environment, "layout model validation")
            .await?;
        emit_phase(app, spec.id, "models", "done");

        // Bind every immutable interpreter, package, and model file into one
        // deterministic inventory. Mutable download/model caches are excluded.
        let integrity_sha256 = write_install_integrity(
            &target,
            &[
                "cpython",
                "venv",
                "models",
                "packages.txt",
                "pylock.toml",
                "runtime-lock.json",
            ],
        )?;

        let manifest = serde_json::json!({
            "engine": "paddleocr-vl-parser",
            "release": PADDLE_PARSER_RELEASE,
            "paddleocr": PADDLE_PARSER_VERSION,
            "paddlepaddle": PADDLE_RUNTIME_VERSION,
            "python": PYTHON_VERSION,
            "python_build_release": PYTHON_BUILD_RELEASE,
            "python_artifact_sha256": python_artifact.sha256,
            "uv": UV_VERSION,
            "lock_sha256": lock_sha256,
            "runtime_lock_sha256": runtime_lock_sha256,
            "sidecar_sha256": format!("{:x}", Sha256::digest(PADDLE_PARSER_SCRIPT.as_bytes())),
            "sidecar_contract": 2,
            "packages_sha256": inventory_sha256,
            "layout_model": "PP-DocLayoutV3",
            "layout_model_artifact_sha256": PADDLE_LAYOUT_MODEL_SHA256,
            "layout_ready": true,
            "offline_model_runtime": true,
            "recognition_backend": "managed llama.cpp",
            "integrity_schema": INSTALL_INTEGRITY_SCHEMA,
            "integrity_sha256": integrity_sha256,
        });
        std::fs::write(
            target.join("install.json"),
            serde_json::to_vec_pretty(&manifest)
                .map_err(|error| format!("Failed to serialize parser manifest: {error}"))?,
        )
        .map_err(|error| format!("Failed to write parser manifest: {error}"))?;
        paddle_full_parser_paths_at(&parser_root)
            .ok_or("The managed full-parser installation is incomplete")?;
        Ok(())
    }
    .await;

    if let Err(error) = provision_result {
        let _ = std::fs::remove_dir_all(&target);
        if backup.exists() {
            let _ = std::fs::rename(&backup, &target);
        }
        emit_phase(app, spec.id, "packages", "failed");
        return Err(error);
    }
    if backup.exists() {
        // A leftover backup is not fatal, but the next startup cleanup will
        // re-verify the new install against it, so surface the failure.
        if let Err(error) = std::fs::remove_dir_all(&backup) {
            log(
                app,
                format!(
                    "WARNING: could not remove the previous parser backup {}: {error}",
                    backup.display()
                ),
            );
        }
    }
    let installed = paddle_full_parser_paths_at(&parser_root)
        .ok_or("The managed full-parser installation is incomplete")?;
    log(
        app,
        format!("{} installed at {}", spec.label, installed.script.display()),
    );
    Ok(())
}
