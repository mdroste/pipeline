//! Per-call read and write authority and canonical read admission.

/// Per-provider-call filesystem authority. Keeping it owned by the call is
/// what allows parallel steps to receive disjoint artifact views.
#[derive(Debug, Clone, Default)]
pub struct ToolAccess {
    read_roots: Vec<std::path::PathBuf>,
    pub(super) write_root: Option<std::path::PathBuf>,
}

impl ToolAccess {
    pub fn new(read_roots: &[&str], write_root: Option<&str>) -> Self {
        let write_root = write_root
            .filter(|root| !root.trim().is_empty())
            .map(std::path::PathBuf::from);
        let mut readable: Vec<std::path::PathBuf> = read_roots
            .iter()
            .filter(|root| !root.trim().is_empty())
            .map(std::path::PathBuf::from)
            .collect();
        if let Some(root) = write_root.as_ref() {
            readable.push(root.clone());
        }
        Self {
            read_roots: readable,
            write_root,
        }
    }
}

/// Validate a tool-read path: resolve symlinks, check it's under an allowed
/// directory, and enforce a size limit.  Returns the canonical path on success.
pub(super) fn validate_tool_path(
    access: &ToolAccess,
    path: &str,
    max_size: usize,
) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);

    // Resolve to canonical path to prevent traversal via symlinks or ..
    let canonical = p
        .canonicalize()
        .map_err(|_| format!("File not found: {path}"))?;

    // Validate the path against the exact roots registered for this run. The
    // OS-wide temporary directory is deliberately not implicit: unrelated
    // applications commonly place credentials and private documents there.
    let is_allowed = access.read_roots.iter().any(|dir| {
        std::path::Path::new(dir)
            .canonicalize()
            .map(|d| canonical.starts_with(&d))
            .unwrap_or(false)
    });

    if !is_allowed {
        return Err(format!(
            "Access denied: {path} is outside allowed directories"
        ));
    }

    let metadata =
        std::fs::metadata(&canonical).map_err(|e| format!("Cannot read file metadata: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("Access denied: {path} is not a regular file"));
    }
    if metadata.len() as usize > max_size {
        return Err(format!(
            "File too large ({} bytes, max {})",
            metadata.len(),
            max_size
        ));
    }

    Ok(canonical)
}
