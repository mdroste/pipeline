//! Bounded reads for local profiles and saved run context.

use std::io::Read;

/// Maximum file size for imported configs (10 MB). Pipeline configs are
/// small JSON; anything larger is almost certainly the wrong file.
pub(super) const MAX_IMPORT_SIZE: u64 = 10_000_000;
pub(super) const MAX_RUN_CONTEXT_SIZE: u64 = 64 * 1024 * 1024;

/// Read a file for import, rejecting files above the size limit.
pub(super) fn read_import_file(path: &str) -> Result<String, String> {
    let file = crate::safety::open_regular_file(std::path::Path::new(path))
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {path}: {e}"))?;
    if bytes.len() as u64 > MAX_IMPORT_SIZE {
        return Err(format!(
            "File is too large. Import files should be under {} MB.",
            MAX_IMPORT_SIZE / 1_000_000,
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("Import file is not valid UTF-8: {e}"))
}
