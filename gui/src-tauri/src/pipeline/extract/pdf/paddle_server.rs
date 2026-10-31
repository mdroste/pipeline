//! Managed recognition server lifetime and full-parser sidecar invocation.

use super::super::core::{run_bounded_output, BoundedOutput};
use super::page_quality::PdfPageTextBaseline;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

pub(in crate::pipeline::extract) const PADDLE_MODEL_ALIAS: &str = "paddleocr-vl-1.6";
pub(in crate::pipeline::extract) const MAX_PADDLE_MODEL_LIST_BYTES: usize = 1024 * 1024;
pub(in crate::pipeline::extract) const PADDLE_READINESS_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

pub(in crate::pipeline::extract) fn paddle_server_token() -> Result<String, String> {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("Failed to generate managed server credentials: {error}"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut token, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(token)
}

pub(in crate::pipeline::extract) fn paddle_model_list_has_alias(value: &serde_json::Value) -> bool {
    value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|models| {
            models.iter().any(|model| {
                model.get("id").and_then(serde_json::Value::as_str) == Some(PADDLE_MODEL_ALIAS)
            })
        })
}

/// Where the sidecar sends page regions for recognition.
pub(in crate::pipeline::extract) enum SidecarRecognition {
    Server { base_url: String, api_key: String },
    BuiltIn { model_dir: PathBuf },
}

pub(in crate::pipeline::extract) struct PaddleServer {
    child: std::process::Child,
    pid: u32,
    pub(super) base_url: String,
    pub(super) api_key: String,
}

