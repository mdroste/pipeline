//! Stable-channel version comparison for update checks.

pub(super) fn strip_v(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Compare the stable release candidate returned by GitHub with the running
/// build. Pipeline has one stable update channel: prerelease candidates are
/// never offered, while a stable release with the same core version replaces
/// a locally installed prerelease build.
pub(super) fn is_newer(candidate: &str, current: &str) -> bool {
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
