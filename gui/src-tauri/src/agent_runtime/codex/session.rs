//! Store-free ownership of one native App Server connection.

use super::compatibility::{detected_version, MINIMUM_CODEX_VERSION};
use super::process::{capture_stderr, prepare_isolated_command, OwnedProcess};
use super::{AppServerClient, InitializeResult};
use std::path::Path;
use std::time::Duration;
use tokio::sync::Mutex;

pub(crate) struct NativeSession {
    pub client: AppServerClient,
    pub version: String,
    pub initialize: InitializeResult,
    pub executable: Option<String>,
    process: Option<OwnedProcess>,
    stderr: Mutex<Option<tokio::task::JoinHandle<Vec<String>>>>,
}

impl NativeSession {
    /// Configuration remains with the caller; launch the same resolved binary
    /// used to prepare its permission profile.
    pub async fn launch_resolved(
        resolved: &crate::deps::ResolvedCommand,
        home: &Path,
        cwd: &Path,
        epoch: u64,
        name: &str,
        title: &str,
    ) -> Result<Self, String> {
        let mut version_command = resolved.command(["--version"]);
        version_command.current_dir(std::env::temp_dir());
        let version = tokio::task::spawn_blocking(move || {
            let output = crate::process::run_bounded(
                &mut version_command,
                Duration::from_secs(15),
                64 * 1024,
            )?;
            if !output.status.success() || output.stdout_truncated {
                return Err("Cannot identify installed Codex version".to_string());
            }
            detected_version(&String::from_utf8_lossy(&output.stdout))
        })
        .await
        .map_err(|e| e.to_string())??;
        if !version.meets_minimum {
            return Err(format!(
                "Codex {} is too old; install {MINIMUM_CODEX_VERSION} or newer",
                version.version
            ));
        }
        let command = prepare_isolated_command(resolved, home, cwd)?;
        let (process, pipes) = OwnedProcess::spawn(command)?;
        let client = AppServerClient::from_io(epoch, pipes.stdout, pipes.stdin);
        let stderr = tokio::spawn(capture_stderr(pipes.stderr));
        let initialize = match client.initialize_as(name, title).await {
            Ok(value) => value,
            Err(error) => {
                process.terminate().await;
                let mut stderr = stderr;
                let diagnostics =
                    match tokio::time::timeout(Duration::from_secs(1), &mut stderr).await {
                        Ok(Ok(lines)) => lines.join("\n").chars().take(4000).collect::<String>(),
                        _ => {
                            stderr.abort();
                            String::new()
                        }
                    };
                return Err(if diagnostics.is_empty() {
                    error.to_string()
                } else {
                    format!("{error}: {diagnostics}")
                });
            }
        };
        if !same_existing_path(Path::new(&initialize.codex_home), home) {
            process.terminate().await;
            stderr.abort();
            return Err("Codex did not use the requested private credential namespace".into());
        }
        Ok(Self {
            client,
            version: version.version,
            initialize,
            executable: Some(resolved.discovered_path().display().to_string()),
            process: Some(process),
            stderr: Mutex::new(Some(stderr)),
        })
    }

    #[cfg(test)]
    pub(crate) fn simulated(client: AppServerClient) -> Self {
        Self {
            client,
            version: "simulated".into(),
            initialize: InitializeResult {
                user_agent: "simulated".into(),
                platform_family: "simulated".into(),
                platform_os: "simulated".into(),
                codex_home: "simulated".into(),
            },
            executable: None,
            process: None,
            stderr: Mutex::new(None),
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.process.as_ref().map(OwnedProcess::pid)
    }

    pub async fn shutdown(&self) {
        let _ = self.client.close_writer().await;
        if let Some(process) = &self.process {
            match tokio::time::timeout(Duration::from_secs(3), process.wait()).await {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(status)) => self.client.fail(
                    format!("Codex App Server exited with status {status}"),
                    true,
                ),
                Ok(Err(error)) => self.client.fail(error, true),
                Err(_) => {
                    process.terminate().await;
                    self.client.fail(
                        "Codex App Server required forced process-tree cleanup",
                        true,
                    );
                }
            }
        }
        self.client.fail("App Server stopped", false);
        if let Some(mut task) = self.stderr.lock().await.take() {
            if tokio::time::timeout(Duration::from_secs(1), &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
    }

    pub async fn terminate(&self) {
        if let Some(process) = &self.process {
            process.terminate().await;
        }
        self.client.fail(
            "App Server required forced cleanup; unfinished outcomes must be reconciled",
            false,
        );
    }
}

pub(crate) fn private_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Native runtime directory must not be a symlink".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("Missing runtime file parent")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Failure to resolve either path is never evidence that two namespaces match.
pub(crate) fn same_existing_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
