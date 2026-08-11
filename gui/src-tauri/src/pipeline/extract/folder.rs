use super::*;

// ── Generalized ingest modes ────────────────────────────────────────
//
// "folder" and "none" input modes for non-document workflows. Folder mode
// records an inventory of the directory as the context text — file contents
// are never inlined; steps open files on demand with the Read tool. None
// mode runs the pipeline from the prompts alone.

/// Directories that never belong in an inventory.
pub(super) const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
];
/// Inventory size cap — beyond this the listing notes the truncation.
pub(super) const MAX_INVENTORY_FILES: usize = 2000;

/// Resolve the effective input mode from the profile setting and the path.
/// A directory path implies folder mode even if the profile says document,
/// so picking a folder in the UI "just works" with any profile.
pub fn effective_input_mode(configured: &str, input_path: &str) -> &'static str {
    match configured {
        "none" => "none",
        "folder" => "folder",
        _ => {
            if !input_path.is_empty() && Path::new(input_path).is_dir() {
                "folder"
            } else {
                "document"
            }
        }
    }
}

/// Build a file-inventory context for a folder input.
pub fn ingest_folder(root: &str) -> Result<ExtractionResult, String> {
    let root_path = PathBuf::from(root);
    if !root_path.is_dir() {
        return Err(format!("Not a directory: {root}"));
    }

    let canonical_root = root_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve input folder {root}: {e}"))?;

    let mut files: Vec<(String, u64, PathBuf)> = Vec::new();
    let mut stack = vec![canonical_root.clone()];
    let mut visited_dirs = std::collections::HashSet::new();
    let mut examined_entries = 0usize;
    let mut examined_dirs = 0usize;
    let mut skipped_special = 0usize;
    let mut truncated = false;
    while let Some(dir) = stack.pop() {
        if crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        let Ok(canonical_dir) = dir.canonicalize() else {
            continue;
        };
        if !canonical_dir.starts_with(&canonical_root)
            || !visited_dirs.insert(canonical_dir.clone())
        {
            continue;
        }
        examined_dirs += 1;
        if examined_dirs > MAX_INVENTORY_DIRS {
            truncated = true;
            break;
        }
        let entries = match fs::read_dir(&canonical_dir) {
            Ok(e) => e,
            Err(_) => continue, // unreadable subdir: skip, don't fail the run
        };
        for entry in entries.flatten() {
            examined_entries += 1;
            if examined_entries > MAX_INVENTORY_ENTRIES {
                truncated = true;
                stack.clear();
                break;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // Never follow directory/file symlinks from an inventory. Apart
            // from escaping the selected root, a directory symlink can form a
            // cycle and a file symlink can be swapped for a special file.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if file_type.is_file() {
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.file_type().is_file() {
                    continue;
                }
                if files.len() >= MAX_INVENTORY_FILES {
                    truncated = true;
                    continue;
                }
                let rel = path
                    .strip_prefix(&canonical_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((rel, meta.len(), path));
            } else {
                skipped_special += 1;
            }
        }
    }
    files.sort();

    let root_display = root.replace('\\', "/");
    let mut text = format!(
        "# Input folder inventory\n\nRoot: {root_display}\n\n{} files. File contents are NOT included here — use the Read tool with paths under the root to open any file you need.\n\n| File | Bytes |\n|---|---|\n",
        files.len()
    );
    for (rel, size, _) in &files {
        text.push_str(&format!("| {rel} | {size} |\n"));
    }
    if truncated {
        text.push_str(&format!(
            "\n> Inventory truncated by its safety limits (maximum {MAX_INVENTORY_FILES} files, {MAX_INVENTORY_ENTRIES} entries, and {MAX_INVENTORY_DIRS} directories).\n"
        ));
    }
    if skipped_special > 0 {
        text.push_str(&format!(
            "\n> Skipped {skipped_special} non-regular filesystem entr{} (for example, a socket or named pipe).\n",
            if skipped_special == 1 { "y" } else { "ies" }
        ));
    }

    let mut hasher = Sha256::new();
    let mut chunk = [0u8; 64 * 1024];
    let mut hashed_bytes = 0u64;
    for (rel, size, path) in &files {
        if hashed_bytes.saturating_add(*size) > MAX_INVENTORY_HASH_BYTES {
            return Err(format!(
                "Input folder exceeds the {} GB hashing safety limit",
                MAX_INVENTORY_HASH_BYTES / 1024 / 1024 / 1024
            ));
        }
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_le_bytes());
        let mut file = open_regular_file(path)
            .map_err(|e| format!("Failed to hash {}: {e}", path.display()))?;
        loop {
            if crate::commands::is_cancelled() {
                return Err("Pipeline cancelled".to_string());
            }
            let count = std::io::Read::read(&mut file, &mut chunk)
                .map_err(|e| format!("Failed to hash {}: {e}", path.display()))?;
            if count == 0 {
                break;
            }
            hashed_bytes = hashed_bytes.saturating_add(count as u64);
            if hashed_bytes > MAX_INVENTORY_HASH_BYTES {
                return Err(format!(
                    "Input folder exceeds the {} GB hashing safety limit",
                    MAX_INVENTORY_HASH_BYTES / 1024 / 1024 / 1024
                ));
            }
            hasher.update(&chunk[..count]);
        }
    }
    let hash = format!("{:x}", hasher.finalize())[..16].to_string();
    Ok(ExtractionResult {
        text,
        method: "folder".to_string(),
        source_path: root.to_string(),
        paper_hash: hash,
        quality_notes: vec![],
    })
}

