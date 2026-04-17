use std::fs;
use std::path::PathBuf;

// Default prompts compiled into the binary.
// Referee (Parallel) prompts:
const DEFAULT_CONTRIBUTION: &str = include_str!("../../../prompts/contribution.md");
const DEFAULT_TECHNICAL: &str = include_str!("../../../prompts/technical.md");
const DEFAULT_EMPIRICAL: &str = include_str!("../../../prompts/empirical.md");
const DEFAULT_CONSISTENCY: &str = include_str!("../../../prompts/consistency.md");
const DEFAULT_EXPOSITION: &str = include_str!("../../../prompts/exposition.md");
// Pipeline-level prompts (wrap, merge, consolidate, validate):
const DEFAULT_PARALLEL_CONTEXT: &str = include_str!("../../../prompts/parallel_context.md");
const DEFAULT_MERGE: &str = include_str!("../../../prompts/merge.md");
const DEFAULT_EDITOR_SYNTHESIS: &str = include_str!("../../../prompts/editor_synthesis.md");
const DEFAULT_VALIDATE_FEEDBACK: &str = include_str!("../../../prompts/validate_feedback.md");

/// Get the user prompts directory (~/.pipeline/prompts/).
fn user_prompts_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".pipeline").join("prompts"))
}

/// Get the default content for a named prompt.
fn default_content(name: &str) -> Option<&'static str> {
    match name {
        "contribution" => Some(DEFAULT_CONTRIBUTION),
        "technical" => Some(DEFAULT_TECHNICAL),
        "empirical" => Some(DEFAULT_EMPIRICAL),
        "consistency" => Some(DEFAULT_CONSISTENCY),
        "exposition" => Some(DEFAULT_EXPOSITION),
        "parallel_context" => Some(DEFAULT_PARALLEL_CONTEXT),
        "merge" => Some(DEFAULT_MERGE),
        "editor_synthesis" => Some(DEFAULT_EDITOR_SYNTHESIS),
        "validate_feedback" => Some(DEFAULT_VALIDATE_FEEDBACK),
        _ => None,
    }
}

/// Load a prompt by name. Checks ~/.pipeline/prompts/ first, falls back to compiled default.
pub fn load_prompt(filename: &str) -> Result<String, String> {
    let base = filename
        .trim_end_matches(".md");

    // Reject filenames with path separators or parent directory references
    if base.contains('/') || base.contains('\\') || base.contains("..") {
        return Err(format!("Invalid prompt name: {filename}"));
    }

    // Check user override
    if let Some(dir) = user_prompts_dir() {
        let user_path = dir.join(format!("{base}.md"));
        if user_path.is_file() {
            // Verify resolved path stays within the prompts directory
            if let (Ok(canonical), Ok(canonical_dir)) = (user_path.canonicalize(), dir.canonicalize()) {
                if !canonical.starts_with(&canonical_dir) {
                    return Err(format!("Prompt path escapes prompts directory: {filename}"));
                }
            }
            return fs::read_to_string(&user_path)
                .map_err(|e| format!("Failed to read {}: {e}", user_path.display()));
        }
    }

    // Fall back to compiled default
    default_content(base)
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Unknown prompt: {filename}"))
}
