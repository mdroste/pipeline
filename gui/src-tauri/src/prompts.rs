use std::fs;
use std::path::PathBuf;

// Default prompts compiled into the binary.
// Referee (Parallel) prompts:
const DEFAULT_CONTRIBUTION: &str = include_str!("../../../prompts/contribution.md");
const DEFAULT_TECHNICAL: &str = include_str!("../../../prompts/technical.md");
const DEFAULT_EMPIRICAL: &str = include_str!("../../../prompts/empirical.md");
const DEFAULT_CONSISTENCY: &str = include_str!("../../../prompts/consistency.md");
const DEFAULT_EXPOSITION: &str = include_str!("../../../prompts/exposition.md");
// Code review profile prompts (Quick + Deep):
const DEFAULT_CODE_CORRECTNESS: &str = include_str!("../../../prompts/code_correctness.md");
const DEFAULT_CODE_DESIGN: &str = include_str!("../../../prompts/code_design.md");
const DEFAULT_CODE_SECURITY: &str = include_str!("../../../prompts/code_security.md");
const DEFAULT_CODE_SYNTHESIS: &str = include_str!("../../../prompts/code_synthesis.md");
const DEFAULT_CODE_CONCURRENCY: &str = include_str!("../../../prompts/code_concurrency.md");
const DEFAULT_CODE_ERRORS: &str = include_str!("../../../prompts/code_errors.md");
const DEFAULT_CODE_PERFORMANCE: &str = include_str!("../../../prompts/code_performance.md");
const DEFAULT_CODE_TESTS: &str = include_str!("../../../prompts/code_tests.md");
const DEFAULT_CODE_VERIFY: &str = include_str!("../../../prompts/code_verify.md");
// Replication Package Audit profile prompts:
const DEFAULT_REPL_COMPLETENESS: &str = include_str!("../../../prompts/repl_completeness.md");
const DEFAULT_REPL_CONSISTENCY: &str = include_str!("../../../prompts/repl_consistency.md");
const DEFAULT_REPL_PORTABILITY: &str = include_str!("../../../prompts/repl_portability.md");
const DEFAULT_REPL_PROVENANCE: &str = include_str!("../../../prompts/repl_provenance.md");
const DEFAULT_REPL_SYNTHESIS: &str = include_str!("../../../prompts/repl_synthesis.md");
// Grant Proposal Review profile prompts:
const DEFAULT_GRANT_AIMS: &str = include_str!("../../../prompts/grant_aims.md");
const DEFAULT_GRANT_FEASIBILITY: &str = include_str!("../../../prompts/grant_feasibility.md");
const DEFAULT_GRANT_CLARITY: &str = include_str!("../../../prompts/grant_clarity.md");
const DEFAULT_GRANT_CONSISTENCY: &str = include_str!("../../../prompts/grant_consistency.md");
const DEFAULT_GRANT_SYNTHESIS: &str = include_str!("../../../prompts/grant_synthesis.md");
// Pipeline-level prompts (wrap, merge, consolidate, validate):
const DEFAULT_PARALLEL_CONTEXT: &str = include_str!("../../../prompts/parallel_context.md");
const DEFAULT_PARALLEL_CONTEXT_GENERIC: &str =
    include_str!("../../../prompts/parallel_context_generic.md");
const DEFAULT_MERGE: &str = include_str!("../../../prompts/merge.md");
const DEFAULT_EDITOR_SYNTHESIS: &str = include_str!("../../../prompts/editor_synthesis.md");
const DEFAULT_VALIDATE_FEEDBACK: &str = include_str!("../../../prompts/validate_feedback.md");
// Preprocessing prompts:
const DEFAULT_ORIENTATION: &str = include_str!("../../../prompts/orientation.md");
const DEFAULT_ORIENTATION_GENERIC: &str = include_str!("../../../prompts/orientation_generic.md");
const DEFAULT_ORIENTATION_FOLDER: &str = include_str!("../../../prompts/orientation_folder.md");

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
        "code_correctness" => Some(DEFAULT_CODE_CORRECTNESS),
        "code_design" => Some(DEFAULT_CODE_DESIGN),
        "code_security" => Some(DEFAULT_CODE_SECURITY),
        "code_synthesis" => Some(DEFAULT_CODE_SYNTHESIS),
        "code_concurrency" => Some(DEFAULT_CODE_CONCURRENCY),
        "code_errors" => Some(DEFAULT_CODE_ERRORS),
        "code_performance" => Some(DEFAULT_CODE_PERFORMANCE),
        "code_tests" => Some(DEFAULT_CODE_TESTS),
        "code_verify" => Some(DEFAULT_CODE_VERIFY),
        "repl_completeness" => Some(DEFAULT_REPL_COMPLETENESS),
        "repl_consistency" => Some(DEFAULT_REPL_CONSISTENCY),
        "repl_portability" => Some(DEFAULT_REPL_PORTABILITY),
        "repl_provenance" => Some(DEFAULT_REPL_PROVENANCE),
        "repl_synthesis" => Some(DEFAULT_REPL_SYNTHESIS),
        "grant_aims" => Some(DEFAULT_GRANT_AIMS),
        "grant_feasibility" => Some(DEFAULT_GRANT_FEASIBILITY),
        "grant_clarity" => Some(DEFAULT_GRANT_CLARITY),
        "grant_consistency" => Some(DEFAULT_GRANT_CONSISTENCY),
        "grant_synthesis" => Some(DEFAULT_GRANT_SYNTHESIS),
        "parallel_context" => Some(DEFAULT_PARALLEL_CONTEXT),
        "parallel_context_generic" => Some(DEFAULT_PARALLEL_CONTEXT_GENERIC),
        "merge" => Some(DEFAULT_MERGE),
        "editor_synthesis" => Some(DEFAULT_EDITOR_SYNTHESIS),
        "validate_feedback" => Some(DEFAULT_VALIDATE_FEEDBACK),
        "orientation" => Some(DEFAULT_ORIENTATION),
        "orientation_generic" => Some(DEFAULT_ORIENTATION_GENERIC),
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