impl Drop for PaddleServer {
    fn drop(&mut self) {
        if self.pid > 0 {
            crate::commands::kill_process(self.pid);
            crate::commands::unregister_child_pid(self.pid);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(in crate::pipeline::extract) async fn start_paddle_server(
    paths: &crate::engines::PaddleEnginePaths,
    settings: &crate::settings::Settings,
) -> Result<PaddleServer, String> {
    use std::process::Stdio;

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Failed to reserve a PaddleOCR-VL port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Failed to inspect the PaddleOCR-VL port: {error}"))?
        .port();
    drop(listener);

    let model = paths
        .model
        .to_str()
        .ok_or("PaddleOCR-VL model path is not valid UTF-8")?;
    let projector = paths
        .mmproj
        .to_str()
        .ok_or("PaddleOCR-VL projector path is not valid UTF-8")?;
    let port = port.to_string();
    let resolved_concurrency = crate::settings::resolved_paddle_page_concurrency(settings);
    let page_concurrency = resolved_concurrency.to_string();
    // llama.cpp's --ctx-size is the total KV budget shared by all parallel
    // slots. Scale it with concurrency so a two-page run still gives every
    // page the model's full 16K context.
    let total_context = (16_384u32 * resolved_concurrency).to_string();
    let mtmd_batch_tokens =
        crate::settings::resolved_paddle_mtmd_batch_tokens(settings).to_string();
    let api_key = paddle_server_token()?;
    let mut command = StdCommand::new(&paths.server);
    command.args([
        "-m",
        model,
        "--mmproj",
        projector,
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "--temp",
        "0",
        "--ctx-size",
        &total_context,
        "--n-gpu-layers",
        "99",
        "--parallel",
        &page_concurrency,
        "--mtmd-batch-max-tokens",
        &mtmd_batch_tokens,
        "--flash-attn",
        &settings.paddle_flash_attention,
        "--alias",
        PADDLE_MODEL_ALIAS,
        "--api-key",
        &api_key,
        "--no-ui",
    ]);
    crate::agent_runtime::process::configure_silent_command(&mut command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start managed llama-server: {error}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let base_url = format!("http://127.0.0.1:{port}");
    let client = &*crate::pipeline::api_common::LOCAL_HTTP_CLIENT;
    let started = std::time::Instant::now();
    loop {
        if crate::commands::is_cancelled() {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("Pipeline cancelled".to_string());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to inspect llama-server: {error}"))?
        {
            if pid > 0 {
                crate::commands::unregister_child_pid(pid);
            }
            return Err(format!(
                "Managed llama-server exited during model loading (exit {})",
                status.code().unwrap_or(-1)
            ));
        }
        if let Ok(response) = client
            .get(format!("{base_url}/health"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            if response.status().is_success() {
                let identity = client
                    .get(format!("{base_url}/v1/models"))
                    .bearer_auth(&api_key)
                    .timeout(std::time::Duration::from_secs(2))
                    .send()
                    .await;
                if let Ok(identity) = identity {
                    if identity.status().is_success() {
                        if let Ok(bytes) = crate::pipeline::api_common::response_bytes_limited(
                            identity,
                            MAX_PADDLE_MODEL_LIST_BYTES,
                            None,
                        )
                        .await
                        {
                            if serde_json::from_slice::<serde_json::Value>(&bytes)
                                .ok()
                                .as_ref()
                                .is_some_and(paddle_model_list_has_alias)
                            {
                                return Ok(PaddleServer {
                                    child,
                                    pid,
                                    base_url,
                                    api_key,
                                });
                            }
                        }
                    }
                }
            }
        }
        if started.elapsed() >= std::time::Duration::from_secs(180) {
            if pid > 0 {
                crate::commands::kill_process(pid);
                crate::commands::unregister_child_pid(pid);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err("PaddleOCR-VL model loading timed out after 180 seconds".to_string());
        }
        let _ = crate::commands::await_or_cancel(
            tokio::time::sleep(PADDLE_READINESS_POLL_INTERVAL),
            None,
        )
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::pipeline::extract) fn run_paddle_full_sidecar(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
    input: &Path,
    output: &Path,
    assets: &Path,
    recognition: &SidecarRecognition,
    baselines: &[PdfPageTextBaseline],
    timeout: std::time::Duration,
) -> Result<BoundedOutput, String> {
    let boolean = |value: bool| if value { "true" } else { "false" };
    let baseline_chars = baselines
        .iter()
        .map(|baseline| baseline.total_chars)
        .collect::<Vec<_>>();
    let baseline_prose_chars = baselines
        .iter()
        .map(|baseline| baseline.prose_chars)
        .collect::<Vec<_>>();
    let baseline_chars = serde_json::to_string(&baseline_chars)
        .map_err(|error| format!("Failed to serialize PDF text-layer baseline: {error}"))?;
    let baseline_prose_chars = serde_json::to_string(&baseline_prose_chars)
        .map_err(|error| format!("Failed to serialize PDF prose-line baseline: {error}"))?;
    let mut command = StdCommand::new(&paths.python);
    let mut arguments: Vec<std::ffi::OsString> = vec![
        "-I".into(),
        "-B".into(),
        paths.script.as_os_str().to_owned(),
        "--input".into(),
        input.as_os_str().to_owned(),
        "--output".into(),
        output.as_os_str().to_owned(),
        "--assets-dir".into(),
        assets.as_os_str().to_owned(),
        "--layout-model-dir".into(),
        paths.layout_model.as_os_str().to_owned(),
        "--concurrency".into(),
        crate::settings::resolved_paddle_page_concurrency(settings)
            .to_string()
            .into(),
        "--max-new-tokens".into(),
        settings.paddle_max_output_tokens.to_string().into(),
        "--page-retries".into(),
        settings.paddle_page_retries.to_string().into(),
        "--baseline-chars".into(),
        baseline_chars.into(),
        "--baseline-prose-chars".into(),
        baseline_prose_chars.into(),
        "--layout-detection".into(),
        boolean(settings.paddle_full_layout_detection).into(),
        "--layout-threshold".into(),
        settings.paddle_full_layout_threshold.to_string().into(),
        "--layout-nms".into(),
        boolean(settings.paddle_full_layout_nms).into(),
        "--layout-merge-bboxes-mode".into(),
        settings.paddle_full_layout_merge_bboxes_mode.clone().into(),
        "--merge-layout-blocks".into(),
        boolean(settings.paddle_full_merge_layout_blocks).into(),
        "--ocr-image-blocks".into(),
        boolean(settings.paddle_full_ocr_image_blocks).into(),
        "--format-block-content".into(),
        boolean(settings.paddle_full_format_block_content).into(),
        "--merge-tables".into(),
        boolean(settings.paddle_full_merge_tables).into(),
        "--relevel-titles".into(),
        boolean(settings.paddle_full_relevel_titles).into(),
        "--show-formula-numbers".into(),
        boolean(settings.paddle_full_show_formula_numbers).into(),
    ];
    let mut environment = crate::engines::paddle_full_parser_env(paths);
    match recognition {
        SidecarRecognition::Server { base_url, api_key } => {
            arguments.extend([
                "--server-url".into(),
                base_url.into(),
                "--server-model".into(),
                PADDLE_MODEL_ALIAS.into(),
            ]);
            environment.push(("PIPELINE_PADDLE_API_KEY".to_string(), api_key.clone()));
        }
        SidecarRecognition::BuiltIn { model_dir } => {
            arguments.push("--native-model-dir".into());
            arguments.push(model_dir.as_os_str().to_owned());
            // Paddle's pool otherwise keeps what loading the weights freed,
            // nearly doubling the memory held for the whole run.
            environment.push(("FLAGS_use_system_allocator".to_string(), "1".to_string()));
        }
    }
    command.args(arguments);
    crate::engines::apply_managed_environment(&mut command, &environment);
    if let Some(parent) = output.parent() {
        command.current_dir(parent);
    }
    run_bounded_output(
        command,
        "PaddleOCR-VL full parser",
        timeout,
        4 * 1024 * 1024,
        output.parent(),
    )
}
