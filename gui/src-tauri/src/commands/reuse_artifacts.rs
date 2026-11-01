//! Immutable source and producer-file handoffs between Review runs.
use crate::pipeline::extract::{plan_folder_copy, ScopedSourceContext, MAX_FOLDER_COPY_FILES};
use crate::safety::WalkBudget;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// How copying one regular file ended.
enum Copied {
    Bytes(u64),
    OverLimit,
    Unreadable(String),
}

/// Copy one validated handle to `destination`, up to `limit` bytes. Never
/// links: a later write to either side must remain isolated. Per-file flushes
/// cost about ten milliseconds each on a Mac, so callers flush once instead.
fn copy_regular(mut file: std::fs::File, destination: &Path, limit: u64) -> Result<Copied, String> {
    use std::io::{Read, Write};
    let parent = destination.parent().ok_or("Artifact has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let modified = file.metadata().and_then(|metadata| metadata.modified());
    let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut chunk = vec![0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        let count = match file.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => return Ok(Copied::Unreadable(error.to_string())),
        };
        copied += count as u64;
        if copied > limit {
            return Ok(Copied::OverLimit);
        }
        pending
            .write_all(&chunk[..count])
            .map_err(|e| e.to_string())?;
    }
    // A folder's fingerprint covers modified times. The copy keeps the
    // original's, so an unchanged folder is recognized from run to run.
    if let Ok(modified) = modified {
        let _ = pending.as_file().set_modified(modified);
    }
    pending.persist(destination).map_err(|e| e.to_string())?;
    Ok(Copied::Bytes(copied))
}

/// The one flush of a finished copy. Windows cannot open a directory to flush
/// it. A rerun verifies retained copies against their fingerprint.
fn flush_copy(destination: &Path) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    if destination.is_file() {
        options.write(true);
    } else if cfg!(not(unix)) {
        return Ok(());
    }
    options
        .open(destination)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())
}

/// Copy a run-owned tree exactly: only regular files, through validated
/// handles, with one aggregate bound. A selected folder goes through
/// [`snapshot_folder`] instead, because this refuses what a folder can hold.
/// Never link parent and child artifacts: a later write must remain isolated.
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
    let mut budget = WalkBudget::new("Review artifact copy").without_time_limit();
    let mut remaining = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;
    while let Some((source, destination)) = pending.pop() {
        budget.entry()?;
        let meta = std::fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        if meta.is_symlink() {
            return Err(format!("Cannot copy symlink {}", source.display()));
        }
        if meta.is_dir() {
            budget.directory()?;
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
            let mut entries = std::fs::read_dir(&source)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            entries.sort_by_key(|e| e.file_name());
            pending.extend(
                entries
                    .into_iter()
                    .map(|e| (e.path(), destination.join(e.file_name()))),
            );
        } else {
            let file = crate::safety::open_regular_file(&source)?;
            match copy_regular(file, &destination, remaining)? {
                Copied::Bytes(bytes) => remaining -= bytes,
                Copied::OverLimit => return Err("Review artifact copy exceeds 256 MiB".into()),
                Copied::Unreadable(error) => return Err(error),
            }
        }
    }
    flush_copy(destination)
}

pub(super) fn snapshot_source(
    source: &ScopedSourceContext,
    root: &Path,
) -> Result<ScopedSourceContext, String> {
    let Some(path) = source.source_path.as_ref() else {
        return Ok(ScopedSourceContext::default());
    };
    let destination = root.join("source-snapshot").join(if path.is_dir() {
        "tree".into()
    } else {
        path.file_name()
            .ok_or("Source has no filename")?
            .to_os_string()
    });
    copy_tree(path, &destination)?;
    Ok(ScopedSourceContext {
        source_path: Some(destination.clone()),
        read_root: Some(if destination.is_dir() {
            destination
        } else {
            destination.parent().unwrap().into()
        }),
    })
}

