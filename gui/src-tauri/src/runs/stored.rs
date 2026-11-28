//! How a finished run keeps its files after compaction, and how to read them.
//!
//! Compaction (`super::compaction`) may keep a large text or data file as
//! `<name>.gz`, remove page images that the run's PDF renders again byte for
//! byte, and turn identical files into hard links. The manifest keeps listing
//! each file under its original name, size and checksum. A reader that opens
//! a finished run's file by path asks [`locate`] where its bytes are and reads
//! them through [`StoredFile`], so it sees the original bytes in every case.
use super::{read_at_most, read_utf8_at_most, write_text_atomic};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};

/// What compaction did to one run. Missing until compaction first runs.
pub(crate) const RECORD_PATH: &str = "context/storage-compaction.json";
const RECORD_DIR: &str = "context";
const RECORD_NAME: &str = "storage-compaction.json";
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
/// Appended to a compressed file's name. The manifest lists the name without it.
pub(crate) const GZIP_SUFFIX: &str = ".gz";
/// Bound on decompressing one file, matching a run's largest captured copy.
const MAX_EXPANDED_BYTES: u64 = crate::safety::MAX_RUN_OUTPUT_BYTES as u64;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct CompactionRecord {
    /// The compaction steps this run has completed; see `compaction::VERSION`.
    #[serde(default)]
    pub(crate) version: u32,
    /// Run-relative paths kept as `<path>.gz`.
    #[serde(default)]
    pub(crate) compressed: BTreeSet<String>,
    /// Present once the page images were removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pages: Option<RebuildablePages>,
}

/// Page images that the run's own PDF renders again exactly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RebuildablePages {
    /// Run-relative path of the PDF that rendered every page identically.
    pub(crate) pdf: String,
    /// Run-relative path of each page image, page 1 first.
    pub(crate) paths: Vec<String>,
}

impl RebuildablePages {
    fn page_of(&self, rel: &str) -> Option<u32> {
        let index = self.paths.iter().position(|path| path == rel)?;
        u32::try_from(index + 1).ok()
    }
}

/// The run's record, `Ok(None)` before compaction, or an error when the record
/// exists but cannot be read. Compaction leaves such a run alone.
pub(crate) fn read_record(run_dir: &Path) -> Result<Option<CompactionRecord>, String> {
    let path = run_dir.join(RECORD_PATH);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect the compaction record: {error}")),
        Ok(_) => {}
    }
    let json = read_utf8_at_most(&path, MAX_RECORD_BYTES, "Compaction record")?;
    serde_json::from_str(&json)
        .map(Some)
        .map_err(|error| format!("Invalid compaction record: {error}"))
}

fn record(run_dir: &Path) -> CompactionRecord {
    read_record(run_dir).ok().flatten().unwrap_or_default()
}

pub(super) fn save_record(run_dir: &Path, record: &CompactionRecord) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("Failed to serialize the compaction record: {error}"))?;
    if json.len() > MAX_RECORD_BYTES {
        return Err("The compaction record exceeds its size limit".into());
    }
    let directory = run_dir.join(RECORD_DIR);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create {}: {error}", directory.display()))?;
    write_text_atomic(&directory, RECORD_NAME, &json)
}

/// Where the bytes of one run file are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredFile {
    /// The file itself, possibly a hard link shared with another run.
    Plain(PathBuf),
    /// A gzip file holding the original bytes.
    Gzip(PathBuf),
    /// A removed page image, rendered again from `pdf` when read.
    Page { pdf: PathBuf, page: u32 },
}

/// Where `rel` keeps its bytes inside `run_dir`, or `None` when the run has
/// no such file. `run_dir` must be canonical and `rel` a relative path without
/// `..`. Every path returned is inside `run_dir`.
pub(crate) fn locate(run_dir: &Path, rel: &str) -> Option<StoredFile> {
    let inside = |path: PathBuf| {
        path.canonicalize()
            .ok()
            .filter(|resolved| resolved.starts_with(run_dir))
    };
    let plain = run_dir.join(rel);
    if fs::symlink_metadata(&plain).is_ok() {
        return inside(plain).map(StoredFile::Plain);
    }
    // A manifest that lists `rel` never lists `rel.gz` once compaction made
    // it, so a missing file with a `.gz` beside it is always the compressed
    // copy. Tree copies use the record instead; see [`expand_copy`].
    let compressed = run_dir.join(format!("{rel}{GZIP_SUFFIX}"));
    if fs::symlink_metadata(&compressed).is_ok() {
        return inside(compressed).map(StoredFile::Gzip);
    }
    let pages = record(run_dir).pages?;
    let page = pages.page_of(rel)?;
    let pdf = inside(run_dir.join(&pages.pdf))?;
    Some(StoredFile::Page { pdf, page })
}

