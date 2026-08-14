use super::*;

pub(super) fn profiles_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("profiles");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create profiles dir: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)) {
            eprintln!(
                "WARNING: could not tighten permissions on {}: {e}",
                dir.display()
            );
        }
    }
    Ok(dir)
}

pub(super) fn profile_path(id: &str) -> Result<PathBuf, String> {
    validate_profile_id(id)?;
    Ok(profiles_dir()?.join(format!("{id}.json")))
}

pub(super) fn validate_profile_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Profile ID cannot be empty".into());
    }
    if id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Profile ID must use 1–64 ASCII letters, numbers, hyphens, or underscores".into(),
        );
    }
    Ok(())
}

pub fn slugify(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Sanitize a step ID by replacing characters reserved for internal use.
/// `/` is the step_id/agent delimiter in multi-agent composite keys;
/// allowing it in step IDs would corrupt merge grouping and multi-agent detection.
pub fn sanitize_step_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    // Collapse consecutive dashes and trim leading/trailing
    sanitized
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ── Defaults ────────────────────────────────────────────────────────
//
// The four pipeline-level prompts (parallel_context, merge, editor_synthesis,
// validate_feedback) live in prompts/*.md and are loaded through prompts::load_prompt,
// which lets users override them at ~/.pipeline/prompts/<name>.md.
