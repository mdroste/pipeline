//! Bounded fetch of the latest published GitHub release.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::version::strip_v;

const OWNER: &str = "mdroste";
const REPO: &str = "pipeline";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_RELEASE_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_CHARS: usize = 200;
const MAX_RETRY_AFTER_SECONDS: i64 = 24 * 60 * 60;

/// Installer published for this build's platform. The names must match
/// `installerNames()` in `scripts/release/draft-release.mjs`; a release test
/// compares them. Platforms without an official installer get `None`.
const INSTALLER_SUFFIX: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("-macOS-AppleSilicon.dmg")
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    Some("-macOS-Intel.dmg")
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    Some("-Windows.exe")
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("-Linux.AppImage")
} else {
    None
};

/// Signed package the in-app updater installs on this platform. Windows and
/// Linux install the installer itself; the names must match `updatePackages()`
/// in `scripts/release/update-manifest.mjs`, and a release test compares them.
const PACKAGE_SUFFIX: Option<&str> = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    Some("-macOS-AppleSilicon.app.tar.gz")
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    Some("-macOS-Intel.app.tar.gz")
} else {
    INSTALLER_SUFFIX
};
const MANIFEST_NAME: &str = "latest.json";

/// The latest stable release as last seen by a successful check. It is also
/// the cached form saved in `updates.json`, so its URLs are revalidated when
/// read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Release {
    pub version: String,
    pub name: String,
    pub release_url: String,
    #[serde(default)]
    pub download_url: Option<String>,
    /// Update manifest for in-app install. Releases before it have none.
    #[serde(default)]
    pub manifest_url: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CheckError {
    /// The request never produced a complete HTTP response.
    Network,
    /// GitHub refused the request; `until` comes from its rate-limit headers.
    RateLimited { until: Option<DateTime<Utc>> },
    /// GitHub answered with something Pipeline could not use.
    Unexpected(String),
}

impl CheckError {
    /// Researcher-facing message. `rate_limited_until` is the saved wait,
    /// which may be later than the headers of this response suggested.
    pub(super) fn message(
        &self,
        rate_limited_until: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> String {
        match self {
            Self::Network => {
                "Could not reach GitHub. Check your connection and try again.".to_string()
            }
            Self::RateLimited { until } => match rate_limited_until.or(*until) {
                Some(until) => rate_limited_message(until, now),
                None => "GitHub is limiting update checks from your network. Try again later."
                    .to_string(),
            },
            Self::Unexpected(detail) => {
                format!("Could not check for updates: {detail}. Try again later.")
            }
        }
    }
}

pub(super) fn rate_limited_message(until: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = ((until - now).num_seconds().max(0) + 59) / 60;
    let wait = match minutes.max(1) {
        1 => "about a minute".to_string(),
        minutes @ 2..=59 => format!("about {minutes} minutes"),
        minutes => match (minutes + 59) / 60 {
            1 => "about an hour".to_string(),
            hours => format!("about {hours} hours"),
        },
    };
    format!("GitHub is limiting update checks from your network. Try again in {wait}.")
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// `Ok(None)` means GitHub has no stable release to offer.
pub(super) async fn fetch_latest() -> Result<Option<Release>, CheckError> {
    let url = format!("https://api.github.com/repos/{OWNER}/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("pipeline-gui/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| CheckError::Unexpected("the network client could not start".into()))?;

    let mut resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| CheckError::Network)?;

    // Unauthenticated GitHub API calls are rate-limited per IP address, which
    // campus networks and VPNs share. GitHub asks clients not to retry before
    // the time its headers give.
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        let header = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok());
        return Err(CheckError::RateLimited {
            until: rate_limit_until(
                header("retry-after"),
                header("x-ratelimit-remaining"),
                header("x-ratelimit-reset"),
                Utc::now(),
            ),
        });
    }
    if !resp.status().is_success() {
        return Err(CheckError::Unexpected(format!(
            "GitHub returned {}",
            resp.status()
        )));
    }

    let too_large = || CheckError::Unexpected("GitHub's response was too large".into());
    if resp
        .content_length()
        .is_some_and(|length| length > MAX_RELEASE_RESPONSE_BYTES as u64)
    {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(
        resp.content_length()
            .unwrap_or(0)
            .min(MAX_RELEASE_RESPONSE_BYTES as u64) as usize,
    );
    while let Some(chunk) = resp.chunk().await.map_err(|_| CheckError::Network)? {
        if chunk.len() > MAX_RELEASE_RESPONSE_BYTES.saturating_sub(bytes.len()) {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    let release: GithubRelease = serde_json::from_slice(&bytes)
        .map_err(|_| CheckError::Unexpected("GitHub's response could not be read".into()))?;
    Ok(release_from(release))
}

fn release_from(release: GithubRelease) -> Option<Release> {
    // `/releases/latest` never returns drafts or prereleases; keep the guard
    // so a changed endpoint cannot offer an unpublished build.
    if release.draft || release.prerelease {
        return None;
    }
    let version = bounded(strip_v(&release.tag_name).to_string());
    let asset_url = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    let download_url = expected_installer(&version)
        .and_then(|expected| asset_url(&expected))
        .filter(|url| is_download_url(url, &version));
    let manifest_url = asset_url(MANIFEST_NAME).filter(|url| is_manifest_url(url, &version));
    Some(Release {
        name: bounded(release.name.unwrap_or_else(|| release.tag_name.clone())),
        release_url: release_page_or_default(&release.html_url),
        download_url,
        manifest_url,
        published_at: release.published_at.map(bounded),
        version,
    })
}

fn bounded(value: String) -> String {
    if value.chars().count() <= MAX_TEXT_CHARS {
        value
    } else {
        value.chars().take(MAX_TEXT_CHARS).collect()
    }
}

pub(super) fn rate_limit_until(
    retry_after: Option<&str>,
    remaining: Option<&str>,
    reset: Option<&str>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if let Some(seconds) = retry_after
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|seconds| *seconds >= 0)
    {
        return now.checked_add_signed(chrono::Duration::seconds(
            seconds.min(MAX_RETRY_AFTER_SECONDS),
        ));
    }
    if remaining.map(str::trim) == Some("0") {
        return reset
            .and_then(|value| value.trim().parse::<i64>().ok())
            .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single());
    }
    None
}

