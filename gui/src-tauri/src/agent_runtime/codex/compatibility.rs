use serde::Serialize;

/// Oldest App Server contract implemented by Workspace. Newer builds are
/// admitted through the live handshake and fail-closed response validation,
/// rather than an exact-version allowlist that breaks on routine CLI updates.
pub const MINIMUM_CODEX_VERSION: &str = "0.147.0";
/// Version that produced the checked-in schema reference. This is evidence for
/// the implemented protocol generation, not an exact runtime allowlist.
pub const SCHEMA_REFERENCE_CODEX_VERSION: &str = "0.147.0";
pub const STABLE_SCHEMA_SHA256: &str =
    "f72b2caa3cbfa4298de9e85c62dda6dfbaf2266ffeb916fed30615ca69ff8c74";
pub const EXPERIMENTAL_SCHEMA_SHA256: &str =
    "babfd5c98cd978dd858b4762cdfbc9fba941e1a0e4053de0050e4082ae1f075a";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetectedCodexVersion {
    pub reported: String,
    pub version: String,
    pub meets_minimum: bool,
    pub matches_reference_schema: bool,
}

fn numeric_version(value: &str) -> Result<[u64; 3], String> {
    let parts = value
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.chars().all(|value| value.is_ascii_digit()) {
                return Err(format!("Invalid Codex CLI version: {value:?}"));
            }
            part.parse::<u64>()
                .map_err(|_| format!("Invalid Codex CLI version: {value:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    parts
        .try_into()
        .map_err(|_| format!("Invalid Codex CLI version: {value:?}"))
}

/// Parse the documented `codex --version` response. Version establishes only
/// the minimum protocol generation; the launched server must still satisfy
/// the live handshake and every operation's response invariants.
pub fn detected_version(reported: &str) -> Result<DetectedCodexVersion, String> {
    let reported = reported.trim();
    let mut fields = reported.split_whitespace();
    let product = fields.next().unwrap_or_default();
    let version = fields.next().unwrap_or_default();
    if product != "codex-cli" || version.is_empty() || fields.next().is_some() {
        return Err(format!(
            "Unsupported Codex version response: {reported:?}; expected `codex-cli <version>`"
        ));
    }
    let parsed = numeric_version(version)?;
    let minimum = numeric_version(MINIMUM_CODEX_VERSION)
        .expect("minimum Codex version constant must be numeric semver");
    Ok(DetectedCodexVersion {
        reported: reported.to_string(),
        version: version.to_string(),
        meets_minimum: parsed >= minimum,
        matches_reference_schema: version == SCHEMA_REFERENCE_CODEX_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest as _, Sha256};

    fn sha256(bytes: &[u8]) -> String {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    }

    #[test]
    fn accepts_the_minimum_and_newer_builds() {
        let qualified = detected_version("codex-cli 0.147.0\n").unwrap();
        assert!(qualified.meets_minimum);
        assert!(qualified.matches_reference_schema);
        assert_eq!(qualified.version, SCHEMA_REFERENCE_CODEX_VERSION);

        let newer = detected_version("codex-cli 0.153.4").unwrap();
        assert!(newer.meets_minimum);
        assert!(!newer.matches_reference_schema);

        let older = detected_version("codex-cli 0.146.9").unwrap();
        assert!(!older.meets_minimum);
        assert!(!older.matches_reference_schema);
    }

    #[test]
    fn rejects_ambiguous_version_output() {
        assert!(detected_version("0.147.0").is_err());
        assert!(detected_version("codex-cli 0.147").is_err());
        assert!(detected_version("codex-cli 0.147.0 extra").is_err());
        assert!(detected_version("codex-cli 0.18446744073709551616.0").is_err());
    }

    #[test]
    fn checked_in_schema_bundles_match_the_compatibility_constants() {
        let stable =
            include_bytes!("../../../../../docs/workbench/protocol/0.147.0/stable.schemas.json");
        let experimental = include_bytes!(
            "../../../../../docs/workbench/protocol/0.147.0/experimental.schemas.json"
        );
        assert_eq!(sha256(stable), STABLE_SCHEMA_SHA256);
        assert_eq!(sha256(experimental), EXPERIMENTAL_SCHEMA_SHA256);
    }
}
