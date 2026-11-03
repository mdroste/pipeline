//! Managed recognition server lifetime and full-parser sidecar invocation.

use super::super::core::{run_bounded_output_observed, BoundedOutput};
use super::page_quality::PdfPageTextBaseline;
use super::server_log::ServerLog;
use crate::engines::native_path;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

/// Mirrors `PROGRESS_PREFIX` in the parser sidecar, which marks the stdout
/// lines meant for the console.
pub(in crate::pipeline::extract) const PADDLE_PROGRESS_PREFIX: &str = "pipeline-progress: ";

pub(in crate::pipeline::extract) const PADDLE_MODEL_ALIAS: &str = "paddleocr-vl-1.6";
pub(in crate::pipeline::extract) const MAX_PADDLE_MODEL_LIST_BYTES: usize = 1024 * 1024;
pub(in crate::pipeline::extract) const PADDLE_READINESS_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(250);

/// Hosts the parser reaches directly, whatever proxy the computer uses.
const LOCAL_HOSTS: &str = "127.0.0.1,localhost";

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
    pub(super) log: ServerLog,
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
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start managed llama-server: {error}"))?;
    let pid = child.id();
    if pid > 0 {
        crate::commands::register_child_pid(pid);
    }
    let log = match child.stderr.take() {
        Some(stderr) => ServerLog::capture(stderr, api_key.clone()),
        None => ServerLog::default(),
    };
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
            // Its last words may still be in the pipe.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            return Err(with_reason(
                format!(
                    "Managed llama-server exited during model loading (exit {})",
                    status.code().unwrap_or(-1)
                ),
                &log,
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
                                    log,
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
            return Err(with_reason(
                "PaddleOCR-VL model loading timed out after 180 seconds".to_string(),
                &log,
            ));
        }
        let _ = crate::commands::await_or_cancel(
            tokio::time::sleep(PADDLE_READINESS_POLL_INTERVAL),
            None,
        )
        .await;
    }
}

fn with_reason(error: String, log: &ServerLog) -> String {
    match log.reason() {
        Some(reason) => format!("{error}: {reason}"),
        None => error,
    }
}

/// The parser's environment for one extraction. Everything it reads stays on
/// this computer, so its proxy exclusions can replace the researcher's.
fn sidecar_environment(
    paths: &crate::engines::PaddleFullParserPaths,
    settings: &crate::settings::Settings,
    recognition: &SidecarRecognition,
) -> Vec<(String, String)> {
    let mut environment = crate::engines::paddle_full_parser_env(paths);
    // Paddle defaults to one OpenMP thread as it starts, and layout runs on
    // the processor whichever recognition reads the pages. Only builds with
    // OpenMP and MKL, such as Windows, read these. Apple's build ignores them.
    let cores = crate::settings::resolved_paddle_cpu_threads(settings).to_string();
    environment.push(("OMP_NUM_THREADS".to_string(), cores.clone()));
    environment.push(("MKL_NUM_THREADS".to_string(), cores));
    match recognition {
        SidecarRecognition::Server { api_key, .. } => {
            environment.push(("PIPELINE_PADDLE_API_KEY".to_string(), api_key.clone()));
            // Recognition requests must reach the local server, not an ambient
            // proxy. Setting these also stops Python reading a system proxy.
            for name in ["NO_PROXY", "no_proxy"] {
                environment.push((name.to_string(), LOCAL_HOSTS.to_string()));
            }
        }
        SidecarRecognition::BuiltIn { .. } => {
            // Paddle's pool otherwise keeps what loading the weights freed,
            // nearly doubling the memory held for the whole run.
            environment.push(("FLAGS_use_system_allocator".to_string(), "1".to_string()));
        }
    }
    environment
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
    on_progress: &mut dyn FnMut(&str),
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
        native_path(input).into_os_string(),
        "--output".into(),
        native_path(output).into_os_string(),
        "--assets-dir".into(),
        native_path(assets).into_os_string(),
        "--layout-model-dir".into(),
        native_path(&paths.layout_model).into_os_string(),
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
    if let SidecarRecognition::Server { base_url, .. } = recognition {
        arguments.extend([
            "--server-url".into(),
            base_url.into(),
            "--server-model".into(),
            PADDLE_MODEL_ALIAS.into(),
        ]);
    }
    if let SidecarRecognition::BuiltIn { model_dir } = recognition {
        arguments.push("--native-model-dir".into());
        arguments.push(native_path(model_dir).into_os_string());
    }
    let environment = sidecar_environment(paths, settings, recognition);
    command.args(arguments);
    crate::engines::apply_managed_environment(&mut command, &environment);
    if let Some(parent) = output.parent() {
        command.current_dir(parent);
    }
    run_bounded_output_observed(
        command,
        "PaddleOCR-VL full parser",
        timeout,
        4 * 1024 * 1024,
        output.parent(),
        Some(&mut |line| {
            if let Some(message) = line.strip_prefix(PADDLE_PROGRESS_PREFIX) {
                on_progress(message);
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser_paths() -> crate::engines::PaddleFullParserPaths {
        crate::engines::PaddleFullParserPaths {
            python: "python".into(),
            script: "sidecar.py".into(),
            model_cache: "cache".into(),
            layout_model: "layout".into(),
            recognition: crate::engines::PaddleRecognition::BuiltIn {
                model_dir: "model".into(),
            },
            release: "release".into(),
            integrity_sha256: "fixture".into(),
            sidecar_sha256: "fixture".into(),
        }
    }

    fn launched_environment(
        recognition: &SidecarRecognition,
    ) -> std::collections::HashMap<String, String> {
        let environment = sidecar_environment(
            &parser_paths(),
            &crate::settings::Settings::default(),
            recognition,
        );
        let mut command = StdCommand::new("python");
        crate::engines::apply_managed_environment(&mut command, &environment);
        command
            .get_envs()
            .filter_map(|(key, value)| {
                Some((
                    key.to_string_lossy().to_string(),
                    value?.to_string_lossy().to_string(),
                ))
            })
            .collect()
    }

    fn server() -> SidecarRecognition {
        SidecarRecognition::Server {
            base_url: "http://127.0.0.1:8080/v1".into(),
            api_key: "key".into(),
        }
    }

    fn built_in() -> SidecarRecognition {
        SidecarRecognition::BuiltIn {
            model_dir: "model".into(),
        }
    }

    #[test]
    fn local_server_requests_bypass_any_proxy() {
        let environment = launched_environment(&server());
        assert_eq!(environment["NO_PROXY"], "127.0.0.1,localhost");
        assert_eq!(environment["no_proxy"], "127.0.0.1,localhost");
    }

    #[test]
    fn layout_gets_the_configured_cores_with_either_recognition() {
        let settings = crate::settings::Settings::default();
        let cores = crate::settings::resolved_paddle_cpu_threads(&settings).to_string();
        for recognition in [server(), built_in()] {
            let environment = launched_environment(&recognition);
            assert_eq!(environment["OMP_NUM_THREADS"], cores);
            assert_eq!(environment["MKL_NUM_THREADS"], cores);
        }
    }

    #[test]
    fn the_allocator_flag_stays_with_built_in_recognition() {
        assert_eq!(
            launched_environment(&built_in())["FLAGS_use_system_allocator"],
            "1"
        );
        assert!(!launched_environment(&server()).contains_key("FLAGS_use_system_allocator"));
    }
}
