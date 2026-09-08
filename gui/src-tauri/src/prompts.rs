use std::path::PathBuf;

// Default prompts compiled into the binary.
// Referee (Parallel) prompts:
const DEFAULT_CONTRIBUTION: &str = include_str!("../../../prompts/contribution.md");
const DEFAULT_TECHNICAL: &str = include_str!("../../../prompts/technical.md");
const DEFAULT_EMPIRICAL: &str = include_str!("../../../prompts/empirical.md");
const DEFAULT_CONSISTENCY: &str = include_str!("../../../prompts/consistency.md");
const DEFAULT_EXPOSITION: &str = include_str!("../../../prompts/exposition.md");
// Grant Proposal Review profile prompts:
const DEFAULT_GRANT_AIMS: &str = include_str!("../../../prompts/grant_aims.md");
const DEFAULT_GRANT_FEASIBILITY: &str = include_str!("../../../prompts/grant_feasibility.md");
const DEFAULT_GRANT_CLARITY: &str = include_str!("../../../prompts/grant_clarity.md");
const DEFAULT_GRANT_CONSISTENCY: &str = include_str!("../../../prompts/grant_consistency.md");
const DEFAULT_GRANT_SYNTHESIS: &str = include_str!("../../../prompts/grant_synthesis.md");
const DEFAULT_GRANT_VALIDATE: &str = include_str!("../../../prompts/grant_validate.md");
// Pipeline-level prompts (wrap, merge, consolidate, validate):
const DEFAULT_PARALLEL_CONTEXT: &str = include_str!("../../../prompts/parallel_context.md");
const DEFAULT_PARALLEL_CONTEXT_GENERIC: &str =
    include_str!("../../../prompts/parallel_context_generic.md");
const DEFAULT_MERGE: &str = include_str!("../../../prompts/merge.md");
const DEFAULT_EDITOR_SYNTHESIS: &str = include_str!("../../../prompts/editor_synthesis.md");
const DEFAULT_EDITOR_SYNTHESIS_ISSUES: &str =
    include_str!("../../../prompts/editor_synthesis_issues.md");
const DEFAULT_VALIDATE_FEEDBACK: &str = include_str!("../../../prompts/validate_feedback.md");
// Preprocessing prompts:
const DEFAULT_ORIENTATION: &str = include_str!("../../../prompts/orientation.md");
const DEFAULT_ORIENTATION_GENERIC: &str = include_str!("../../../prompts/orientation_generic.md");
const DEFAULT_ORIENTATION_GRANT: &str = include_str!("../../../prompts/orientation_grant.md");
const DEFAULT_ORIENTATION_FOLDER: &str = include_str!("../../../prompts/orientation_folder.md");

/// Get user prompt overrides in the active research data directory.
fn user_prompts_dir() -> Result<PathBuf, String> {
    Ok(crate::storage::data_root()?.join("prompts"))
}

/// Get the default content for a named prompt.
fn default_content(name: &str) -> Option<&'static str> {
    match name {
        "contribution" => Some(DEFAULT_CONTRIBUTION),
        "technical" => Some(DEFAULT_TECHNICAL),
        "empirical" => Some(DEFAULT_EMPIRICAL),
        "consistency" => Some(DEFAULT_CONSISTENCY),
        "exposition" => Some(DEFAULT_EXPOSITION),
        "grant_aims" => Some(DEFAULT_GRANT_AIMS),
        "grant_feasibility" => Some(DEFAULT_GRANT_FEASIBILITY),
        "grant_clarity" => Some(DEFAULT_GRANT_CLARITY),
        "grant_consistency" => Some(DEFAULT_GRANT_CONSISTENCY),
        "grant_synthesis" => Some(DEFAULT_GRANT_SYNTHESIS),
        "grant_validate" => Some(DEFAULT_GRANT_VALIDATE),
        "parallel_context" => Some(DEFAULT_PARALLEL_CONTEXT),
        "parallel_context_generic" => Some(DEFAULT_PARALLEL_CONTEXT_GENERIC),
        "merge" => Some(DEFAULT_MERGE),
        "editor_synthesis" => Some(DEFAULT_EDITOR_SYNTHESIS),
        "editor_synthesis_issues" => Some(DEFAULT_EDITOR_SYNTHESIS_ISSUES),
        "validate_feedback" => Some(DEFAULT_VALIDATE_FEEDBACK),
        "orientation" => Some(DEFAULT_ORIENTATION),
        "orientation_generic" => Some(DEFAULT_ORIENTATION_GENERIC),
        "orientation_grant" => Some(DEFAULT_ORIENTATION_GRANT),
        "orientation_folder" => Some(DEFAULT_ORIENTATION_FOLDER),
        _ => None,
    }
}

/// Compiled-in default for a named prompt, ignoring user overrides.
/// Used by "reset to default" actions in the editor, where the user's
/// intent is the shipped template, not their override.
pub fn compiled_default(name: &str) -> Option<&'static str> {
    default_content(name)
}

/// Load a prompt by name. Checks ~/.pipeline/prompts/ first, falls back to compiled default.
pub fn load_prompt(filename: &str) -> Result<String, String> {
    let base = filename.trim_end_matches(".md");

    // Reject filenames with path separators or parent directory references
    if base.contains('/') || base.contains('\\') || base.contains("..") {
        return Err(format!("Invalid prompt name: {filename}"));
    }

    // Check user override
    {
        let dir = user_prompts_dir()?;
        let user_path = dir.join(format!("{base}.md"));
        if user_path.is_file() {
            // Verify resolved path stays within the prompts directory
            if let (Ok(canonical), Ok(canonical_dir)) =
                (user_path.canonicalize(), dir.canonicalize())
            {
                if !canonical.starts_with(&canonical_dir) {
                    return Err(format!("Prompt path escapes prompts directory: {filename}"));
                }
            }
            use std::io::Read as _;
            let file = crate::safety::open_regular_file(&user_path)?;
            let mut bytes = Vec::with_capacity(64 * 1024);
            file.take(crate::pipeline_config::MAX_STEP_PROMPT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("Failed to read {}: {e}", user_path.display()))?;
            if bytes.len() > crate::pipeline_config::MAX_STEP_PROMPT_BYTES {
                return Err(format!(
                    "Prompt override {} is too large",
                    user_path.display()
                ));
            }
            return String::from_utf8(bytes)
                .map_err(|e| format!("Prompt override {} is not UTF-8: {e}", user_path.display()));
        }
    }

    // Fall back to compiled default
    default_content(base)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unknown prompt: {filename}"))
}