/// Whether the run has `rel` in any stored form.
pub(crate) fn exists(run_dir: &Path, rel: &str) -> bool {
    run_dir
        .canonicalize()
        .ok()
        .and_then(|dir| locate(&dir, rel))
        .is_some()
}

impl StoredFile {
    /// The file on disk: the file itself, its `.gz`, or the PDF of a page.
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Plain(path) | Self::Gzip(path) => path,
            Self::Page { pdf, .. } => pdf,
        }
    }

    /// The original size, when it is known without reading the whole file.
    pub(crate) fn logical_len(&self) -> Result<Option<u64>, String> {
        match self {
            Self::Plain(path) => crate::safety::open_regular_file(path)?
                .metadata()
                .map(|metadata| Some(metadata.len()))
                .map_err(|error| format!("Cannot inspect {}: {error}", path.display())),
            Self::Gzip(path) => gzip_original_len(path).map(Some),
            Self::Page { .. } => Ok(None),
        }
    }

    /// The original bytes, at most `limit`. The flag says more existed.
    pub(crate) fn read_at_most(&self, limit: usize) -> Result<(Vec<u8>, bool), String> {
        match self {
            Self::Plain(path) => read_at_most(path, limit),
            Self::Gzip(path) => {
                let file = crate::safety::open_regular_file(path)?;
                read_limited(flate2::read::GzDecoder::new(file), limit)
            }
            Self::Page { pdf, page } => {
                let directory = tempfile::Builder::new()
                    .prefix("pipeline-page-")
                    .tempdir()
                    .map_err(|error| format!("Cannot prepare a page image: {error}"))?;
                let rendered = crate::pipeline::extract::render_retained_pdf_page(
                    pdf,
                    directory.path(),
                    *page,
                )?;
                read_at_most(&rendered, limit)
            }
        }
    }
}

fn read_limited(reader: impl Read, limit: usize) -> Result<(Vec<u8>, bool), String> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024) + 1);
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read a compressed run file: {error}"))?;
    let more = bytes.len() > limit;
    bytes.truncate(limit);
    Ok((bytes, more))
}

/// A gzip member ends with the original size modulo 2^32. Run files stay far
/// below 4 GiB, so it is exact.
fn gzip_original_len(path: &Path) -> Result<u64, String> {
    use std::io::{Seek as _, SeekFrom};
    let mut file = crate::safety::open_regular_file(path)?;
    let mut trailer = [0u8; 4];
    file.seek(SeekFrom::End(-4))
        .and_then(|_| file.read_exact(&mut trailer))
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    Ok(u64::from(u32::from_le_bytes(trailer)))
}

/// Read `rel` from `run_dir` as its original bytes, refusing more than `limit`.
/// `Ok(None)` when the run has no such file.
pub(crate) fn read_run_file_bytes(
    run_dir: &Path,
    rel: &str,
    limit: usize,
) -> Result<Option<Vec<u8>>, String> {
    if rel.is_empty()
        || Path::new(rel).is_absolute()
        || rel.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid run-relative path".into());
    }
    let Ok(run_dir) = run_dir.canonicalize() else {
        return Ok(None);
    };
    let Some(stored) = locate(&run_dir, rel) else {
        return Ok(None);
    };
    let (bytes, more) = stored.read_at_most(limit)?;
    if more {
        return Err(format!("{rel} exceeds its {limit}-byte read limit"));
    }
    Ok(Some(bytes))
}

/// Write the original bytes of `stored` to a new file at `destination`.
pub(crate) fn write_original(stored: &StoredFile, destination: &Path) -> Result<(), String> {
    let (bytes, more) = stored.read_at_most(MAX_EXPANDED_BYTES as usize)?;
    if more {
        return Err(format!(
            "{} is larger than a run file can be",
            destination.display()
        ));
    }
    // A compressed copy carries the original's modified time, and so does
    // the file it expands to, as a plain copy would.
    let modified = match stored {
        StoredFile::Gzip(path) => fs::metadata(path).and_then(|meta| meta.modified()).ok(),
        _ => None,
    };
    write_new_file(destination, &bytes, modified)
}

fn write_new_file(
    destination: &Path,
    bytes: &[u8],
    modified: Option<std::time::SystemTime>,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent folder", destination.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Cannot write {}: {error}", destination.display()))?;
    temp.write_all(bytes)
        .and_then(|()| temp.flush())
        .map_err(|error| format!("Cannot write {}: {error}", destination.display()))?;
    if let Some(modified) = modified {
        let _ = temp.as_file().set_modified(modified);
    }
    crate::safety::persist_file(temp, destination)
        .map_err(|error| format!("Cannot write {}: {}", destination.display(), error.error))?;
    Ok(())
}

