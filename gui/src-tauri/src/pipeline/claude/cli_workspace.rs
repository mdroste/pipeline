//! CLI prompt and workspace planning.

use super::*;

/// System instructions are read by the CLI itself, not by a model tool. Keep
/// even short ones in a private file so their size never consumes argv space.
pub(crate) struct PreparedCliSystemPrompt {
    pub path: String,
    _directory: TempDir,
}

pub(crate) fn prepare_cli_system_prompt(prompt: &str) -> Result<PreparedCliSystemPrompt, String> {
    let directory = tempfile::Builder::new()
        .prefix("pipeline_system_prompt_")
        .tempdir()
        .map_err(|e| format!("Failed to create system-instruction folder: {e}"))?;
    let path = directory.path().join("system.txt");
    std::fs::write(&path, prompt)
        .map_err(|e| format!("Failed to write system instructions: {e}"))?;
    let path = normalize_cli_root(&path.to_string_lossy())
        .ok_or("Failed to resolve system-instruction file")?;
    Ok(PreparedCliSystemPrompt {
        path,
        _directory: directory,
    })
}

/// Count the UTF-16 command line including Rust's Windows argument escaping.
/// This also catches oversized schemas and tool rules before CreateProcessW
/// reports a misleading executable-not-found error.
pub(crate) fn validate_cli_command_length(
    command: &std::process::Command,
    windows: bool,
) -> Result<(), String> {
    if !windows {
        return Ok(());
    }
    let mut length = command
        .get_program()
        .to_string_lossy()
        .encode_utf16()
        .count()
        + 3;
    for arg in command.get_args() {
        let text = arg.to_string_lossy();
        let quoted = text.is_empty() || text.contains([' ', '\t']);
        length += 1 + if quoted { 2 } else { 0 };
        let mut slashes = 0;
        for unit in text.encode_utf16() {
            if unit == b'\\' as u16 {
                slashes += 1;
            } else {
                length += if unit == b'"' as u16 {
                    slashes * 2 + 2
                } else {
                    slashes + 1
                };
                slashes = 0;
            }
        }
        length += slashes * if quoted { 2 } else { 1 };
    }
    if length > 32_767 {
        return Err("This review's command exceeds the Windows launch limit. Shorten its output schema or tool rules and try again; system instructions are already passed in a file.".into());
    }
    Ok(())
}

/// A CLI-safe prompt argument. Long prompts live in a private temporary
/// directory rather than directly under the process-wide temp directory, so
/// providers can grant access to only this call's prompt file.
pub(crate) struct PreparedCliPrompt {
    pub argument: String,
    pub path: Option<String>,
    pub read_root: Option<String>,
    _temp_dir: Option<TempDir>,
}

pub(crate) fn prepare_cli_prompt(prompt: &str) -> Result<PreparedCliPrompt, String> {
    if prompt.len() <= MAX_DIRECT_PROMPT_LENGTH {
        return Ok(PreparedCliPrompt {
            argument: prompt.to_string(),
            path: None,
            read_root: None,
            _temp_dir: None,
        });
    }

    let temp_dir = tempfile::Builder::new()
        .prefix("pipeline_prompt_")
        .tempdir()
        .map_err(|e| format!("Failed to create prompt temp directory: {e}"))?;
    let prompt_path = temp_dir.path().join("prompt.txt");
    let mut file = std::fs::File::create(&prompt_path)
        .map_err(|e| format!("Failed to create prompt temp file: {e}"))?;
    file.write_all(prompt.as_bytes())
        .map_err(|e| format!("Failed to write prompt temp file: {e}"))?;
    file.flush()
        .map_err(|e| format!("Failed to flush prompt temp file: {e}"))?;

    let path = normalize_cli_root(&prompt_path.to_string_lossy())
        .ok_or_else(|| "Failed to resolve prompt temp file".to_string())?;
    let read_root = normalize_cli_root(&temp_dir.path().to_string_lossy())
        .ok_or_else(|| "Failed to resolve prompt temp directory".to_string())?;
    Ok(PreparedCliPrompt {
        argument: format!("Read the instructions at {path} and follow them exactly."),
        path: Some(path),
        read_root: Some(read_root),
        _temp_dir: Some(temp_dir),
    })
}

/// Normalize a path for CLI arguments. Existing paths are canonicalized
/// (which resolves macOS `/var` aliases and Windows extended-path prefixes);
/// absolute paths with another platform's syntax are normalized lexically so
/// request construction can be tested deterministically on every host.
pub(crate) fn normalize_cli_root(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let slash_path = trimmed.replace('\\', "/");
    for candidate in [
        std::path::Path::new(trimmed),
        std::path::Path::new(&slash_path),
    ] {
        if let Ok(canonical) = candidate.canonicalize() {
            return Some(normalize_canonical_cli_path(&canonical.to_string_lossy()));
        }
    }

    if is_cli_absolute(&slash_path) {
        Some(normalize_absolute_lexically(&slash_path))
    } else {
        None
    }
}

