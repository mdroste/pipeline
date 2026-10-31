//! Installs a signed release from inside the app.
//!
//! The manifest attached to a release names this platform's package and its
//! signature. The Tauri updater downloads the package and verifies it against
//! the public key in `tauri.conf.json`, which also binds it to the announced
//! version, so the manifest itself needs no trust. The verified package waits
//! in memory until the researcher restarts: a download never interrupts work.
//! One install runs at a time, and failures leave the current app in place.
//!
//! Replacing the app is platform work the updater owns: an atomic bundle swap
//! on macOS, which may ask for an administrator password; the installer in
//! passive mode on Windows, which ends this process; and an in-place AppImage
//! swap on Linux.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::{Error, Update, UpdaterExt};
use tokio::sync::Notify;

use super::github::{self, Release};
use super::{state, version, CURRENT};
use crate::emit::EventBus;

/// Emitted with an [`InstallState`] whenever an install moves forward.
const UPDATE_INSTALL_EVENT: &str = "update-install";
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Covers the whole download, so it leaves room for slow connections.
const PACKAGE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// Well above the largest package a release has shipped.
const MAX_PACKAGE_BYTES: u64 = 1024 * 1024 * 1024;
/// Progress granularity when the server does not report a size.
const PROGRESS_STEP_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallPhase {
    Idle,
    Downloading,
    /// Verified and waiting for the researcher to restart.
    Ready,
    Installing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallState {
    pub phase: InstallPhase,
    pub version: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
}

const IDLE: InstallState = InstallState {
    phase: InstallPhase::Idle,
    version: None,
    downloaded: 0,
    total: None,
    error: None,
};

struct Job {
    state: InstallState,
    cancel: Option<Arc<Notify>>,
    package: Option<(Update, Vec<u8>)>,
}

static JOB: Mutex<Job> = Mutex::new(Job {
    state: IDLE,
    cancel: None,
    package: None,
});

fn job() -> MutexGuard<'static, Job> {
    JOB.lock().unwrap_or_else(|error| error.into_inner())
}

fn publish(bus: &EventBus, state: &InstallState) {
    if let Ok(payload) = serde_json::to_value(state) {
        let _ = bus.emit_event(UPDATE_INSTALL_EVENT, payload);
    }
}

pub fn state() -> InstallState {
    job().state.clone()
}

/// The manifest URL, revalidated because the cached release is outside the app
/// bundle. Without one the release can only be installed from the browser.
pub(super) fn manifest_url(release: &Release) -> Option<reqwest::Url> {
    release
        .manifest_url
        .as_deref()
        .filter(|url| github::is_manifest_url(url, &release.version))
        .and_then(|url| url.parse().ok())
}

fn saved_release() -> Result<Release, String> {
    state::load()?
        .latest
        .filter(|release| version::is_newer(&release.version, CURRENT))
        .ok_or_else(|| "No update is available.".to_string())
}

/// Start downloading the newest release. Returns at once; `update-install`
/// events report progress. A running install is left to finish.
pub fn start(app: &AppHandle) -> Result<InstallState, String> {
    if !super::schedule::automatic_supported() {
        return Err("This copy of Pipeline can't install updates itself.".to_string());
    }
    let release = saved_release()?;
    let manifest = manifest_url(&release)
        .ok_or_else(|| "This release can't be installed from inside Pipeline.".to_string())?;
    let bus = crate::emit::from_app(app.clone());
    let cancel = Arc::new(Notify::new());
    {
        let mut job = job();
        if matches!(
            job.state.phase,
            InstallPhase::Downloading | InstallPhase::Ready | InstallPhase::Installing
        ) {
            return Ok(job.state.clone());
        }
        job.state = InstallState {
            phase: InstallPhase::Downloading,
            version: Some(release.version.clone()),
            ..IDLE
        };
        job.cancel = Some(cancel.clone());
        publish(&bus, &job.state);
    }
    let app = app.clone();
    let events = bus.clone();
    tauri::async_runtime::spawn(async move {
        let oversized = Arc::new(AtomicBool::new(false));
        let download = fetch_package(&app, &release, manifest, &events, &cancel, &oversized);
        let outcome = tokio::select! {
            outcome = download => Some(outcome),
            () = cancel.notified() => oversized
                .load(Ordering::Acquire)
                .then(|| Err("The update is larger than expected, so it was not downloaded.".to_string())),
        };
        finish_download(&bus, outcome);
    });
    Ok(state())
}

