//! Checks GitHub Releases for a newer version of the app.
//!
//! Pipeline only notifies: researchers download and install a new version
//! themselves. Official builds check automatically at most once a day unless
//! the device preference is off; manual checks work in every build. Timing
//! and backoff live in `schedule`, the bounded request in `github`, and the
//! preference and cached result in `state`.

mod github;
mod schedule;
mod state;
mod version;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;

use crate::emit::EventBus;
use github::Release;

pub use schedule::start;

/// Emitted with an [`UpdateInfo`] when a check finds a newer stable release.
pub const UPDATE_AVAILABLE_EVENT: &str = "update-available";
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
            checked_at,
        };
    };
    UpdateInfo {
        current: CURRENT.to_string(),
        latest: release.version.clone(),
        update_available: version::is_newer(&release.version, CURRENT),
        // Cached URLs are revalidated: the file is outside the app bundle.
        release_url: github::release_page_or_default(&release.release_url),
        release_name: release.name.clone(),
        published_at: release.published_at.clone(),
        download_url: release
            .download_url
            .clone()
            .filter(|url| github::is_download_url(url, &release.version)),
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

pub fn set_automatic(enabled: bool) -> Result<UpdateStatus, String> {
    Ok(status_from(Ok(state::set_automatic(enabled)?)))
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

async fn run_automatic(bus: &EventBus) {
    let _flight = CHECK.lock().await;
    let now = Utc::now();
    let Ok(saved) = state::load() else {
        return;
    };
    if schedule::automatic_due(&saved, now) {
        let _ = fetch_and_record(bus, now).await;
    }
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