/// Turn a copy of `run_dir`, or of its `prefix` subtree, back into plain
/// files: each compressed file under `prefix` is expanded in place.
/// `copy_root` must mirror run-relative paths. The record says which `.gz`
/// files compaction made, because a captured folder can hold its own.
pub(crate) fn expand_copy(run_dir: &Path, copy_root: &Path, prefix: &str) -> Result<(), String> {
    let record = record(run_dir);
    for rel in record
        .compressed
        .iter()
        .filter(|rel| rel.starts_with(prefix))
    {
        let compressed = copy_root.join(format!("{rel}{GZIP_SUFFIX}"));
        if fs::symlink_metadata(&compressed).is_err() {
            continue;
        }
        write_original(&StoredFile::Gzip(compressed.clone()), &copy_root.join(rel))?;
        fs::remove_file(&compressed)
            .map_err(|error| format!("Cannot finish expanding {rel}: {error}"))?;
    }
    Ok(())
}

/// Every removed page image of one run, rendered once.
pub(crate) struct RebuiltPages {
    _directory: tempfile::TempDir,
    files: HashMap<String, PathBuf>,
}

impl RebuiltPages {
    /// Render the run's removed pages. `Ok(None)` when it kept its images.
    pub(crate) fn render(run_dir: &Path) -> Result<Option<Self>, String> {
        let Some(pages) = record(run_dir).pages else {
            return Ok(None);
        };
        let run_dir = run_dir
            .canonicalize()
            .map_err(|error| format!("Cannot resolve the run folder: {error}"))?;
        let pdf = run_dir
            .join(&pages.pdf)
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(&run_dir))
            .ok_or("The PDF for this run's page images is missing")?;
        let directory = tempfile::Builder::new()
            .prefix("pipeline-pages-")
            .tempdir()
            .map_err(|error| format!("Cannot prepare page images: {error}"))?;
        let count = u32::try_from(pages.paths.len()).map_err(|_| "Too many page images")?;
        let rendered =
            crate::pipeline::extract::render_retained_pdf_pages(&pdf, directory.path(), count)?;
        let files = rendered
            .names
            .iter()
            .filter_map(|name| {
                let page = page_number(name)?;
                let rel = pages
                    .paths
                    .get(usize::try_from(page).ok()?.checked_sub(1)?)?;
                Some((rel.clone(), directory.path().join(name)))
            })
            .collect();
        Ok(Some(Self {
            _directory: directory,
            files,
        }))
    }

    /// The rendered image for one page path.
    pub(crate) fn path(&self, rel: &str) -> Option<&Path> {
        self.files.get(rel).map(PathBuf::as_path)
    }

    /// Page paths in this set, in no order.
    pub(crate) fn rel_paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

/// The number in a pdftoppm file name such as `page-007.jpg`.
pub(crate) fn page_number(name: &str) -> Option<u32> {
    name.strip_prefix("page-")?
        .strip_suffix(".jpg")?
        .parse()
        .ok()
}

/// Put a run's removed page images back into a copy of it.
pub(crate) fn restore_pages_in_copy(run_dir: &Path, copy_root: &Path) -> Result<(), String> {
    let Some(pages) = RebuiltPages::render(run_dir)? else {
        return Ok(());
    };
    for rel in pages.rel_paths() {
        let source = pages.path(rel).ok_or("A rebuilt page is missing")?;
        let destination = copy_root.join(rel);
        if destination.exists() {
            continue;
        }
        let bytes =
            fs::read(source).map_err(|error| format!("Cannot read a page image: {error}"))?;
        write_new_file(&destination, &bytes, None)?;
    }
    Ok(())
}

/// The physical file behind a path: two hard links share one identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    volume: u64,
    index: u64,
}

/// A file's identity and its number of links, without following a symlink.
pub(crate) fn file_identity(path: &Path) -> Option<(FileIdentity, u64)> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let metadata = fs::symlink_metadata(path).ok()?;
        metadata.is_file().then(|| {
            (
                FileIdentity {
                    volume: metadata.dev(),
                    index: metadata.ino(),
                },
                metadata.nlink(),
            )
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        let file = fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .ok()?;
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        // SAFETY: the handle is open for the call and the structure is plain
        // data that the call fills in.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) } == 0 {
            return None;
        }
        Some((
            FileIdentity {
                volume: u64::from(info.dwVolumeSerialNumber),
                index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            },
            u64::from(info.nNumberOfLinks),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
#[path = "tests/stored.rs"]
mod tests;