/// Copy a selected folder under the limits in `pipeline::extract`. What it
/// leaves out is recorded in the copy; nothing the folder holds fails a run.
/// Later copies of the result, retained or restored, use [`snapshot_source`]
/// and keep it exactly, so a rerun reads the same files.
fn snapshot_folder(
    source: &ScopedSourceContext,
    root: &Path,
    max_files: usize,
) -> Result<ScopedSourceContext, String> {
    let Some(path) = source.source_path.as_ref() else {
        return Ok(ScopedSourceContext::default());
    };
    let selected = path
        .canonicalize()
        .map_err(|error| format!("Failed to resolve selected folder: {error}"))?;
    let destination = root.join("source-snapshot/tree");
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let mut plan = plan_folder_copy(&selected, max_files, crate::commands::is_cancelled)?;
    let mut budget = WalkBudget::new_cancellable("Review folder copy").without_time_limit();
    let mut remaining = crate::pipeline::extract::MAX_FOLDER_COPY_BYTES;
    for relative in &plan.files {
        budget.entry()?;
        let file = selected.join(relative);
        // A folder swapped for a link since the plan must not lead outside.
        let inside = file
            .canonicalize()
            .is_ok_and(|real| real.starts_with(&selected));
        let Some(handle) = inside
            .then(|| crate::safety::open_regular_file(&file).ok())
            .flatten()
        else {
            plan.omissions.unreadable += 1;
            continue;
        };
        let limit = remaining.min(crate::pipeline::extract::MAX_FOLDER_COPY_FILE_BYTES);
        match copy_regular(handle, &destination.join(relative), limit)? {
            Copied::Bytes(bytes) => remaining -= bytes,
            // It grew after the plan measured it.
            Copied::OverLimit => plan.omissions.record_too_large(relative, limit),
            Copied::Unreadable(_) => plan.omissions.unreadable += 1,
        }
    }
    plan.omissions.save(&destination)?;
    flush_copy(&destination)?;
    Ok(ScopedSourceContext {
        source_path: Some(destination.clone()),
        read_root: Some(destination),
    })
}

/// How many files each folder input of a run may copy. Every copied file and
/// step output takes a manifest entry, and a full manifest would fail the run
/// after extraction, so the folders share what the planned steps leave. One
/// folder always gets the full limit; a run with several gets less only when
/// they could not all fit.
pub(super) fn folder_file_limit(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
    primary_is_folder: bool,
    named: &HashMap<String, String>,
) -> usize {
    let is_selected = |slot: &&crate::pipeline_config::InputSlot| {
        slot.mode == "folder"
            && named
                .get(&slot.key)
                .is_some_and(|path| !path.trim().is_empty())
    };
    let named_folders = config.extraction.extra_inputs.iter().filter(is_selected);
    let folders = named_folders.count() + usize::from(primary_is_folder);
    let steps = crate::safety::estimate_run_budget(config, settings)
        .map(|estimate| estimate.step_units_upper_bound)
        .unwrap_or(crate::safety::MAX_RUN_STEP_UNITS)
        .min(crate::safety::MAX_RUN_STEP_UNITS) as usize;
    (crate::runs::MAX_RUN_PLANNED_ARTIFACTS.saturating_sub(steps) / folders.max(1))
        .min(MAX_FOLDER_COPY_FILES)
}

/// Capture what a run reads from its staged input, off the async workers,
/// because a selected folder can take as long as its disk does to copy.
pub(super) async fn capture_source(
    staged: ScopedSourceContext,
    input_mode: &str,
    root: &Path,
    folder_files: usize,
) -> Result<ScopedSourceContext, String> {
    let folder = input_mode == "folder";
    let root = root.to_path_buf();
    crate::run_scope::spawn_blocking(move || {
        if folder {
            snapshot_folder(&staged, &root, folder_files)
        } else {
            snapshot_source(&staged, &root)
        }
    })
    .await
    .map_err(|error| format!("Source capture task failed: {error}"))?
}

/// What the launch preview and `check` should tell a researcher about the
/// selected folders: files that won't fit in the Review's private copy.
pub(super) async fn folder_capture_notices(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
    primary: Option<&str>,
    primary_is_folder: bool,
    named: &HashMap<String, String>,
) -> Vec<String> {
    let mut folders: Vec<String> = primary
        .filter(|_| primary_is_folder)
        .map(str::to_string)
        .into_iter()
        .collect();
    for slot in &config.extraction.extra_inputs {
        if slot.mode == "folder" {
            folders.extend(named.get(&slot.key).cloned());
        }
    }
    folders.retain(|path| !path.trim().is_empty());
    if folders.is_empty() {
        return Vec::new();
    }
    let max_files = folder_file_limit(config, settings, primary_is_folder, named);
    notices_within(folders, max_files, crate::safety::MAX_WALK_TIME).await
}