/// Async boundary for folder inventory/hash work. The blocking implementation
/// checks the global cancellation flag between entries and read chunks.
pub async fn ingest_folder_async(root: &str) -> Result<ExtractionResult, String> {
    let root = root.to_string();
    tokio::task::spawn_blocking(move || ingest_folder(&root))
        .await
        .map_err(|e| format!("Folder ingestion task failed: {e}"))?
}

/// Context for a workflow that takes no input at all.
pub fn ingest_none() -> ExtractionResult {
    let stamp = chrono::Local::now().to_rfc3339();
    let hash = format!("{:x}", Sha256::digest(stamp.as_bytes()))[..16].to_string();
    ExtractionResult {
        text: "(This workflow runs from its step prompts alone; there is no input document.)"
            .to_string(),
        method: "none".to_string(),
        source_path: String::new(),
        paper_hash: hash,
        quality_notes: vec![],
    }
}

#[cfg(test)]
mod ingest_tests {
    use super::*;

    #[test]
    fn effective_mode_resolution() {
        assert_eq!(effective_input_mode("none", ""), "none");
        assert_eq!(effective_input_mode("folder", "/x"), "folder");
        assert_eq!(
            effective_input_mode("", "/definitely/not/a/dir.pdf"),
            "document"
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            effective_input_mode("", dir.path().to_str().unwrap()),
            "folder"
        );
    }

    #[test]
    fn folder_inventory_lists_files_and_skips_junk() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.py"), "print(1)").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/data.csv"), "a,b\n1,2").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "x").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/junk.js"), "x").unwrap();

        let result = ingest_folder(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(result.method, "folder");
        assert!(result.text.contains("main.py"));
        assert!(result.text.contains("sub/data.csv"));
        assert!(!result.text.contains("junk.js"));
        assert!(!result.text.contains(".git/config"));
        assert_eq!(result.paper_hash.len(), 16);
    }

    #[test]
    fn folder_hash_changes_when_same_size_content_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.txt");
        fs::write(&file, "alpha").unwrap();
        let first = ingest_folder(dir.path().to_str().unwrap())
            .unwrap()
            .paper_hash;
        fs::write(&file, "bravo").unwrap();
        let second = ingest_folder(dir.path().to_str().unwrap())
            .unwrap()
            .paper_hash;
        assert_ne!(first, second);
    }

    #[test]
    #[cfg(unix)]
    fn folder_inventory_skips_symlink_cycles_and_special_files() {
        use std::os::unix::ffi::OsStrExt as _;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("regular.txt"), "safe").unwrap();
        std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();

        let fifo = dir.path().join("blocked.pipe");
        let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

        let result = ingest_folder(dir.path().to_str().unwrap()).unwrap();
        assert!(result.text.contains("regular.txt"));
        assert!(!result.text.contains("cycle/"));
        assert!(!result.text.contains("blocked.pipe |"));
        assert!(result
            .text
            .contains("Skipped 1 non-regular filesystem entry"));
    }

    #[test]
    fn none_mode_produces_placeholder_context() {
        let result = ingest_none();
        assert_eq!(result.method, "none");
        assert!(result.source_path.is_empty());
        assert_eq!(result.paper_hash.len(), 16);
    }
}
