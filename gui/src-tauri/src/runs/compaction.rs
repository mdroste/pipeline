//! Shrinks finished runs on disk without changing what any reader sees.
//!
//! Each finished run, and at app start each older one, goes through three
//! steps, recorded in `stored::RECORD_PATH`:
//!
//! 1. Page images are removed when one of the run's PDFs renders every page
//!    again byte for byte with the bundled Poppler.
//! 2. Large text and data files are kept as `<name>.gz` when that saves a
//!    fifth or more.
//! 3. A file identical to one in an earlier run, or earlier in the same run,
//!    becomes a hard link to it.
//!
//! Every change is one atomic rename or removal, and the record is written
//! before a removal depends on it, so an interrupted pass leaves each file
//! readable. A run is skipped while a review holds it, until it has a
//! structured report, and while it keeps checkpoints that recovery could use.
use super::stored::{self, CompactionRecord, RebuildablePages, GZIP_SUFFIX};
use super::{read_utf8_at_most, short_sha256, ArtifactEntry, RunManifest, MAX_MANIFEST_BYTES};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Raise when compaction learns a step, so runs compacted earlier get it too.
pub(super) const VERSION: u32 = 1;
/// Smaller files save too little to be worth compressing.
const MIN_COMPRESS_BYTES: u64 = 256 * 1024;
/// Smaller files are never linked, so usage counts them without identity checks.
pub(super) const MIN_LINK_BYTES: u64 = 64 * 1024;
/// A compressed copy is kept only when it is at most this share of the original.
const MAX_COMPRESSED_SHARE: f64 = 0.8;
/// Large derived context files. `context/document.md` stays plain because
/// listing and recovery check it on every run.
const CONTEXT_FILES: [&str; 4] = [
    "context/document_bundle.json",
    "context/document_base.json",
    "context/paddle_structure.json",
    "context/blocks.jsonl",
];
/// Captured folder files that compress well and are only read through the
/// run-file readers. Images are streamed by path, so they stay plain.
const COMPRESSIBLE_SOURCE: [&str; 30] = [
    "csv", "tsv", "txt", "dat", "dta", "sav", "json", "jsonl", "xml", "tex", "bib", "md", "log",
    "smcl", "do", "ado", "r", "rmd", "qmd", "py", "ipynb", "m", "jl", "sas", "sps", "sql", "html",
    "htm", "yaml", "yml",
];
/// Where compaction stages files inside a run. It sits beside the run's
/// folders, so an interrupted pass never leaves a stray file inside the source
/// copy that a re-run fingerprints.
const SCRATCH_DIR: &str = ".compaction";

static REQUESTED: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Compact finished runs in the background. A request made while a pass is
/// running makes it go round once more.
pub fn schedule_compaction() {
    if cfg!(test) {
        return;
    }
    REQUESTED.store(true, Ordering::SeqCst);
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        while REQUESTED.swap(false, Ordering::SeqCst) {
            if let Err(error) = compact_finished_runs() {
                eprintln!("WARNING: saved reports could not be compacted: {error}");
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
        // A request that arrived after the last pass started still runs.
        if !REQUESTED.load(Ordering::SeqCst) || RUNNING.swap(true, Ordering::SeqCst) {
            break;
        }
    });
}

/// Compact every finished run that needs it. Returns how many changed.
pub fn compact_finished_runs() -> Result<usize, String> {
    let changed = compact_runs_in(&super::runs_dir()?, super::retention::is_held)?;
    if changed > 0 {
        super::history_reads::changed();
    }
    Ok(changed)
}