/// Scan `folders` under one deadline, because nothing else can stop a preview:
/// no run exists yet, and the researcher is waiting for it to open.
async fn notices_within(folders: Vec<String>, max_files: usize, limit: Duration) -> Vec<String> {
    let deadline = Instant::now() + limit;
    let scanned = folders.clone();
    let scan = move || {
        let notice = |folder: &String| {
            let path = Path::new(folder);
            match plan_folder_copy(path, max_files, || Instant::now() >= deadline) {
                Ok(plan) => plan.notice(path),
                Err(_) if Instant::now() >= deadline => {
                    Some(crate::pipeline::extract::slow_scan_notice(path, max_files))
                }
                // A folder that can't be read reports its problem at launch.
                Err(_) => None,
            }
        };
        scanned.iter().filter_map(notice).collect()
    };
    wait_for_blocking(scan, limit + Duration::from_secs(1))
        .await
        .unwrap_or_else(|| {
            let slow = |folder: &String| {
                crate::pipeline::extract::slow_scan_notice(Path::new(folder), max_files)
            };
            folders.iter().map(slow).collect()
        })
}

/// Stop waiting for blocking work after `patience`. Work stuck inside one slow
/// system call can't see a deadline; it ends by itself and nothing waits on it.
async fn wait_for_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    patience: Duration,
) -> Option<T> {
    tokio::time::timeout(patience, crate::run_scope::spawn_blocking(work))
        .await
        .ok()?
        .ok()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SourceRecord {
    path: Option<String>,
    fingerprint: String,
}

fn source_fingerprint(root: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut hash = Sha256::new();
    let mut pending = vec![root.to_path_buf()];
    let mut walk = WalkBudget::new("Captured source verification").without_time_limit();
    let mut remaining = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;
    while let Some(path) = pending.pop() {
        walk.entry()?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_symlink() {
            return Err("Captured source contains a symlink".into());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        if metadata.is_dir() {
            walk.directory()?;
            hash.update(b"directory");
            let mut children = std::fs::read_dir(path)
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            children.sort();
            pending.extend(children);
        } else {
            hash.update(b"file");
            let mut bytes = Vec::new();
            crate::safety::open_regular_file(&path)?
                .take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            remaining = remaining
                .checked_sub(bytes.len() as u64)
                .ok_or("Captured source exceeds 256 MiB")?;
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Whether a run's retained copy still has the fingerprint recorded when it
/// was captured. `None` means no record covers `file`.
pub(super) fn retained_copy_matches(run_dir: &Path, file: &Path) -> Option<bool> {
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&run_dir.join("context/source-snapshot.json"))
        .ok()?
        .take(4097)
        .read_to_end(&mut bytes)
        .ok()?;
    let record: SourceRecord = serde_json::from_slice(&bytes).ok()?;
    if run_dir.join(record.path?) != file {
        return None;
    }
    Some(source_fingerprint(file).ok()? == record.fingerprint)
}

pub(super) fn retain_source(
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
) -> Result<(), String> {
    retain_source_at(source, writer, "")
}

fn retain_source_at(
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
    prefix: &str,
) -> Result<(), String> {
    let root = writer.dir().join(prefix);
    let retained = snapshot_source(source, &root)?;
    let relative = retained.source_path.as_ref().map(|p| {
        p.strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    });
    let rel = |tail: &str| {
        if prefix.is_empty() {
            tail.to_string()
        } else {
            format!("{prefix}/{tail}")
        }
    };
    // Register before fingerprinting, and never destructively: the record
    // must describe exactly what stays on disk or every rerun fails its
    // integrity check.
    if retained.source_path.is_some() {
        writer.register_captured_tree(&rel("source-snapshot"), "source")?;
    }
    let record = SourceRecord {
        path: relative,
        fingerprint: retained
            .source_path
            .as_deref()
            .map(source_fingerprint)
            .transpose()?
            .unwrap_or_default(),
    };
    writer.add_text(
        &rel("context/source-snapshot.json"),
        "Captured source identity",
        "context",
        &serde_json::to_string(&record).map_err(|e| e.to_string())?,
    )?;
    Ok(())
}

pub(super) fn restore_source(
    parent: &Path,
    root: &Path,
    required: bool,
) -> Result<ScopedSourceContext, String> {
    let record = parent.join("context/source-snapshot.json");
    if !record.exists() {
        if required {
            return Err("This run has no immutable source snapshot. Start a new run to use source files; rerun cannot substitute the current files for the original revision.".into());
        }
        return Ok(ScopedSourceContext::default());
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&record)?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4096 {
        return Err("Invalid source snapshot record".into());
    }
    let record: SourceRecord = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let Some(relative) = record.path else {
        if required {
            return Err("The parent run did not capture source files. Start a new run.".into());
        }
        return Ok(ScopedSourceContext::default());
    };
    if !Path::new(&relative)
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
        || !relative.starts_with("source-snapshot/")
    {
        return Err("Invalid source snapshot path".into());
    }
    let captured = snapshot_source(
        &ScopedSourceContext {
            source_path: Some(parent.join(relative)),
            read_root: None,
        },
        root,
    )?;
    if source_fingerprint(
        captured
            .source_path
            .as_deref()
            .ok_or("Missing restored source")?,
    )? != record.fingerprint
    {
        return Err("Captured source failed its integrity check. Start a new run from the intended source revision.".into());
    }
    Ok(captured)
}

pub(super) fn retain_named_source(
    key: &str,
    source: &ScopedSourceContext,
    writer: &mut crate::runs::RunWriter,
) -> Result<(), String> {
    retain_source_at(
        source,
        writer,
        &format!(
            "named-sources/{}",
            crate::pipeline::executor::step_slug(key)
        ),
    )
}

pub(super) fn restore_named_source(
    parent: &Path,
    key: &str,
    root: &Path,
    required: bool,
) -> Result<ScopedSourceContext, String> {
    let relative = Path::new("named-sources").join(crate::pipeline::executor::step_slug(key));
    restore_source(&parent.join(&relative), &root.join(relative), required)
}

pub(super) fn requires_primary_source(config: &crate::pipeline_config::PipelineConfig) -> bool {
    config.steps.iter().filter(|s| s.enabled).any(|s| s.context.include.iter().any(|selector|
        matches!(selector, crate::pipeline_config::ArtifactSelector::Primary { parts } if parts.contains(&crate::pipeline_config::PrimaryArtifactPart::Source))))
}

pub(super) fn requires_named_source(
    config: &crate::pipeline_config::PipelineConfig,
    key: &str,
) -> bool {
    config.steps.iter().filter(|s| s.enabled).any(|s| s.context.include.iter().any(|selector|
        matches!(selector, crate::pipeline_config::ArtifactSelector::NamedInput { key: selected, parts } if selected == key && parts.contains(&crate::pipeline_config::NamedInputArtifactPart::Source))))
}

/// Adapters may read Word XML and TeX figures; point those reads at the same
/// captured revision as extraction, then restore the user-facing provenance.
pub(super) fn build_bundle(
    extraction: &crate::models::ExtractionResult,
    source: Option<&Path>,
    destination: &Path,
) -> Result<crate::document_bundle::BundleBuild, String> {
    let mut captured = extraction.clone();
    captured.source_path = source
        .map(|path| {
            if path.is_dir() && extraction.method != "folder" {
                crate::pipeline::extract::find_main_tex(path).unwrap_or_else(|| path.into())
            } else {
                path.into()
            }
        })
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    if source.is_none() {
        captured.method = "resumed-document".into();
    }
    let mut build = crate::document_bundle::build(&captured, destination)?;
    build
        .bundle
        .extraction
        .source_path
        .clone_from(&extraction.source_path);
    if let Some(origin) = build.bundle.origins.first_mut() {
        origin.path.clone_from(&extraction.source_path);
    }
    Ok(build)
}

pub(super) fn restore_producers(
    parent: &Path,
    destination: &Path,
    ids: impl Iterator<Item = String>,
) -> Result<(), String> {
    for id in ids {
        let slug = crate::pipeline::executor::step_slug(&id);
        let source = parent.join("artifacts/by-step").join(&slug);
        if source.exists() {
            copy_tree(&source, &destination.join("by-step").join(slug))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_sources_and_producers_are_independent_of_originals() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original.tex");
        std::fs::write(&original, "original revision").unwrap();
        let parent = temp.path().join("parent");
        let staged = snapshot_source(
            &ScopedSourceContext {
                source_path: Some(original.clone()),
                read_root: None,
            },
            &parent,
        )
        .unwrap();
        std::fs::create_dir_all(parent.join("context")).unwrap();
        let relative = staged
            .source_path
            .as_ref()
            .unwrap()
            .strip_prefix(&parent)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        std::fs::write(
            parent.join("context/source-snapshot.json"),
            serde_json::to_vec(&SourceRecord {
                path: Some(relative),
                fingerprint: source_fingerprint(staged.source_path.as_ref().unwrap()).unwrap(),
            })
            .unwrap(),
        )
        .unwrap();
        std::fs::write(&original, "changed live revision").unwrap();
        let child = temp.path().join("child");
        let restored = restore_source(&parent, &child, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(restored.source_path.unwrap()).unwrap(),
            "original revision"
        );
        let named_parent = temp.path().join("named-parent");
        let named_root = named_parent
            .join("named-sources")
            .join(crate::pipeline::executor::step_slug("appendix"));
        copy_tree(&parent, &named_root).unwrap();
        let named = restore_named_source(&named_parent, "appendix", &child, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(named.source_path.unwrap()).unwrap(),
            "original revision"
        );
        let slug = crate::pipeline::executor::step_slug("producer");
        let files = parent
            .join("artifacts/by-step")
            .join(&slug)
            .join("unit/files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("results.csv"), "1,2").unwrap();
        restore_producers(
            &parent,
            &child.join("artifacts"),
            std::iter::once("producer".into()),
        )
        .unwrap();
        std::fs::write(
            child
                .join("artifacts/by-step")
                .join(slug)
                .join("unit/files/results.csv"),
            "3,4",
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(files.join("results.csv")).unwrap(),
            "1,2"
        );
        assert!(restore_source(temp.path(), &child, true).is_err());
        std::fs::write(
            staged.source_path.as_ref().unwrap(),
            "tampered retained revision",
        )
        .unwrap();
        assert!(
            restore_source(&parent, &temp.path().join("tampered-child"), true)
                .unwrap_err()
                .contains("integrity check")
        );
    }
    #[test]
    fn retained_source_keeps_large_files_and_round_trips_trees() {
        let temp = tempfile::tempdir().unwrap();
        let big = temp.path().join("scan.pdf");
        std::fs::write(&big, vec![0u8; 51_000_000]).unwrap();
        let mut writer = crate::runs::RunWriter::create_in(temp.path(), "big_run").unwrap();
        retain_source(
            &ScopedSourceContext {
                source_path: Some(big.clone()),
                read_root: None,
            },
            &mut writer,
        )
        .unwrap();
        let run_dir = writer.dir().to_path_buf();
        assert!(run_dir.join("source-snapshot/scan.pdf").is_file());
        let restored = restore_source(&run_dir, &temp.path().join("big_child"), true).unwrap();
        assert_eq!(
            std::fs::metadata(restored.source_path.unwrap())
                .unwrap()
                .len(),
            51_000_000
        );

        // A modest tree round-trips; the 500-entry registration cap is
        // covered without per-file fsync cost in runs::tests.
        let tree = temp.path().join("tree");
        std::fs::create_dir_all(&tree).unwrap();
        for index in 0..24 {
            std::fs::write(tree.join(format!("{index:04}.tex")), "x").unwrap();
        }
        let mut writer = crate::runs::RunWriter::create_in(temp.path(), "tree_run").unwrap();
        retain_source(
            &ScopedSourceContext {
                source_path: Some(tree.clone()),
                read_root: Some(tree.clone()),
            },
            &mut writer,
        )
        .unwrap();
        let run_dir = writer.dir().to_path_buf();
        assert_eq!(
            std::fs::read_dir(run_dir.join("source-snapshot/tree"))
                .unwrap()
                .count(),
            24
        );
        let restored = restore_source(&run_dir, &temp.path().join("tree_child"), true).unwrap();
        assert_eq!(
            std::fs::read_dir(restored.source_path.unwrap())
                .unwrap()
                .count(),
            24
        );
    }

    /// Capture the way a run does, in a private scope so that cancellation in
    /// other tests cannot reach it.
    fn capture(selected: &Path, mode: &str, root: &Path) -> ScopedSourceContext {
        capture_up_to(selected, mode, root, MAX_FOLDER_COPY_FILES)
    }

    fn capture_up_to(
        selected: &Path,
        mode: &str,
        root: &Path,
        max_files: usize,
    ) -> ScopedSourceContext {
        let staged = crate::pipeline::extract::stage_selected_source(selected, mode, root).unwrap();
        let run = crate::run_scope::ActiveScope::begin(format!("capture-{}", root.display()));
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(crate::run_scope::scoped(
                run.scope().clone(),
                capture_source(staged, mode, root, max_files),
            ))
            .unwrap()
    }

    fn sparse(path: &Path, bytes: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::File::create(path).unwrap().set_len(bytes).unwrap();
    }

    const GB: u64 = 1024 * 1024 * 1024;

    /// Without selective copying, any one of these fails the launch: the
    /// symlink, the pipe, the data file, or a copy that flushes every file.
    #[test]
    #[cfg(unix)]
    fn a_replication_folder_reaches_inventory_with_its_omissions_noted() {
        use std::os::unix::ffi::OsStrExt as _;
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("package");
        std::fs::create_dir_all(package.join("code")).unwrap();
        for index in 0..600 {
            std::fs::write(package.join(format!("code/{index:04}.do")), "display 1").unwrap();
        }
        std::fs::write(package.join("README.md"), "# Replication").unwrap();
        std::fs::write(package.join(".DS_Store"), "x").unwrap();
        for name in ["config", "objects/aa/object", "refs/heads/main"] {
            let path = package.join(".git").join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "git").unwrap();
        }
        std::fs::create_dir_all(package.join("node_modules/pkg")).unwrap();
        std::fs::write(package.join("node_modules/pkg/index.js"), "x").unwrap();
        std::os::unix::fs::symlink(package.join("README.md"), package.join("link.md")).unwrap();
        let fifo = package.join("blocked.pipe");
        let fifo_c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        sparse(&package.join("data/panel.dta"), 3 * GB);

        let private = temp.path().join("private");
        let captured = capture(&package, "folder", &private);
        let tree = captured.source_path.as_ref().unwrap();
        let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read_dir(tree.join("code")).unwrap().count(), 600);
        assert!(
            inventory.text.contains("601 files"),
            "{}",
            &inventory.text[..200]
        );
        assert!(inventory.text.contains("| code/0599.do |"));
        assert!(!tree.join("data/panel.dta").exists());
        assert!(!tree.join(".git").exists());
        assert!(!tree.join("node_modules").exists());
        assert!(!tree.join("link.md").exists());
        let notes = inventory.quality_notes.join("\n");
        assert!(notes.contains("data/panel.dta (3.0 GB)"), "{notes}");
        assert!(notes.contains(".git, node_modules"), "{notes}");
        for skipped in ["1 hidden item", "1 symbolic link", "1 special file"] {
            assert!(notes.contains(skipped), "{notes}");
        }
        assert!(inventory.text.contains("| data/panel.dta | 3221225472 |"));

        // The retained copy and a rerun's restored copy repeat the same files
        // exactly, so reruns verify and report the same omissions.
        let mut writer = crate::runs::RunWriter::create_in(temp.path(), "folder_run").unwrap();
        retain_source(&captured, &mut writer).unwrap();
        let restored = restore_source(writer.dir(), &temp.path().join("rerun"), true).unwrap();
        let reread = crate::pipeline::extract::ingest_folder(
            restored.source_path.unwrap().to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(reread.quality_notes, inventory.quality_notes);
    }

    #[test]
    fn an_unchanged_folder_keeps_its_fingerprint_from_run_to_run() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder");
        std::fs::create_dir_all(folder.join("code")).unwrap();
        std::fs::write(folder.join("notes.md"), "notes").unwrap();
        std::fs::write(folder.join("code/main.do"), "use data").unwrap();
        let fingerprint = |run: &str| {
            let captured = capture(&folder, "folder", &temp.path().join(run));
            let tree = captured.source_path.unwrap();
            crate::pipeline::extract::ingest_folder(tree.to_str().unwrap())
                .unwrap()
                .paper_hash
        };

        let first = fingerprint("first-run");
        assert_eq!(first, fingerprint("second-run"));

        // A same-size edit is still a different folder.
        std::fs::write(folder.join("notes.md"), "edits").unwrap();
        assert_ne!(first, fingerprint("third-run"));
    }

    #[test]
    fn a_complete_folder_copy_leaves_no_record() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("folder");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("notes.md"), "notes").unwrap();
        let captured = capture(&folder, "folder", &temp.path().join("private"));
        let tree = captured.source_path.unwrap();
        assert_eq!(std::fs::read_dir(&tree).unwrap().count(), 1);
        let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();
        assert!(inventory.quality_notes.is_empty());
    }

    #[test]
    fn a_latex_project_keeps_files_a_folder_copy_would_leave_out() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        for (name, text) in [
            (
                "main.tex",
                "\\documentclass{article}\n\\input{build/macros}\n\\input{.shared/defs}\n\\includegraphics{target/plot.pdf}\n",
            ),
            ("build/macros.tex", "macros"),
            (".shared/defs.tex", "definitions"),
            ("target/plot.pdf", "%PDF-1.4"),
        ] {
            let path = project.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let captured = capture(&project, "document", &temp.path().join("private"));
        let tree = captured.source_path.unwrap();
        for name in [
            "main.tex",
            "build/macros.tex",
            ".shared/defs.tex",
            "target/plot.pdf",
        ] {
            assert!(tree.join(name).is_file(), "{name} was not captured");
        }
    }

    #[tokio::test]
    async fn launch_notices_name_only_folders_that_will_not_fit() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("package");
        sparse(&package.join("data/panel.dta"), 3 * GB);
        std::fs::write(package.join("README.md"), "# Replication").unwrap();
        let tidy = temp.path().join("tidy");
        std::fs::create_dir_all(&tidy).unwrap();
        std::fs::write(tidy.join("notes.md"), "notes").unwrap();
        let mut config = crate::pipeline_config::PipelineConfig {
            steps: Vec::new(),
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        config.extraction.extra_inputs = vec![crate::pipeline_config::InputSlot {
            key: "appendix".into(),
            mode: "folder".into(),
            ..Default::default()
        }];
        let package = package.to_str().unwrap();
        let tidy = tidy.to_str().unwrap();
        let named =
            |path: &str| std::collections::HashMap::from([("appendix".into(), path.into())]);

        let settings = crate::settings::Settings::default();
        let primary =
            folder_capture_notices(&config, &settings, Some(package), true, &named(tidy)).await;
        assert_eq!(primary.len(), 1, "{primary:?}");
        assert!(
            primary[0].starts_with("package holds 3.0 GB in 2 files."),
            "{}",
            primary[0]
        );
        assert!(primary[0].contains("1 file (3.0 GB) will be left out"));
        let slot =
            folder_capture_notices(&config, &settings, Some(tidy), true, &named(package)).await;
        assert_eq!(slot, primary);
        let document =
            folder_capture_notices(&config, &settings, Some(package), false, &named(tidy)).await;
        assert!(document.is_empty());
    }

    fn config_with_folders(slots: &[&str]) -> crate::pipeline_config::PipelineConfig {
        let mut config = crate::pipeline_config::PipelineConfig {
            steps: Vec::new(),
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        config.extraction.extra_inputs = slots
            .iter()
            .map(|key| crate::pipeline_config::InputSlot {
                key: key.to_string(),
                mode: "folder".into(),
                ..Default::default()
            })
            .collect();
        config
    }

    fn selected(slots: &[&str]) -> HashMap<String, String> {
        slots
            .iter()
            .map(|key| (key.to_string(), "/selected".to_string()))
            .collect()
    }

    #[test]
    fn folders_share_what_the_manifest_can_hold() {
        let settings = crate::settings::Settings::default();
        let limit =
            |config: &_, primary, named: &_| folder_file_limit(config, &settings, primary, named);
        let two = config_with_folders(&["a", "b"]);
        assert_eq!(limit(&two, true, &HashMap::new()), MAX_FOLDER_COPY_FILES);
        assert_eq!(limit(&two, true, &selected(&["a"])), 2_000);
        assert_eq!(limit(&two, true, &selected(&["a", "b"])), 1_333);
        assert_eq!(limit(&two, false, &selected(&["a", "b"])), 2_000);

        // Each planned step output also takes an entry, and a single folder
        // keeps the full limit at the largest run the safety limits allow.
        let mut fan_out = config_with_folders(&["a"]);
        fan_out.steps = vec![crate::pipeline_config::StepConfig {
            id: "fan".into(),
            enabled: true,
            phase: crate::pipeline_config::Phase::Parallel,
            for_each: Some(crate::pipeline_config::ForEach {
                glob: "**/*".into(),
                max: 1_000,
                artifact: None,
            }),
            ..Default::default()
        }];
        assert_eq!(limit(&fan_out, true, &selected(&["a"])), 1_500);
        fan_out.steps[0].for_each.as_mut().unwrap().max = 2_000;
        assert_eq!(limit(&fan_out, true, &HashMap::new()), 2_000);
    }

    #[test]
    fn three_full_folders_fit_one_run_only_at_their_shared_limit() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("package");
        std::fs::create_dir_all(&folder).unwrap();
        for index in 0..MAX_FOLDER_COPY_FILES {
            std::fs::write(folder.join(format!("{index:04}.do")), "x").unwrap();
        }
        let retain_three = |captured: &ScopedSourceContext, run: &str| {
            let mut writer = crate::runs::RunWriter::create_in(temp.path(), run).unwrap();
            retain_source(captured, &mut writer)?;
            retain_named_source("appendix", captured, &mut writer)?;
            retain_named_source("archive", captured, &mut writer)?;
            writer.add_text("report.md", "Report", "report", "finished")
        };

        let config = config_with_folders(&["appendix", "archive"]);
        let named = selected(&["appendix", "archive"]);
        let shared =
            folder_file_limit(&config, &crate::settings::Settings::default(), true, &named);
        let private = temp.path().join("shared");
        let fitted = capture_up_to(&folder, "folder", &private, shared);
        retain_three(&fitted, "shared_run").unwrap();
        let tree = fitted.source_path.unwrap();
        let inventory = crate::pipeline::extract::ingest_folder(tree.to_str().unwrap()).unwrap();
        assert!(
            inventory.quality_notes[0].contains("up to 1,333 files"),
            "{:?}",
            inventory.quality_notes
        );

        let full = capture(&folder, "folder", &temp.path().join("full"));
        let error = retain_three(&full, "full_run").unwrap_err();
        assert!(error.contains("could not be fully registered"), "{error}");
    }

    #[tokio::test]
    async fn a_preview_scan_that_runs_out_of_time_says_so() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().to_str().unwrap().to_string();
        let name = temp.path().file_name().unwrap().to_string_lossy();

        let notices = notices_within(vec![folder], MAX_FOLDER_COPY_FILES, Duration::ZERO).await;

        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].starts_with(&format!("{name} took too long to scan")));
        assert!(notices[0].contains("up to 2,000 files"), "{}", notices[0]);
    }

    #[tokio::test]
    async fn a_preview_stops_waiting_for_a_scan_stuck_in_the_system() {
        let started = Instant::now();
        let stuck = || std::thread::sleep(Duration::from_millis(600));

        let finished = wait_for_blocking(stuck, Duration::from_millis(20)).await;

        assert!(finished.is_none());
        assert!(started.elapsed() < Duration::from_millis(400));
        assert_eq!(
            wait_for_blocking(|| 7, Duration::from_secs(5)).await,
            Some(7)
        );
    }

    #[test]
    fn legacy_bundle_rebuild_never_opens_the_live_document() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("changed.docx");
        std::fs::write(&original, "a changed and invalid Word document").unwrap();
        let extraction = crate::models::ExtractionResult {
            text: "captured text".into(),
            method: "resumed-document".into(),
            source_path: original.to_string_lossy().into_owned(),
            paper_hash: "captured-hash".into(),
            quality_notes: Vec::new(),
        };
        let build = build_bundle(&extraction, None, temp.path()).unwrap();
        assert_eq!(build.bundle.source_kind, "text");
        assert_eq!(build.bundle.extraction.source_path, extraction.source_path);
    }
}
