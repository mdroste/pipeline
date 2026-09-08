//! Deterministic step identity and output directories.
use super::schedule::base_id;
use crate::pipeline::claude::normalize_cli_root;

// ── Artifact write handoff ──────────────────────────────────────────

/// Collision-resistant filesystem key for a step. Replacement-based slugs
/// made valid IDs such as `a.b` and `a_b` share one report file, so one step
/// could silently ingest another step's output.
pub(super) fn step_slug(step_key: &str) -> String {
    use sha2::{Digest as _, Sha256};
    let mut readable = String::new();
    let mut previous_dash = false;
    for character in step_key.chars() {
        let normalized = if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            character
        } else {
            '-'
        };
        if normalized == '-' {
            if previous_dash {
                continue;
            }
            previous_dash = true;
        } else {
            previous_dash = false;
        }
        readable.push(normalized);
        if readable.len() >= 40 {
            break;
        }
    }
    let readable = readable.trim_matches('-');
    let readable = if readable.is_empty() {
        "step"
    } else {
        readable
    };
    let digest = format!("{:x}", Sha256::digest(step_key.as_bytes()));
    format!("{readable}--{}", &digest[..12])
}

/// Allocate one producer-owned write root. Parallel agents and fan-out units
/// never share a writable namespace.
pub(super) fn step_write_dir(
    run_artifact_dir: Option<&str>,
    step_key: &str,
) -> Result<Option<String>, String> {
    let Some(root) = run_artifact_dir else {
        return Ok(None);
    };
    let base = base_id(step_key);
    let directory = std::path::Path::new(root)
        .join("by-step")
        .join(step_slug(base))
        .join(step_slug(step_key));
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!("Failed to create artifact directory for step '{step_key}': {error}")
    })?;
    Ok(Some(
        normalize_cli_root(&directory.to_string_lossy())
            .unwrap_or_else(|| directory.to_string_lossy().replace('\\', "/")),
    ))
}