async fn fetch_package(
    app: &AppHandle,
    release: &Release,
    manifest: reqwest::Url,
    bus: &EventBus,
    cancel: &Arc<Notify>,
    oversized: &Arc<AtomicBool>,
) -> Result<(Update, Vec<u8>), String> {
    let exit_app = app.clone();
    let updater = app
        .updater_builder()
        .endpoints(vec![manifest])
        .and_then(|builder| {
            builder
                .timeout(MANIFEST_TIMEOUT)
                .on_before_exit(move || {
                    crate::stop_runtimes();
                    exit_app.cleanup_before_exit();
                })
                .build()
        })
        .map_err(|error| failure_message(&error))?;
    let mut update = updater
        .check()
        .await
        .map_err(|error| failure_message(&error))?
        .ok_or_else(|| "This update is no longer available.".to_string())?;
    if update.version != release.version
        || !github::is_package_url(update.download_url.as_str(), &release.version)
    {
        return Err("The release's update information doesn't match the release.".to_string());
    }
    update.timeout = Some(PACKAGE_TIMEOUT);

    let mut progress = Progress::default();
    let bytes = update
        .download(
            |chunk, total| {
                let report = progress.record(chunk as u64, total);
                if progress.downloaded.max(total.unwrap_or(0)) > MAX_PACKAGE_BYTES {
                    oversized.store(true, Ordering::Release);
                    cancel.notify_one();
                }
                if report {
                    let mut job = job();
                    job.state.downloaded = progress.downloaded;
                    job.state.total = total;
                    publish(bus, &job.state);
                }
            },
            || {},
        )
        .await
        .map_err(|error| failure_message(&error))?;
    Ok((update, bytes))
}

/// Keeps a streaming download to about a hundred events.
#[derive(Default)]
struct Progress {
    downloaded: u64,
    reported: u64,
}

impl Progress {
    /// Adds a chunk and returns whether the change is worth reporting.
    fn record(&mut self, chunk: u64, total: Option<u64>) -> bool {
        self.downloaded = self.downloaded.saturating_add(chunk);
        let step = total.map_or(PROGRESS_STEP_BYTES, |total| (total / 100).max(1));
        let report = self.downloaded - self.reported >= step;
        if report {
            self.reported = self.downloaded;
        }
        report
    }
}

/// `None` means the researcher cancelled.
fn finish_download(bus: &EventBus, outcome: Option<Result<(Update, Vec<u8>), String>>) {
    let mut job = job();
    job.cancel = None;
    match outcome {
        None => job.state = IDLE,
        Some(Ok(package)) => {
            job.state.phase = InstallPhase::Ready;
            job.package = Some(package);
        }
        Some(Err(error)) => job.state = failed(job.state.version.clone(), error),
    }
    publish(bus, &job.state);
}

fn failed(version: Option<String>, error: String) -> InstallState {
    InstallState {
        phase: InstallPhase::Failed,
        version,
        error: Some(error),
        ..IDLE
    }
}

/// Stop a download, or set aside one that is waiting for a restart.
pub fn cancel(app: &AppHandle) -> InstallState {
    let mut job = job();
    match job.state.phase {
        // The download task returns to idle when it sees the signal.
        InstallPhase::Downloading => {
            if let Some(cancel) = &job.cancel {
                cancel.notify_one();
            }
        }
        InstallPhase::Ready | InstallPhase::Failed => {
            job.package = None;
            job.state = IDLE;
            publish(&crate::emit::from_app(app.clone()), &job.state);
        }
        InstallPhase::Idle | InstallPhase::Installing => {}
    }
    job.state.clone()
}

