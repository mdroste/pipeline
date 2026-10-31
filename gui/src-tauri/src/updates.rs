//! Checks GitHub Releases for a newer version of the app and installs it.
//!
//! Official builds check each time they start, and then once a day, unless the
//! device preference is off; manual checks work in every build. Installing is
//! always the researcher's choice. `install` downloads a signed package and
//! restarts only on request; a build or release it cannot update falls back to
//! the installer in the browser. Timing and backoff live in `schedule`, the
//! bounded request in `github`, and the preference and cached result in `state`.

mod github;
mod install;
mod schedule;
mod state;
mod version;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;

use crate::emit::EventBus;
use github::Release;

pub use install::{
    cancel as install_cancel, restart as install_restart, start as install_start,
    state as install_state, InstallState,
};
pub use schedule::start;

/// Emitted with an [`UpdateInfo`] when a check finds a newer stable release.
pub const UPDATE_AVAILABLE_EVENT: &str = "update-available";
/// Emitted with an [`UpdateStatus`] when the saved preference changes.
const UPDATE_STATUS_EVENT: &str = "update-status";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// A manual check right after another check reuses its result.
const RECENT_CHECK_SECONDS: i64 = 30;

/// Keeps scheduled and manual checks from sending overlapping requests.
static CHECK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_name: String,
    pub published_at: Option<String>,
    /// This platform's installer on the release, when one is attached.
    pub download_url: Option<String>,
    /// Whether this copy can install the release itself.
    pub can_install: bool,
    pub checked_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub automatic: bool,
    /// False for development and source builds, which check only on request.
    pub automatic_available: bool,
    pub last_checked_at: Option<String>,
    /// The last successful check, compared with the running version.
    pub info: Option<UpdateInfo>,
    /// Present when the saved preference could not be read.
    pub error: Option<String>,
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn info(latest: Option<&Release>, checked_at: Option<DateTime<Utc>>) -> UpdateInfo {
    let checked_at = checked_at.map(timestamp);
    let Some(release) = latest else {
        return UpdateInfo {
            current: CURRENT.to_string(),
            latest: CURRENT.to_string(),
            update_available: false,
            release_url: github::releases_page(),
            release_name: String::new(),
            published_at: None,
            download_url: None,
            can_install: false,
            checked_at,
        };
    };
    let update_available = version::is_newer(&release.version, CURRENT);
    UpdateInfo {
        current: CURRENT.to_string(),
        latest: release.version.clone(),
        update_available,
        // Cached URLs are revalidated: the file is outside the app bundle.
        release_url: github::release_page_or_default(&release.release_url),
        release_name: release.name.clone(),
        published_at: release.published_at.clone(),
        download_url: release
            .download_url
            .clone()
            .filter(|url| github::is_download_url(url, &release.version)),
        can_install: update_available
            && schedule::automatic_supported()
            && install::manifest_url(release).is_some(),
        checked_at,
    }
}

fn status_from(saved: Result<state::State, String>) -> UpdateStatus {
    let automatic_available = schedule::automatic_supported();
    match saved {
        Ok(saved) => UpdateStatus {
            automatic: saved.automatic,
            automatic_available,
            last_checked_at: saved.last_checked_at.map(timestamp),
            info: saved
                .last_checked_at
                .map(|at| info(saved.latest.as_ref(), Some(at))),
            error: None,
        },
        Err(error) => UpdateStatus {
            automatic: false,
            automatic_available,
            last_checked_at: None,
            info: None,
            error: Some(error),
        },
    }
}

/// Saved preference and last result. Sends no request.
pub fn status() -> UpdateStatus {
    status_from(state::load())
}

/// Saves the preference and tells every open control for it.
pub fn set_automatic(bus: &EventBus, enabled: bool) -> Result<UpdateStatus, String> {
    let status = status_from(Ok(state::set_automatic(enabled)?));
    if let Ok(payload) = serde_json::to_value(&status) {
        let _ = bus.emit_event(UPDATE_STATUS_EVENT, payload);
    }
    Ok(status)
}