/// Compact the runs under `root`, oldest first so later runs link to earlier
/// ones. `is_held` is asked before each change to a run.
pub(super) fn compact_runs_in(
    root: &Path,
    is_held: impl Fn(&str) -> bool,
) -> Result<usize, String> {
    let mut runs = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Cannot list saved reports: {error}"))?
        .flatten()
    {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(run_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if super::validate_run_id(&run_id).is_err() {
            continue;
        }
        let dir = entry.path();
        let Some(manifest) = read_utf8_at_most(
            &dir.join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok()) else {
            continue;
        };
        runs.push((manifest.created.clone(), run_id, dir, manifest));
    }
    runs.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    let mut index = LinkIndex::default();
    let mut changed = 0;
    for (_, run_id, dir, manifest) in runs {
        // Left only if the app stopped during a pass.
        if fs::symlink_metadata(dir.join(SCRATCH_DIR)).is_ok() && !is_held(&run_id) {
            let _ = fs::remove_dir_all(dir.join(SCRATCH_DIR));
        }
        // A record that exists but cannot be read means leaving the run alone.
        let Ok(record) = stored::read_record(&dir) else {
            continue;
        };
        let record = record.unwrap_or_default();
        let held = || is_held(&run_id);
        if record.version >= VERSION || !compactable(&dir, &manifest) || held() {
            index.add_run(&dir, &manifest, &record);
            continue;
        }
        match compact_run(&dir, &manifest, record, &mut index, &held) {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(error) => eprintln!("WARNING: report {run_id} could not be compacted: {error}"),
        }
        let _ = fs::remove_dir_all(dir.join(SCRATCH_DIR));
    }
    Ok(changed)
}

/// Finished and with a structured report. Recovery rebuilds only a missing
/// report, so a stopped run that has one keeps its checkpoints only for
/// inspection; compaction leaves those files exactly as they are.
fn compactable(dir: &Path, manifest: &RunManifest) -> bool {
    matches!(
        manifest.status.as_str(),
        "done" | "degraded" | "partial" | "failed" | "cancelled" | "interrupted"
    ) && fs::symlink_metadata(dir.join("report.json")).is_ok_and(|meta| meta.is_file())
        && fs::symlink_metadata(dir.join(".task-pin")).is_err()
}

fn compact_run(
    dir: &Path,
    manifest: &RunManifest,
    mut record: CompactionRecord,
    index: &mut LinkIndex,
    held: &dyn Fn() -> bool,
) -> Result<bool, String> {
    let scratch = dir.join(SCRATCH_DIR);
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir(&scratch).map_err(|error| format!("Cannot prepare compaction: {error}"))?;
    let mut changed = false;

    if record.pages.is_none() && !held() {
        if let Some(pages) = rebuildable_pages(dir, manifest) {
            record.pages = Some(pages.clone());
            stored::save_record(dir, &record)?;
            for rel in &pages.paths {
                if held() {
                    return Ok(true);
                }
                let _ = fs::remove_file(dir.join(rel));
            }
            let _ = fs::remove_dir(dir.join("artifacts/pages"));
            changed = true;
        }
    }

    let listed: HashSet<&str> = manifest
        .artifacts
        .iter()
        .map(|entry| entry.rel_path.as_str())
        .collect();
    let mut compressed = Vec::new();
    for entry in &manifest.artifacts {
        if held() {
            return Ok(changed);
        }
        if compress(dir, &scratch, &listed, &record, entry)? {
            compressed.push(entry.rel_path.clone());
        }
    }
    if !compressed.is_empty() {
        record.compressed.extend(compressed);
        stored::save_record(dir, &record)?;
        changed = true;
    }
    // Retire each original whose compressed copy is recorded and still holds
    // its exact bytes, including ones an earlier pass could not remove.
    let entries: HashMap<&str, &ArtifactEntry> = manifest
        .artifacts
        .iter()
        .map(|entry| (entry.rel_path.as_str(), entry))
        .collect();
    for rel in &record.compressed {
        if held() {
            return Ok(changed);
        }
        let plain = dir.join(rel);
        let Some(entry) = entries.get(rel.as_str()) else {
            continue;
        };
        if fs::symlink_metadata(&plain).is_ok()
            && holds_original(&dir.join(format!("{rel}{GZIP_SUFFIX}")), entry)
        {
            let _ = fs::remove_file(&plain);
        }
    }

    for entry in &manifest.artifacts {
        if held() {
            return Ok(changed);
        }
        changed |= index.link(dir, &scratch, entry, &record);
    }
    record.version = VERSION;
    stored::save_record(dir, &record)?;
    Ok(changed)
}

/// The run's page image paths, page 1 first, when it has a complete set.
fn page_paths(manifest: &RunManifest) -> Option<Vec<String>> {
    if let Some(pages) = &manifest.page_artifacts {
        return (1..=pages.count)
            .map(|page| pages.rel_path(page))
            .collect::<Result<_, _>>()
            .ok();
    }
    // Older runs list each page in the manifest.
    let mut numbered: Vec<(u32, String)> = manifest
        .artifacts
        .iter()
        .filter(|entry| entry.group == "pages")
        .map(|entry| {
            let name = entry.rel_path.strip_prefix("artifacts/pages/")?;
            Some((stored::page_number(name)?, entry.rel_path.clone()))
        })
        .collect::<Option<_>>()?;
    numbered.sort();
    let contiguous = numbered
        .iter()
        .map(|(page, _)| *page)
        .eq(1..=numbered.len() as u32);
    contiguous.then(|| numbered.into_iter().map(|(_, rel)| rel).collect())
}

/// The pages and the PDF that renders all of them exactly, if there is one.
fn rebuildable_pages(dir: &Path, manifest: &RunManifest) -> Option<RebuildablePages> {
    let paths = page_paths(manifest)?;
    let count = u32::try_from(paths.len()).ok()?;
    if paths.is_empty()
        || count > crate::pipeline::extract::MAX_RENDERED_PDF_PAGES
        || !paths.iter().all(|rel| rel.ends_with(".jpg"))
    {
        return None;
    }
    let stored_pages: Vec<Vec<u8>> = paths
        .iter()
        .map(|rel| read_plain(&dir.join(rel)))
        .collect::<Option<_>>()?;
    let candidates = manifest.artifacts.iter().filter(|entry| {
        let rel = entry.rel_path.to_ascii_lowercase();
        rel.ends_with(".pdf")
            && (rel.starts_with("source-snapshot/") || rel.starts_with("artifacts/document/"))
    });
    for candidate in candidates {
        let pdf = dir.join(&candidate.rel_path);
        let first = tempfile::tempdir().ok()?;
        let Ok(page) = crate::pipeline::extract::render_retained_pdf_page(&pdf, first.path(), 1)
        else {
            continue;
        };
        if read_plain(&page).as_ref() != Some(&stored_pages[0]) {
            continue;
        }
        let all = tempfile::tempdir().ok()?;
        let Ok(rendered) =
            crate::pipeline::extract::render_retained_pdf_pages(&pdf, all.path(), count)
        else {
            continue;
        };
        let mut matched = 0usize;
        for name in &rendered.names {
            let Some(index) =
                stored::page_number(name).and_then(|page| (page as usize).checked_sub(1))
            else {
                continue;
            };
            if stored_pages
                .get(index)
                .is_some_and(|stored| read_plain(&all.path().join(name)).as_ref() == Some(stored))
            {
                matched += 1;
            }
        }
        if matched == stored_pages.len() {
            return Some(RebuildablePages {
                pdf: candidate.rel_path.clone(),
                paths,
            });
        }
    }
    None
}

fn read_plain(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(path)
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

fn compressible(rel: &str) -> bool {
    if CONTEXT_FILES.contains(&rel) {
        return true;
    }
    let Some(inside) = rel.strip_prefix("source-snapshot/tree/") else {
        return false;
    };
    Path::new(inside)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            COMPRESSIBLE_SOURCE.contains(&extension.to_ascii_lowercase().as_str())
        })
}