/// Platform-independent parent extraction for absolute prompt paths. Rust's
/// host `Path` parser does not recognize Windows drive paths when tests run on
/// macOS/Linux, so normalize separators before finding the parent.
pub(crate) fn cli_parent_dir(path: &str) -> Option<String> {
    let normalized = path.trim().replace('\\', "/");
    let (parent, _) = normalized.rsplit_once('/')?;
    let parent = if parent.is_empty() { "/" } else { parent };
    normalize_cli_root(parent)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliWorkspacePlan {
    pub cwd: Option<String>,
    pub read_dirs: Vec<String>,
}

/// Select the provider cwd and the additional read-only roots. The artifact
/// directory wins as cwd in write mode. Roots already covered by cwd, or by a
/// less-specific root in the same list, are omitted.
pub(crate) fn plan_cli_workspace(
    cwd: Option<&str>,
    extra_read_dirs: &[&str],
    transient_read_root: Option<&str>,
    write_dir: Option<&str>,
) -> Result<CliWorkspacePlan, String> {
    let mut effective_cwd = write_dir
        .or(cwd)
        .or(transient_read_root)
        .map(|path| {
            normalize_cli_root(path)
                .ok_or_else(|| format!("Provider working directory must be absolute: {path}"))
        })
        .transpose()?;
    let mut roots: Vec<String> = extra_read_dirs
        .iter()
        .copied()
        .chain(transient_read_root)
        .filter(|path| !path.trim().is_empty())
        .map(|path| {
            normalize_cli_root(path)
                .ok_or_else(|| format!("Provider read directory must be absolute: {path}"))
        })
        .collect::<Result<_, _>>()?;
    // When the caller has no distinct cwd, use one of its explicit readable
    // roots instead of falling back to the process-wide temp directory.
    if effective_cwd.is_none() {
        effective_cwd = roots.first().cloned();
    }
    roots.retain(|root| {
        effective_cwd
            .as_deref()
            .is_none_or(|base| !same_or_descendant(root, base))
    });

    roots.sort_by(|a, b| {
        path_depth(a)
            .cmp(&path_depth(b))
            .then_with(|| path_sort_key(a).cmp(&path_sort_key(b)))
    });
    let mut minimal: Vec<String> = Vec::new();
    for root in roots {
        if !minimal.iter().any(|base| same_or_descendant(&root, base)) {
            minimal.push(root);
        }
    }
    minimal.sort_by_key(|p| path_sort_key(p));

    Ok(CliWorkspacePlan {
        cwd: effective_cwd,
        read_dirs: minimal,
    })
}

fn normalize_canonical_cli_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        rest.to_string()
    } else {
        normalized
    }
}

fn is_cli_absolute(path: &str) -> bool {
    path.starts_with('/')
        || (path.as_bytes().get(1) == Some(&b':')
            && path.as_bytes().get(2) == Some(&b'/')
            && path.as_bytes()[0].is_ascii_alphabetic())
}

fn normalize_absolute_lexically(path: &str) -> String {
    let (prefix, rest) = if path.starts_with("//") {
        ("//".to_string(), path.trim_start_matches('/'))
    } else if path.as_bytes().get(1) == Some(&b':') {
        (path[..2].to_string(), path[2..].trim_start_matches('/'))
    } else {
        ("/".to_string(), path.trim_start_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    match prefix.as_str() {
        "/" => format!("/{}", parts.join("/")),
        "//" => format!("//{}", parts.join("/")),
        drive => format!("{drive}/{}", parts.join("/")),
    }
}

fn path_sort_key(path: &str) -> String {
    if path.as_bytes().get(1) == Some(&b':') || path.starts_with("//") {
        path.to_ascii_lowercase()
    } else {
        path.to_string()
    }
}

fn path_depth(path: &str) -> usize {
    path.split('/').filter(|part| !part.is_empty()).count()
}

pub(super) fn same_or_descendant(path: &str, root: &str) -> bool {
    let path = path_sort_key(path.trim_end_matches('/'));
    let root = path_sort_key(root.trim_end_matches('/'));
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod prompt_file_tests {
    use super::*;

    #[test]
    fn large_system_instructions_stay_out_of_argv_and_are_removed_after_the_call() {
        let content = "β 📚 instructions\n".repeat(50_000);
        let prepared = prepare_cli_system_prompt(&content).unwrap();
        let path = std::path::PathBuf::from(&prepared.path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        let mut command = std::process::Command::new("claude.exe");
        command.args(["--append-system-prompt-file", &prepared.path]);
        validate_cli_command_length(&command, true).unwrap();
        drop(prepared);
        assert!(!path.exists());
    }

    #[test]
    fn remaining_arguments_are_checked_with_utf16_and_windows_quoting() {
        let mut command = std::process::Command::new("claude.exe");
        command.arg("x".repeat(32_753));
        validate_cli_command_length(&command, true).unwrap();
        command.arg("x");
        assert!(validate_cli_command_length(&command, true).is_err());
        let mut unicode = std::process::Command::new("claude.exe");
        unicode.arg("📚".repeat(17_000));
        assert!(validate_cli_command_length(&unicode, true).is_err());
        let mut escaped = std::process::Command::new("claude.exe");
        escaped.arg("\\\"".repeat(9_000));
        assert!(validate_cli_command_length(&escaped, true).is_err());
    }
}