/// Check now because the researcher asked.
pub async fn check_now(bus: &EventBus) -> Result<UpdateInfo, String> {
    let _flight = CHECK.lock().await;
    let now = Utc::now();
    // An unreadable state file must not block a check the researcher asked for.
    if let Ok(saved) = state::load() {
        if let Some(until) = saved.rate_limited_until.filter(|until| *until > now) {
            return Err(github::rate_limited_message(until, now));
        }
        if saved
            .last_checked_at
            .is_some_and(|at| at <= now && now - at < Duration::seconds(RECENT_CHECK_SECONDS))
        {
            return Ok(info(saved.latest.as_ref(), saved.last_checked_at));
        }
    }
    fetch_and_record(bus, now).await
}

/// Returns whether this launch still owes its startup check.
async fn run_automatic(bus: &EventBus, startup_check_owed: bool) -> bool {
    let _flight = CHECK.lock().await;
    let now = Utc::now();
    let Ok(saved) = state::load() else {
        return startup_check_owed;
    };
    let due = if startup_check_owed {
        schedule::automatic_allowed(&saved, now)
    } else {
        schedule::automatic_due(&saved, now)
    };
    if !due {
        return startup_check_owed;
    }
    fetch_and_record(bus, now).await.is_err() && startup_check_owed
}

async fn fetch_and_record(bus: &EventBus, now: DateTime<Utc>) -> Result<UpdateInfo, String> {
    let outcome = github::fetch_latest().await;
    let saved = state::update(|state| schedule::apply_outcome(state, &outcome, now));
    match outcome {
        Ok(latest) => {
            let found = info(latest.as_ref(), Some(now));
            if found.update_available {
                if let Ok(payload) = serde_json::to_value(&found) {
                    let _ = bus.emit_event(UPDATE_AVAILABLE_EVENT, payload);
                }
            }
            Ok(found)
        }
        Err(error) => {
            let until = saved.ok().and_then(|saved| saved.rate_limited_until);
            Err(error.message(until, now))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            name: format!("Pipeline v{version}"),
            release_url: format!("https://github.com/mdroste/pipeline/releases/tag/v{version}"),
            download_url: Some("https://example.com/Pipeline.dmg".into()),
            manifest_url: Some(format!(
                "https://github.com/mdroste/pipeline/releases/download/v{version}/latest.json"
            )),
            published_at: None,
        }
    }

    #[test]
    fn cached_release_is_compared_with_the_running_version() {
        assert!(info(Some(&release("999.0.0")), None).update_available);
        let same = info(Some(&release(CURRENT)), None);
        assert!(!same.update_available);
        assert_eq!(same.latest, CURRENT);
    }

    #[test]
    fn cached_urls_are_revalidated() {
        let mut cached = release("999.0.0");
        cached.release_url = "https://example.com/phish".into();
        let found = info(Some(&cached), None);
        assert_eq!(found.release_url, github::releases_page());
        assert_eq!(found.download_url, None);
    }

    #[test]
    fn source_builds_never_install_in_place() {
        assert!(!info(Some(&release("999.0.0")), None).can_install);
    }

    #[test]
    fn no_stable_release_reports_up_to_date() {
        let found = info(None, None);
        assert!(!found.update_available);
        assert_eq!(found.latest, CURRENT);
        assert_eq!(found.release_url, github::releases_page());
    }

    #[test]
    fn unreadable_state_reports_automatic_checks_off() {
        let status = status_from(Err("damaged".into()));
        assert!(!status.automatic);
        assert_eq!(status.error.as_deref(), Some("damaged"));
        assert!(status.info.is_none());
    }

    #[test]
    fn status_omits_info_before_the_first_check() {
        let status = status_from(Ok(state::State::default()));
        assert!(status.automatic);
        assert!(status.info.is_none());
        assert!(status.last_checked_at.is_none());
    }
}