/// Replace the app with the downloaded update, then restart into it. Windows
/// never returns from a successful install: the updater ends this process.
pub async fn restart(app: AppHandle) -> Result<(), String> {
    let bus = crate::emit::from_app(app.clone());
    let (update, bytes) = {
        let mut job = job();
        let Some(package) = job.package.take() else {
            return Err("No update is ready to install.".to_string());
        };
        job.state.phase = InstallPhase::Installing;
        publish(&bus, &job.state);
        package
    };
    // An administrator password prompt can hold this for as long as it is open.
    let installed = tauri::async_runtime::spawn_blocking(move || update.install(bytes)).await;
    let message = match installed {
        Ok(Ok(())) => {
            app.request_restart();
            return Ok(());
        }
        Ok(Err(error)) => failure_message(&error),
        Err(_) => "The update could not be installed.".to_string(),
    };
    {
        let mut job = job();
        job.state = failed(job.state.version.clone(), message.clone());
        publish(&bus, &job.state);
    }
    // The updater stops the runtimes and hides the window before launching
    // the installer, so a launch that failed needs a fresh start.
    #[cfg(windows)]
    app.request_restart();
    Err(message)
}

fn failure_message(error: &Error) -> String {
    match error {
        Error::Reqwest(_) | Error::Network(_) | Error::ReleaseNotFound => {
            "Could not download the update. Check your connection and try again.".to_string()
        }
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::MissingSignedVersion
        | Error::SignedVersionMismatch { .. } => {
            "The update failed its signature check, so it was not installed.".to_string()
        }
        Error::PreviousAppNotRestored(path) => format!(
            "The update failed and Pipeline could not be put back in place. Your previous copy is at {}.",
            path.display()
        ),
        Error::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            "Pipeline doesn't have permission to replace itself.".to_string()
        }
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "This release has no update for this computer.".to_string()
        }
        error => format!("The update could not be installed: {error}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(manifest_url: Option<&str>) -> Release {
        Release {
            version: "1.2.0".into(),
            name: "Pipeline v1.2.0".into(),
            release_url: "https://github.com/mdroste/pipeline/releases/tag/v1.2.0".into(),
            download_url: None,
            manifest_url: manifest_url.map(str::to_string),
            published_at: None,
        }
    }

    #[test]
    fn cached_manifest_urls_are_revalidated() {
        let url = "https://github.com/mdroste/pipeline/releases/download/v1.2.0/latest.json";
        assert!(manifest_url(&release(Some(url))).is_some());
        assert!(manifest_url(&release(None)).is_none());
        assert!(manifest_url(&release(Some("https://example.com/latest.json"))).is_none());
        assert!(manifest_url(&release(Some(&url.replace("v1.2.0", "v1.1.0")))).is_none());
    }

    #[test]
    fn progress_reports_about_a_hundred_steps() {
        let total = Some(1_000_000);
        let mut progress = Progress::default();
        let reports = (0..10_000).filter(|_| progress.record(100, total)).count();
        assert_eq!(progress.downloaded, 1_000_000);
        assert_eq!(reports, 100);
    }

    #[test]
    fn progress_without_a_size_reports_each_megabyte() {
        let mut progress = Progress::default();
        assert!(!progress.record(PROGRESS_STEP_BYTES - 1, None));
        assert!(progress.record(1, None));
        assert!(!progress.record(1, None));
    }

    #[test]
    fn states_serialize_for_the_banner() {
        let value = serde_json::to_value(failed(Some("1.2.0".into()), "no".into())).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "phase": "failed",
                "version": "1.2.0",
                "downloaded": 0,
                "total": null,
                "error": "no",
            })
        );
    }

    #[test]
    fn failures_are_in_plain_language() {
        assert!(failure_message(&Error::Network("502".into())).contains("connection"));
        assert!(failure_message(&Error::MissingSignedVersion).contains("signature"));
        assert!(failure_message(&Error::SignedVersionMismatch {
            signed: "1.1.0".into(),
            announced: "1.2.0".into(),
        })
        .contains("signature"));
        assert!(failure_message(&Error::PreviousAppNotRestored(
            "/Applications/old.app".into()
        ))
        .contains("/Applications/old.app"));
        assert!(failure_message(&Error::Io(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )))
        .contains("permission"));
        assert!(
            failure_message(&Error::TargetNotFound("linux-riscv64".into()))
                .contains("this computer")
        );
    }
}
