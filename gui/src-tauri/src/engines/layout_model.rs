//! The PP-DocLayoutV3 layout model for the Full Parser.
//!
//! Hugging Face serves the model's files at a pinned revision, and Paddle's own
//! archive holds the same bytes. Paddle's host is in Beijing, and a connection
//! to it from elsewhere can hold at tens of KB/s for the whole transfer without
//! ever stalling, so the archive is only the fallback. Either route leaves the
//! same files in the parser's `models` scope, where the install integrity
//! inventory binds them. The pins here repeat `runtime-lock.json`, and a test
//! holds them equal.

use super::*;
use std::future::Future;

const LAYOUT_MODEL_DIR: &str = "PP-DocLayoutV3_infer";
const LAYOUT_MODEL_LABEL: &str = "PP-DocLayoutV3 model";
const LAYOUT_MODEL_REVISION: &str = "7b48a7566925fa464281f930c58eee04fe2c862a";

pub(super) const LAYOUT_MODEL_FILES: &[NativeModelFile] = &[
    NativeModelFile {
        name: "inference.json",
        bytes: 1_196_890,
        sha256: "2b68367c5b312a03de5a6e1642c597c8f95165a7e40cd59c6700cf4a5042f4fd",
    },
    NativeModelFile {
        name: "inference.yml",
        bytes: 1_482,
        sha256: "506fcfac13b3b546ae40d7886b44126420f392adb694e3f8bb6a6286a1f90fdc",
    },
    NativeModelFile {
        name: "inference.pdiparams",
        bytes: 130_806_572,
        sha256: "70bd316b0582769ec968829fd1feb1a6a58b7c941b938327e551b6b12b45c137",
    },
];

pub(super) fn layout_model_url(filename: &str) -> String {
    format!(
        "https://huggingface.co/PaddlePaddle/PP-DocLayoutV3/resolve/{LAYOUT_MODEL_REVISION}/{filename}?download=true"
    )
}

/// Install the layout model under `version_root` and return its directory.
pub(super) async fn install_layout_model(
    app: &crate::emit::EventBus,
    version_root: &Path,
) -> Result<PathBuf, String> {
    let model = version_root.join("models").join(LAYOUT_MODEL_DIR);
    with_archive_fallback(
        app,
        &model,
        download_layout_model_files(app, &model),
        install_layout_model_archive(app, version_root),
        || INSTALL_CANCEL.load(Ordering::Acquire),
    )
    .await?;
    Ok(model)
}

/// Run `archive` only when `files` failed for a reason other than cancellation,
/// after clearing the partial model that `files` left behind.
async fn with_archive_fallback(
    app: &crate::emit::EventBus,
    model: &Path,
    files: impl Future<Output = Result<(), String>>,
    archive: impl Future<Output = Result<(), String>>,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    let Err(error) = files.await else {
        return Ok(());
    };
    if cancelled() {
        return Err(error);
    }
    log(
        app,
        format!("{error}. Trying the archive from PaddlePaddle instead"),
    );
    match std::fs::remove_dir_all(model) {
        Err(cleanup) if cleanup.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!(
                "Failed to clear the partial layout model: {cleanup}"
            ));
        }
        _ => {}
    }
    archive
        .await
        .map_err(|fallback| format!("{error}; {fallback}"))
}

async fn download_layout_model_files(
    app: &crate::emit::EventBus,
    model: &Path,
) -> Result<(), String> {
    std::fs::create_dir_all(model)
        .map_err(|error| format!("Failed to create layout model directory: {error}"))?;
    for file in LAYOUT_MODEL_FILES {
        download_verified(
            app,
            &[layout_model_url(file.name)],
            &model.join(file.name),
            file.sha256,
            file.bytes,
            LAYOUT_MODEL_LABEL,
        )
        .await?;
    }
    Ok(())
}