pub(super) fn releases_page() -> String {
    format!("https://github.com/{OWNER}/{REPO}/releases/latest")
}

/// Only this repository's release pages are opened in the browser.
pub(super) fn release_page_or_default(url: &str) -> String {
    let prefix = format!("/{OWNER}/{REPO}/releases/");
    match github_path(url) {
        Some(path) if path.len() > prefix.len() && path.starts_with(&prefix) => url.to_string(),
        _ => releases_page(),
    }
}

fn expected_installer(version: &str) -> Option<String> {
    INSTALLER_SUFFIX.map(|suffix| format!("Pipeline-{version}{suffix}"))
}

/// Accept only this platform's installer, attached to the `v<version>` tag of
/// this repository.
pub(super) fn is_download_url(url: &str, version: &str) -> bool {
    let Some(installer) = expected_installer(version) else {
        return false;
    };
    is_release_asset(url, version, &installer)
}

/// Accept only the manifest attached to the `v<version>` tag of this repository.
pub(super) fn is_manifest_url(url: &str, version: &str) -> bool {
    is_release_asset(url, version, MANIFEST_NAME)
}

/// Accept only this platform's update package, attached to the `v<version>`
/// tag of this repository.
pub(super) fn is_package_url(url: &str, version: &str) -> bool {
    PACKAGE_SUFFIX.is_some_and(|suffix| {
        is_release_asset(url, version, &format!("Pipeline-{version}{suffix}"))
    })
}

fn is_release_asset(url: &str, version: &str, name: &str) -> bool {
    github_path(url)
        .is_some_and(|path| path == format!("/{OWNER}/{REPO}/releases/download/v{version}/{name}"))
}

