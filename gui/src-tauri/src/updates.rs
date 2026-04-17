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

    if !resp.status().is_success() {
        return Err(format!("GitHub returned {}", resp.status()));
    }

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| format!("parse error: {e}"))?;

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

/// Compares two dot-separated numeric version strings. Returns true iff
/// `candidate` is strictly newer than `current`. Non-numeric suffixes on
/// components (e.g. "1.2.3-rc1") are tolerated: the leading digits are used.
fn is_newer(candidate: &str, current: &str) -> bool {
    let a = parse_parts(candidate);
    let b = parse_parts(current);
    let len = a.len().max(b.len());
    for i in 0..len {
        let ai = a.get(i).copied().unwrap_or(0);
        let bi = b.get(i).copied().unwrap_or(0);
        if ai != bi {
            return ai > bi;
        }
    }
    false
}

fn parse_parts(v: &str) -> Vec<u32> {
    strip_v(v)
        .split('.')
        .map(|p| {
            let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().unwrap_or(0)
        })
        .collect()
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
    fn is_newer_pads_missing_components_with_zero() {
        assert!(!is_newer("1.0", "1.0.0"));
        assert!(is_newer("1.0.1", "1.0"));
        assert!(is_newer("2", "1.99.99"));
    }

    #[test]
    fn is_newer_tolerates_nonnumeric_suffix() {
        // "1.2.3-rc1" compared to "1.2.3" — same numeric parts, not newer.
        assert!(!is_newer("1.2.3-rc1", "1.2.3"));
        // "1.2.4-rc1" vs "1.2.3" — newer by patch.
        assert!(is_newer("1.2.4-rc1", "1.2.3"));
    }
}