/// Write `<rel>.gz` beside an eligible file. The original stays until the
/// record lists the compressed copy.
fn compress(
    dir: &Path,
    scratch: &Path,
    listed: &HashSet<&str>,
    record: &CompactionRecord,
    entry: &ArtifactEntry,
) -> Result<bool, String> {
    let rel = entry.rel_path.as_str();
    if entry.bytes < MIN_COMPRESS_BYTES || !compressible(rel) || record.compressed.contains(rel) {
        return Ok(false);
    }
    // The run already keeps a file under the compressed name.
    if listed.contains(format!("{rel}{GZIP_SUFFIX}").as_str()) {
        return Ok(false);
    }
    let Some(bytes) = read_plain(&dir.join(rel)) else {
        return Ok(false);
    };
    // Compress only the exact bytes the run captured.
    if bytes.len() as u64 != entry.bytes || short_sha256(&bytes) != entry.sha256 {
        return Ok(false);
    }
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(bytes.len() / 4),
        flate2::Compression::new(6),
    );
    encoder
        .write_all(&bytes)
        .map_err(|error| format!("Cannot compress {rel}: {error}"))?;
    let packed = encoder
        .finish()
        .map_err(|error| format!("Cannot compress {rel}: {error}"))?;
    if packed.len() as f64 > bytes.len() as f64 * MAX_COMPRESSED_SHARE {
        return Ok(false);
    }
    // The compressed copy keeps the original's modified time, so a copy
    // expanded from it matches the file the run captured in every respect.
    let modified = fs::symlink_metadata(dir.join(rel))
        .and_then(|metadata| metadata.modified())
        .ok();
    // A `.gz` left by an interrupted pass is replaced.
    publish(
        scratch,
        &dir.join(format!("{rel}{GZIP_SUFFIX}")),
        &packed,
        modified,
    )?;
    Ok(true)
}

