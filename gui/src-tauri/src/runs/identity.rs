//! Run identities, storage root and the existing cross-process lock path.
use super::records::InputIdentity;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static RUN_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn new_run_id(paper_hash: &str) -> String {
    let sequence = RUN_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}-{sequence:016x}",
        paper_hash,
        chrono::Local::now().format("%Y%m%d-%H%M%S-%9f")
    )
}

pub fn input_identity(
    input_path: &str,
    interpretation: &str,
    content_hash: &str,
    inherited: Option<&InputIdentity>,
) -> InputIdentity {
    let selection = if input_path.trim().is_empty() {
        String::new()
    } else {
        let path = Path::new(input_path);
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('\\', "/")
    };
    let selection_key = if selection.is_empty() {
        String::new()
    } else {
        format!("{}:{selection}", interpretation.trim())
    };
    let inherited_lineage = inherited
        .map(|identity| identity.lineage_id.trim())
        .filter(|lineage| !lineage.is_empty());
    let lineage_id = inherited_lineage.map(str::to_string).unwrap_or_else(|| {
        if selection_key.is_empty() {
            String::new()
        } else {
            let digest = Sha256::digest(selection_key.as_bytes());
            format!(
                "lineage-{}",
                digest[..8]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            )
        }
    });
    InputIdentity {
        selection_key,
        content_hash: content_hash.trim().to_string(),
        lineage_id,
    }
}

pub fn runs_dir() -> Result<PathBuf, String> {
    let dir = crate::storage::data_root()?.join("runs");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create runs dir: {e}"))?;
    Ok(dir)
}

/// Run ids are `{hash}_{timestamp}` — same alphabet as validated paper hashes.
pub fn validate_run_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 96 {
        return Err("Invalid run id".into());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid run id".into());
    }
    Ok(())
}

/// The one OS-level lock shared by the pipeline guard, incomplete-run
/// recovery, and pin release. It lives next to the active `runs/` directory so
/// a custom data folder never splits the lock between two files.
pub fn run_lock_path() -> Result<PathBuf, String> {
    Ok(crate::storage::data_root()?.join("run.lock"))
}

/// Open (creating if needed) the shared run lock without locking it.
pub fn open_run_lock(path: &Path) -> Result<fs::File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create Pipeline data directory: {e}"))?;
    }
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("Failed to open the Pipeline run lock: {e}"))
}
