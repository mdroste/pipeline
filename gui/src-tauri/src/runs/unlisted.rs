//! Registration of files that landed in a run directory without passing
//! through `add_text`: model-written output (bounded destructively) and
//! captured input trees (never deleted).

use super::*;

/// Limits for registering files that landed in a run directory without
/// passing through `add_text`.
struct UnlistedPolicy {
    max_files: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
    /// Model-written output: delete what exceeds a limit (an unindexed disk
    /// leak). Captured inputs never use this; they fail instead.
    delete_excess: bool,
}

impl UnlistedPolicy {
    fn model_output(group: &str) -> Self {
        Self {
            max_files: if group == "agent_response" {
                2_000
            } else {
                500
            },
            max_file_bytes: 50_000_000,
            max_total_bytes: 250_000_000,
            delete_excess: true,
        }
    }

    fn captured_input() -> Self {
        Self {
            max_files: crate::safety::MAX_WALK_ENTRIES,
            max_file_bytes: crate::safety::MAX_RUN_OUTPUT_BYTES as u64,
            max_total_bytes: crate::safety::MAX_RUN_OUTPUT_BYTES as u64,
            delete_excess: false,
        }
    }
}

impl RunWriter {
    /// Register files under `subdir` (relative to the run dir) that aren't
    /// already in the artifact index. Model-written supporting files land
    /// on disk without going through `add_text`; this picks them up at the
    /// end of the run. Symlinks are skipped, oversized unlisted files are
    /// deleted, and the walk is capped defensively. A file the manifest already
    /// references is left alone whatever its size. Returns how many were added.
    pub fn register_unlisted(&mut self, subdir: &str, group: &str) -> usize {
        self.register_tree(subdir, group, UnlistedPolicy::model_output(group))
            .unwrap_or_else(|added| added)
    }

    /// Register a tree that was already bounded when it was copied (256 MiB
    /// total, no symlinks) such as a captured source snapshot. Hashing it takes
    /// as long as the disk takes, so no clock applies. Never deletes anything:
    /// a captured input must stay complete so reruns can verify it. Returns
    /// an error instead of truncating when a limit is hit.
    pub fn register_captured_tree(&mut self, subdir: &str, group: &str) -> Result<usize, String> {
        self.register_tree(subdir, group, UnlistedPolicy::captured_input())
            .map_err(|_| format!("Captured {group} tree could not be fully registered"))
    }

    /// Shared walker. `Err(added)` reports a limit or registration failure
    /// under a non-destructive policy (destructive policies never fail).
    fn register_tree(
        &mut self,
        subdir: &str,
        group: &str,
        policy: UnlistedPolicy,
    ) -> Result<usize, usize> {
        let mut known: std::collections::HashSet<String> =
            self.artifacts.iter().map(|a| a.rel_path.clone()).collect();
        if let Some(pages) = &self.page_artifacts {
            known.extend((1..=pages.count).filter_map(|page| pages.rel_path(page).ok()));
        }
        let mut added = 0usize;
        let mut total_bytes = 0u64;
        let mut stack = vec![self.dir.join(subdir)];
        let mut walk = crate::safety::WalkBudget::new("Run artifact discovery");
        if !policy.delete_excess {
            walk = walk.without_time_limit();
        }
        let stop = |added: usize| {
            if policy.delete_excess {
                Ok(added)
            } else {
                Err(added)
            }
        };
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for entry in entries.flatten() {
                if walk.entry().is_err() {
                    return stop(added);
                }
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_symlink() {
                    if !policy.delete_excess {
                        return Err(added);
                    }
                    continue;
                }
                if ft.is_dir() {
                    if walk.directory().is_err() {
                        return stop(added);
                    }
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    // A CLI model can create FIFOs/sockets on Unix. Never open
                    // them during finalization; remove them from the run so a
                    // later artifact read cannot block either.
                    if !policy.delete_excess {
                        return Err(added);
                    }
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let Ok(rel) = path.strip_prefix(&self.dir) else {
                    continue;
                };
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                // Every limit below can delete, so a file the manifest already
                // references must be recognized first.
                if known.contains(&rel_str) {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt as _;
                    if metadata.nlink() > 1 {
                        if !policy.delete_excess {
                            return Err(added);
                        }
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                }
                if metadata.len() > policy.max_file_bytes
                    || total_bytes.saturating_add(metadata.len()) > policy.max_total_bytes
                {
                    if !policy.delete_excess {
                        return Err(added);
                    }
                    // These files were produced inside Pipeline's per-run
                    // model sandbox. Remove runaway output rather than leave
                    // an unindexed, unbounded disk leak behind.
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if added >= policy.max_files {
                    if !policy.delete_excess {
                        return Err(added);
                    }
                    // Keep walking after the registration cap. Otherwise a
                    // model can place unlimited unindexed files after the
                    // first 500 and evade every byte quota.
                    let _ = fs::remove_file(&path);
                    continue;
                }
                let relative_label = rel_str
                    .strip_prefix(&format!("{subdir}/"))
                    .unwrap_or(&rel_str);
                let label = if group == "agent_response" {
                    agent_response_label(relative_label)
                } else {
                    relative_label.to_string()
                };
                match self.register_existing(&rel_str, &label, group) {
                    Ok(()) => {
                        added += 1;
                        total_bytes += metadata.len();
                    }
                    Err(_) if policy.delete_excess => {}
                    Err(_) => return Err(added),
                }
            }
        }
        Ok(added)
    }
}