fn publish(
    scratch: &Path,
    destination: &Path,
    bytes: &[u8],
    modified: Option<std::time::SystemTime>,
) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(scratch)
        .map_err(|error| format!("Cannot stage {}: {error}", destination.display()))?;
    temp.write_all(bytes)
        .and_then(|()| temp.flush())
        .map_err(|error| format!("Cannot stage {}: {error}", destination.display()))?;
    if let Some(modified) = modified {
        let _ = temp.as_file().set_modified(modified);
    }
    crate::safety::sync_file(temp.as_file())
        .map_err(|error| format!("Cannot sync {}: {error}", destination.display()))?;
    crate::safety::persist_file(temp, destination)
        .map_err(|error| format!("Cannot publish {}: {}", destination.display(), error.error))?;
    Ok(())
}

/// Whether a compressed copy expands to exactly the bytes `entry` records.
fn holds_original(compressed: &Path, entry: &ArtifactEntry) -> bool {
    let Ok((bytes, more)) =
        stored::StoredFile::Gzip(compressed.to_path_buf()).read_at_most(entry.bytes as usize)
    else {
        return false;
    };
    !more && bytes.len() as u64 == entry.bytes && short_sha256(&bytes) == entry.sha256
}

/// The first stored copy of each distinct file seen so far.
#[derive(Default)]
struct LinkIndex {
    files: HashMap<(bool, String, u64), PathBuf>,
    staged: u64,
}

impl LinkIndex {
    /// Where `entry`'s bytes are kept, keyed by their identity. Removed pages
    /// and small files are left out.
    fn stored(
        dir: &Path,
        entry: &ArtifactEntry,
        record: &CompactionRecord,
    ) -> Option<((bool, String, u64), PathBuf)> {
        // Recovery ignores a linked checkpoint, so checkpoints stay unlinked.
        if entry.sha256.is_empty() || entry.rel_path.starts_with("artifacts/checkpoints/") {
            return None;
        }
        let compressed = record.compressed.contains(&entry.rel_path);
        let path = if compressed {
            dir.join(format!("{}{GZIP_SUFFIX}", entry.rel_path))
        } else {
            dir.join(&entry.rel_path)
        };
        let metadata = fs::symlink_metadata(&path).ok()?;
        (metadata.is_file() && metadata.len() >= MIN_LINK_BYTES)
            .then(|| ((compressed, entry.sha256.clone(), entry.bytes), path))
    }

    fn add_run(&mut self, dir: &Path, manifest: &RunManifest, record: &CompactionRecord) {
        for entry in &manifest.artifacts {
            if let Some((key, path)) = Self::stored(dir, entry, record) {
                self.files.entry(key).or_insert(path);
            }
        }
    }

    /// Replace `entry`'s copy with a link to an identical earlier one.
    fn link(
        &mut self,
        dir: &Path,
        scratch: &Path,
        entry: &ArtifactEntry,
        record: &CompactionRecord,
    ) -> bool {
        let Some((key, path)) = Self::stored(dir, entry, record) else {
            return false;
        };
        let Some(target) = self.files.get(&key).cloned() else {
            self.files.insert(key, path);
            return false;
        };
        let same_file = match (stored::file_identity(&target), stored::file_identity(&path)) {
            (Some((left, _)), Some((right, _))) => left == right,
            _ => true,
        };
        // Captured files keep the original's modified time, which a link
        // would replace with the other copy's.
        let modified = |path: &Path| fs::symlink_metadata(path).and_then(|m| m.modified()).ok();
        if entry.rel_path.starts_with("source-snapshot/") && modified(&target) != modified(&path) {
            return false;
        }
        if same_file || !same_contents(&target, &path) {
            return false;
        }
        self.staged += 1;
        let staged = scratch.join(format!("link-{}", self.staged));
        if fs::hard_link(&target, &staged).is_err() {
            // The folder cannot hold links, or the file has as many as it can.
            return false;
        }
        if fs::rename(&staged, &path).is_err() {
            let _ = fs::remove_file(&staged);
            return false;
        }
        true
    }
}

fn same_contents(left: &Path, right: &Path) -> bool {
    let (Ok(left), Ok(right)) = (
        crate::safety::open_regular_file(left),
        crate::safety::open_regular_file(right),
    ) else {
        return false;
    };
    if left.metadata().map(|m| m.len()).ok() != right.metadata().map(|m| m.len()).ok() {
        return false;
    }
    let (mut left, mut right) = (
        std::io::BufReader::new(left),
        std::io::BufReader::new(right),
    );
    let (mut a, mut b) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let Ok(count) = left.read(&mut a) else {
            return false;
        };
        if count == 0 {
            return right.read(&mut b[..1]).is_ok_and(|n| n == 0);
        }
        if right.read_exact(&mut b[..count]).is_err() || a[..count] != b[..count] {
            return false;
        }
    }
}

#[cfg(test)]
#[path = "tests/compaction.rs"]
mod tests;