async fn install_layout_model_archive(
    app: &crate::emit::EventBus,
    version_root: &Path,
) -> Result<(), String> {
    let model_archive = version_root.join("PP-DocLayoutV3_infer.tar");
    download_verified(
        app,
        &[PADDLE_LAYOUT_MODEL_URL.to_string()],
        &model_archive,
        PADDLE_LAYOUT_MODEL_SHA256,
        MAX_LAYOUT_MODEL_ARCHIVE_BYTES,
        LAYOUT_MODEL_LABEL,
    )
    .await?;
    let models = version_root.join("models");
    let archive_for_task = model_archive.clone();
    tokio::task::spawn_blocking(move || unpack_layout_model_archive(&archive_for_task, &models))
        .await
        .map_err(|error| format!("Layout-model extraction task failed: {error}"))??;
    std::fs::remove_file(&model_archive)
        .map_err(|error| format!("Failed to remove staged model archive: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn events() -> crate::emit::EventBus {
        std::sync::Arc::new(crate::emit::NullEvents)
    }

    #[test]
    fn the_pinned_files_match_the_runtime_lock() {
        let lock: serde_json::Value = serde_json::from_str(PADDLE_PARSER_RUNTIME_LOCK).unwrap();
        let fallback = lock.pointer("/layoutModel/qualificationFallback").unwrap();
        let base_url = fallback["baseUrl"].as_str().unwrap();
        let revision = fallback["revision"].as_str().unwrap();
        let files = fallback["files"].as_object().unwrap();

        assert_eq!(revision, LAYOUT_MODEL_REVISION);
        assert_eq!(files.len(), LAYOUT_MODEL_FILES.len());
        for file in LAYOUT_MODEL_FILES {
            assert_eq!(
                files[file.name].as_str(),
                Some(file.sha256),
                "{}",
                file.name
            );
            assert_eq!(
                layout_model_url(file.name),
                format!("{base_url}/{revision}/{}?download=true", file.name)
            );
        }
    }

    #[tokio::test]
    async fn a_complete_file_download_never_touches_the_archive() {
        let archive_ran = AtomicBool::new(false);

        let result = with_archive_fallback(
            &events(),
            Path::new("unused"),
            async { Ok(()) },
            async {
                archive_ran.store(true, Ordering::SeqCst);
                Ok(())
            },
            || false,
        )
        .await;

        assert_eq!(result, Ok(()));
        assert!(!archive_ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn the_archive_replaces_a_partial_file_download() {
        let directory = tempfile::tempdir().unwrap();
        let model = directory.path().join(LAYOUT_MODEL_DIR);
        std::fs::create_dir(&model).unwrap();
        std::fs::write(model.join("inference.json"), b"partial").unwrap();

        // The archive refuses to extract over an existing file.
        let result = with_archive_fallback(
            &events(),
            &model,
            async { Err("huggingface.co: HTTP 503".to_string()) },
            async {
                assert!(!model.exists());
                Ok(())
            },
            || false,
        )
        .await;

        assert_eq!(result, Ok(()));
    }

    #[tokio::test]
    async fn the_archive_runs_when_the_model_directory_was_never_created() {
        let directory = tempfile::tempdir().unwrap();

        let result = with_archive_fallback(
            &events(),
            &directory.path().join(LAYOUT_MODEL_DIR),
            async { Err("first".to_string()) },
            async { Ok(()) },
            || false,
        )
        .await;

        assert_eq!(result, Ok(()));
    }

    #[tokio::test]
    async fn cancellation_does_not_start_the_archive() {
        let directory = tempfile::tempdir().unwrap();
        let archive_ran = AtomicBool::new(false);

        let result = with_archive_fallback(
            &events(),
            directory.path(),
            async { Err("Installation cancelled".to_string()) },
            async {
                archive_ran.store(true, Ordering::SeqCst);
                Ok(())
            },
            || true,
        )
        .await;

        assert_eq!(result, Err("Installation cancelled".to_string()));
        assert!(!archive_ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_failed_fallback_reports_both_sources() {
        let directory = tempfile::tempdir().unwrap();

        let result = with_archive_fallback(
            &events(),
            directory.path(),
            async { Err("huggingface.co: HTTP 503".to_string()) },
            async { Err("bj.bcebos.com: HTTP 403".to_string()) },
            || false,
        )
        .await;

        assert_eq!(
            result,
            Err("huggingface.co: HTTP 503; bj.bcebos.com: HTTP 403".to_string())
        );
    }

    #[tokio::test]
    #[ignore = "requires the public release hosts"]
    async fn pinned_layout_model_downloads_and_verifies_from_hugging_face() {
        let directory = tempfile::tempdir().unwrap();
        let model = directory.path().join(LAYOUT_MODEL_DIR);

        download_layout_model_files(&events(), &model)
            .await
            .unwrap();

        for file in LAYOUT_MODEL_FILES {
            let metadata = std::fs::metadata(model.join(file.name)).unwrap();
            assert_eq!(metadata.len(), file.bytes, "{}", file.name);
        }
    }
}
