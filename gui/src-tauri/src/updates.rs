//! Checks GitHub Releases for a newer version of the app.
//!
//! The check is lightweight: it hits the public releases API, compares the
//! tag (stripped of a leading `v`) to `CARGO_PKG_VERSION`, and returns what
//! it found. The frontend decides whether to surface a banner, and the user
//! downloads/installs updates manually from the release page.

use serde::Serialize;

const OWNER: &str = "mdroste";
const REPO: &str = "pipeline";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_RELEASE_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_name: String,
    pub published_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub async fn check() -> Result<UpdateInfo, String> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let url = format!("https://api.github.com/repos/{OWNER}/{REPO}/releases/latest");

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("pipeline-gui/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    // Unauthenticated GitHub API calls are rate-limited per IP; 403/429 here
    // means "try again later", not a real failure worth alarming anyone over.
    if resp.status() == reqwest::StatusCode::FORBIDDEN
        || resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err("GitHub API rate limit reached; update check skipped".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub returned {}", resp.status()));
    }

    if resp
        .content_length()
        .is_some_and(|length| length > MAX_RELEASE_RESPONSE_BYTES as u64)
    {
        return Err("GitHub update response exceeded the safety limit".to_string());
    }
    let mut resp = resp;
    let mut bytes = Vec::with_capacity(
        resp.content_length()
            .unwrap_or(0)
            .min(MAX_RELEASE_RESPONSE_BYTES as u64) as usize,
    );
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("response read failed: {e}"))?
    {
        if chunk.len() > MAX_RELEASE_RESPONSE_BYTES.saturating_sub(bytes.len()) {
            return Err("GitHub update response exceeded the safety limit".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }
    let release: GithubRelease =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse error: {e}"))?;

    // Skip drafts and prereleases — we only care about shipped stable builds.
    if release.draft || release.prerelease {
        return Ok(UpdateInfo {
            current: current.clone(),
            latest: current,
            update_available: false,
            release_url: release.html_url,
            release_name: release.name.unwrap_or_default(),
            published_at: release.published_at,
        });
    }

    let latest_tag = strip_v(&release.tag_name).to_string();
    let update_available = is_newer(&latest_tag, &current);

    Ok(UpdateInfo {
        current,
        latest: latest_tag,
        update_available,
        release_url: release.html_url,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        published_at: release.published_at,
    })
}

fn strip_v(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Compare the stable release candidate returned by GitHub with the running
/// build. Pipeline has one stable update channel: prerelease candidates are
/// never offered, while a stable release with the same core version replaces
/// a locally installed prerelease build.
fn is_newer(candidate: &str, current: &str) -> bool {
    let Some(candidate) = parse_version(candidate) else {
        return false;
    };
    let Some(current) = parse_version(current) else {
        return false;
    };
    if candidate.prerelease {
        return false;
    }
    match candidate.core.cmp(&current.core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => current.prerelease,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedVersion {
    core: [u64; 3],
    prerelease: bool,
}

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let value = strip_v(value.trim());
    let without_build = match value.split_once('+') {
        Some((_, "")) => return None,
        Some((core, _)) => core,
        None => value,
    };
    let (core, prerelease) = match without_build.split_once('-') {
        Some((_, "")) => return None,
        Some((core, _)) => (core, true),
        None => (without_build, false),
    };
    let components = core.split('.').collect::<Vec<_>>();
    if components.len() != 3
        || components.iter().any(|component| {
            component.is_empty()
                || !component
                    .chars()
                    .all(|character| character.is_ascii_digit())
                || component.len() > 1 && component.starts_with('0')
        })
    {
        return None;
    }
    let parts = components
        .into_iter()
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let core: [u64; 3] = parts.try_into().ok()?;
    Some(ParsedVersion { core, prerelease })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_v_handles_both_forms() {
        assert_eq!(strip_v("v1.2.3"), "1.2.3");
        assert_eq!(strip_v("1.2.3"), "1.2.3");
        assert_eq!(strip_v(""), "");
    }

    #[test]
    fn is_newer_detects_patch_minor_major_bumps() {
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("2.0.0", "1.99.99"));
    }

    #[test]
    fn is_newer_false_when_same_or_older() {
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        assert!(!is_newer("0.9.0", "1.0.0"));
    }

    #[test]
    fn is_newer_ignores_leading_v() {
        assert!(is_newer("v1.2.0", "1.1.0"));
        assert!(is_newer("1.2.0", "v1.1.0"));
    }

    #[test]
    fn is_newer_rejects_incomplete_or_malformed_versions() {
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(!is_newer("1.0.1", "1.0"));
        assert!(!is_newer("2", "1.99.99"));
        assert!(!is_newer("latest", "1.0.0"));
    }

    #[test]
    fn is_newer_never_offers_prereleases() {
        assert!(!is_newer("1.2.3-rc1", "1.2.3"));
        assert!(!is_newer("1.2.4-rc1", "1.2.3"));
    }

    #[test]
    fn stable_release_replaces_the_same_core_prerelease() {
        assert!(is_newer("1.2.3", "1.2.3-rc.1"));
        assert!(!is_newer("1.2.2", "1.2.3-rc.1"));
    }
}