fn github_path(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| url.path().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()
    }

    fn github_release(value: serde_json::Value) -> GithubRelease {
        serde_json::from_value(value).unwrap()
    }

    fn installer_url(version: &str) -> Option<String> {
        expected_installer(version).map(|name| {
            format!("https://github.com/mdroste/pipeline/releases/download/v{version}/{name}")
        })
    }

    #[test]
    fn stable_release_carries_this_platforms_installer() {
        let mut assets = vec![serde_json::json!({
            "name": "Pipeline-1.2.0-Other.zip",
            "browser_download_url": "https://github.com/mdroste/pipeline/releases/download/v1.2.0/Pipeline-1.2.0-Other.zip",
        })];
        if let (Some(name), Some(url)) = (expected_installer("1.2.0"), installer_url("1.2.0")) {
            assets.push(serde_json::json!({ "name": name, "browser_download_url": url }));
        }
        let release = release_from(github_release(serde_json::json!({
            "tag_name": "v1.2.0",
            "name": "Pipeline v1.2.0",
            "html_url": "https://github.com/mdroste/pipeline/releases/tag/v1.2.0",
            "published_at": "2026-10-27T18:00:00Z",
            "assets": assets,
        })))
        .unwrap();
        assert_eq!(release.version, "1.2.0");
        assert_eq!(release.name, "Pipeline v1.2.0");
        assert_eq!(
            release.release_url,
            "https://github.com/mdroste/pipeline/releases/tag/v1.2.0"
        );
        assert_eq!(release.download_url, installer_url("1.2.0"));
        assert_eq!(
            release.published_at.as_deref(),
            Some("2026-10-27T18:00:00Z")
        );
    }

    #[test]
    fn manifest_is_read_from_the_release_assets() {
        let url = "https://github.com/mdroste/pipeline/releases/download/v1.2.0/latest.json";
        let manifest = |url: &str| {
            release_from(github_release(serde_json::json!({
                "tag_name": "v1.2.0",
                "html_url": "https://github.com/mdroste/pipeline/releases/tag/v1.2.0",
                "published_at": null,
                "assets": [{ "name": "latest.json", "browser_download_url": url }],
            })))
            .unwrap()
            .manifest_url
        };
        assert_eq!(manifest(url).as_deref(), Some(url));
        assert_eq!(manifest(&url.replace("v1.2.0", "v1.1.0")), None);
        assert_eq!(manifest(&url.replace("mdroste", "other")), None);
        assert!(!is_manifest_url(&format!("{url}?x=1"), "1.2.0"));
    }

    #[test]
    fn update_packages_must_belong_to_the_announced_version() {
        let Some(suffix) = PACKAGE_SUFFIX else {
            return;
        };
        let url = format!(
            "https://github.com/mdroste/pipeline/releases/download/v1.2.0/Pipeline-1.2.0{suffix}"
        );
        assert!(is_package_url(&url, "1.2.0"));
        assert!(!is_package_url(&url, "1.2.1"));
        assert!(!is_package_url(
            &url.replace("github.com", "github.com.example.net"),
            "1.2.0"
        ));
        assert!(!is_package_url(
            &url.replace("Pipeline-", "Other-"),
            "1.2.0"
        ));
    }

    #[test]
    fn missing_installer_falls_back_to_the_release_page() {
        let release = release_from(github_release(serde_json::json!({
            "tag_name": "v1.2.0",
            "name": null,
            "html_url": "https://github.com/mdroste/pipeline/releases/tag/v1.2.0",
            "published_at": null,
        })))
        .unwrap();
        assert_eq!(release.name, "v1.2.0");
        assert_eq!(release.download_url, None);
    }

    #[test]
    fn drafts_and_prereleases_are_never_offered() {
        for flag in ["draft", "prerelease"] {
            let mut value = serde_json::json!({
                "tag_name": "v9.0.0",
                "html_url": "https://github.com/mdroste/pipeline/releases/tag/v9.0.0",
                "published_at": null,
            });
            value[flag] = serde_json::Value::Bool(true);
            assert_eq!(release_from(github_release(value)), None);
        }
    }

    #[test]
    fn foreign_or_altered_urls_are_rejected() {
        assert_eq!(
            release_page_or_default("https://example.com/mdroste/pipeline/releases/tag/v1.2.0"),
            releases_page()
        );
        assert_eq!(
            release_page_or_default("http://github.com/mdroste/pipeline/releases/tag/v1.2.0"),
            releases_page()
        );
        assert_eq!(
            release_page_or_default("https://github.com/other/pipeline/releases/tag/v1.2.0"),
            releases_page()
        );
        assert_eq!(
            release_page_or_default("https://github.com/mdroste/pipeline/releases/"),
            releases_page()
        );
        let Some(url) = installer_url("1.2.0") else {
            return;
        };
        assert!(is_download_url(&url, "1.2.0"));
        assert!(!is_download_url(&url, "1.2.1"));
        assert!(!is_download_url(&format!("{url}?x=1"), "1.2.0"));
        assert!(!is_download_url(
            &url.replace("github.com", "github.com.example.net"),
            "1.2.0"
        ));
        assert!(!is_download_url(
            &url.replace("https://", "https://user@"),
            "1.2.0"
        ));
    }

    #[test]
    fn long_text_is_truncated() {
        let release = release_from(github_release(serde_json::json!({
            "tag_name": "v1.2.0",
            "name": "x".repeat(5000),
            "html_url": "https://github.com/mdroste/pipeline/releases/tag/v1.2.0",
            "published_at": null,
        })))
        .unwrap();
        assert_eq!(release.name.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn rate_limit_headers_set_the_wait() {
        let now = now();
        assert_eq!(
            rate_limit_until(Some("120"), None, None, now),
            Some(now + chrono::Duration::seconds(120))
        );
        assert_eq!(
            rate_limit_until(Some("999999"), None, None, now),
            Some(now + chrono::Duration::hours(24))
        );
        let reset = (now + chrono::Duration::minutes(40))
            .timestamp()
            .to_string();
        assert_eq!(
            rate_limit_until(None, Some("0"), Some(&reset), now),
            Some(now + chrono::Duration::minutes(40))
        );
        assert_eq!(rate_limit_until(None, Some("12"), Some(&reset), now), None);
        assert_eq!(rate_limit_until(Some("soon"), None, None, now), None);
        assert_eq!(rate_limit_until(None, None, None, now), None);
    }

    #[test]
    fn messages_are_plain_language() {
        let now = now();
        let wait = |minutes| rate_limited_message(now + chrono::Duration::minutes(minutes), now);
        assert!(wait(0).ends_with("Try again in about a minute."));
        assert!(wait(40).ends_with("Try again in about 40 minutes."));
        assert!(wait(60).ends_with("Try again in about an hour."));
        assert!(wait(61).ends_with("Try again in about 2 hours."));
        assert_eq!(
            CheckError::Network.message(None, now),
            "Could not reach GitHub. Check your connection and try again."
        );
        assert!(CheckError::RateLimited { until: None }
            .message(None, now)
            .ends_with("Try again later."));
        assert_eq!(
            CheckError::Unexpected("GitHub returned 502 Bad Gateway".into()).message(None, now),
            "Could not check for updates: GitHub returned 502 Bad Gateway. Try again later."
        );
    }
}
