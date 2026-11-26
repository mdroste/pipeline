//! Immutable source and producer-file handoffs between Review runs.
use crate::pipeline::extract::{plan_folder_copy, ScopedSourceContext, MAX_FOLDER_COPY_FILES};
use crate::runs::receipts::source_fingerprint;
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
    crate::safety::persist_file(pending, destination).map_err(|e| e.to_string())?;
    Ok(Copied::Bytes(copied))
}

/// The one flush of a finished copy. Windows cannot open a directory to flush
/// it. A rerun verifies retained copies against their fingerprint, so a refused
/// flush is logged and doesn't fail a copy that has landed.
fn flush_copy(destination: &Path) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    if destination.is_file() {
        options.write(true);
    } else if cfg!(not(unix)) {
        return Ok(());
    }
    let copy = options.open(destination).map_err(|e| e.to_string())?;
    crate::safety::sync_landed(&copy);
    Ok(())
}

/// Copy a run-owned tree exactly: only regular files, through validated
/// handles, with one aggregate bound. A selected folder goes through
/// [`snapshot_folder`] instead, because this refuses what a folder can hold.
/// Never link parent and child artifacts: a later write must remain isolated.
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    copy_tree_except(source, destination, |_| false)
}

/// [`copy_tree`], leaving out each path `skip` names.
fn copy_tree_except(
    source: &Path,
    destination: &Path,
    skip: impl Fn(&Path) -> bool,
) -> Result<(), String> {
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
                    .filter(|e| !skip(&e.path()))
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
    let mut plan = plan_folder_copy(&selected, max_files, crate::cancellation::is_cancelled)?;
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

/// Carries reused steps' files into a new run. A unit the parent never
/// finished stays behind, so its files cannot reach the new run's later steps.
pub(super) fn restore_producers(
    parent: &Path,
    destination: &Path,
    ids: impl Iterator<Item = String>,
) -> Result<(), String> {
    for id in ids {
        let slug = crate::pipeline::executor::step_slug(&id);
        let source = parent.join("artifacts/by-step").join(&slug);
        if source.exists() {
            copy_tree_except(&source, &destination.join("by-step").join(slug), |path| {
                path.parent() == Some(source.as_path())
                    && crate::pipeline::unit_files::call_is_open(path)
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
