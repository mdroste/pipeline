//! What a Review's private copy of a selected folder holds.
//!
//! A replication package can hold gigabytes of data, so a folder is copied
//! selectively where a document is copied whole. The copy leaves out hidden
//! entries, generated folders, symlinks, and special files, and whatever does
//! not fit: any file over the per-file cap, then the
//! largest files until the rest meet the file count and total size limits, so
//! code and documentation survive next to large data. Nothing here fails a
//! run: everything left out is recorded beside the copy, where the inventory,
//! the quality notes, and the replication check read it.
//!
//! `commands::reuse_artifacts` performs the copy from this plan.

use super::folder::MAX_INVENTORY_FILES;
use super::*;

/// Folders a copy leaves out. Narrower than the inventory's list: a package's
/// own code often lives in `build`, `dist`, or `target`, and the size limits
/// bound those folders like any other.
const COPY_SKIP_DIRS: &[&str] = &[".git", "node_modules", "__pycache__", ".venv", "venv"];

/// Recorded at the root of a copy that left something out. Hidden, so fan-out
/// and inventories never treat it as package content, and no selected folder
/// can supply one: the copy excludes hidden entries.
pub(crate) const FOLDER_OMISSIONS_FILE: &str = ".pipeline_folder_omissions.json";
/// Bound on that record. It lives inside the copy, so it is reserved from the
/// copy's total size.
const MAX_OMISSIONS_BYTES: usize = 4 * 1024 * 1024;
const MAX_LISTED_FOLDERS: usize = 50;
/// A longer path still counts toward the totals, but is not listed.
const MAX_LISTED_PATH_BYTES: usize = 512;
const MAX_NOTE_EXAMPLES: usize = 5;

/// Limits of one folder copy, with the bytes reserved for its record. Several
/// folders in one run may share a lower file limit, because each copied file
/// takes one entry in the run's manifest.
pub(crate) const MAX_FOLDER_COPY_BYTES: u64 =
    MAX_SCOPED_SOURCE_TOTAL_BYTES - MAX_OMISSIONS_BYTES as u64;
pub(crate) const MAX_FOLDER_COPY_FILE_BYTES: u64 = MAX_SCOPED_SOURCE_FILE_BYTES;
pub(crate) const MAX_FOLDER_COPY_FILES: usize = MAX_INVENTORY_FILES;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OmittedFile {
    pub(crate) path: String,
    pub(crate) bytes: u64,
}

/// Everything a folder copy left out.
#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FolderOmissions {
    /// Files over a size limit, largest first. Bounded, so the record stays
    /// small however many files a folder holds.
    pub(crate) too_large: Vec<OmittedFile>,
    pub(crate) too_large_count: usize,
    pub(crate) too_large_bytes: u64,
    /// Generated folders such as `.git` and `node_modules`.
    pub(crate) folders: Vec<String>,
    pub(crate) folder_count: usize,
    pub(crate) hidden: usize,
    pub(crate) links: usize,
    pub(crate) special: usize,
    pub(crate) unreadable: usize,
    /// The scan hit its limits, so the folder may hold more than was seen.
    pub(crate) incomplete: bool,
    /// This copy's file limit, when it was lower than usual and cost files.
    pub(crate) shared_file_limit: Option<usize>,
}

/// One folder's planned copy: root-relative files, and what stays behind.
#[derive(Debug, Default)]
pub(crate) struct FolderPlan {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) bytes: u64,
    pub(crate) omissions: FolderOmissions,
}

/// Decide what a copy of `root` holds, up to `max_files` files. Reads no file
/// contents. `stop` is polled between entries, so a run's cancellation or a
/// preview's deadline ends a scan of a slow share.
pub(crate) fn plan_folder_copy(
    root: &Path,
    max_files: usize,
    stop: impl Fn() -> bool,
) -> Result<FolderPlan, String> {
    plan_within(root, max_files, MAX_INVENTORY_ENTRIES, stop)
}

/// A directory's listing, cut off at `limit` entries. One directory can hold
/// millions, so this runs before anything collects or sorts them.
fn read_listing<T>(
    listing: impl Iterator<Item = T>,
    limit: usize,
    stop: &impl Fn() -> bool,
) -> Result<Vec<T>, String> {
    let mut entries = Vec::new();
    for entry in listing.take(limit) {
        if stop() {
            return Err("Pipeline cancelled".to_string());
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn plan_within(
    root: &Path,
    max_files: usize,
    max_entries: usize,
    stop: impl Fn() -> bool,
) -> Result<FolderPlan, String> {
    let mut omissions = FolderOmissions::default();
    let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
    let mut stack = vec![PathBuf::new()];
    let mut examined_entries = 0usize;
    let mut examined_dirs = 0usize;
    while let Some(relative) = stack.pop() {
        if stop() {
            return Err("Pipeline cancelled".to_string());
        }
        examined_dirs += 1;
        // Stops one short of the inventory's limit: copying and registering
        // the result also count the copy's own root.
        if examined_dirs >= MAX_INVENTORY_DIRS {
            omissions.incomplete = true;
            break;
        }
        let entries = match fs::read_dir(root.join(&relative)) {
            Ok(entries) => entries,
            // Permission to read the selection itself is a launch problem.
            Err(error) if relative.as_os_str().is_empty() => {
                return Err(format!("Failed to read the selected folder: {error}"));
            }
            Err(_) => {
                omissions.unreadable += 1;
                continue;
            }
        };
        // One past what is left, so a scan that reaches the limit knows it did.
        let room = max_entries.saturating_sub(examined_entries) + 1;
        let mut entries = read_listing(entries.flatten(), room, &stop)?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            if stop() {
                return Err("Pipeline cancelled".to_string());
            }
            examined_entries += 1;
            if examined_entries > max_entries {
                omissions.incomplete = true;
                stack.clear();
                break;
            }
            let name = entry.file_name();
            let path = relative.join(&name);
            let name = name.to_string_lossy();
            let Ok(file_type) = entry.file_type() else {
                omissions.unreadable += 1;
                continue;
            };
            if file_type.is_dir() && COPY_SKIP_DIRS.contains(&name.as_ref()) {
                omissions.record_folder(&path);
            } else if name.starts_with('.') {
                omissions.hidden += 1;
            } else if file_type.is_symlink() {
                omissions.links += 1;
            } else if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                match entry.metadata() {
                    Ok(metadata) => candidates.push((metadata.len(), path)),
                    Err(_) => omissions.unreadable += 1,
                }
            } else {
                omissions.special += 1;
            }
        }
    }

    // Smallest first: a limit then costs a package its largest files, never
    // its code and documentation.
    candidates.sort();
    let mut plan = FolderPlan {
        omissions,
        ..Default::default()
    };
    let mut left_out = Vec::new();
    for (bytes, path) in candidates {
        if bytes <= MAX_FOLDER_COPY_FILE_BYTES
            && plan.files.len() < max_files
            && plan.bytes.saturating_add(bytes) <= MAX_FOLDER_COPY_BYTES
        {
            plan.bytes += bytes;
            plan.files.push(path);
        } else {
            left_out.push((bytes, path));
        }
    }
    for (bytes, path) in left_out.iter().rev() {
        plan.omissions.record_too_large(path, *bytes);
    }
    if plan.omissions.too_large_count > 0 && max_files < MAX_FOLDER_COPY_FILES {
        plan.omissions.shared_file_limit = Some(max_files);
    }
    Ok(plan)
}

fn folder_label(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "This folder".to_string())
}

/// For a folder whose preflight scan ran out of time.
pub(crate) fn slow_scan_notice(root: &Path, max_files: usize) -> String {
    format!(
        "{} took too long to scan, so Pipeline can't yet say whether it all fits. {} The review's log will list anything left out.",
        folder_label(root),
        copy_limits(max_files),
    )
}

impl FolderPlan {
    /// What a researcher should know before starting a Review of this folder:
    /// the files its size limits will leave behind. Generated folders and
    /// hidden entries are expected and stay out of it.
    pub(crate) fn notice(&self, root: &Path) -> Option<String> {
        let omissions = &self.omissions;
        if omissions.too_large_count == 0 && !omissions.incomplete {
            return None;
        }
        let name = folder_label(root);
        if omissions.too_large_count == 0 {
            return Some(format!("{name}: {SCAN_INCOMPLETE}"));
        }
        let total = self.bytes.saturating_add(omissions.too_large_bytes);
        let files = self.files.len() + omissions.too_large_count;
        Some(format!(
            "{name} holds {} in {}. {} {} will be left out. Their names and sizes are listed, but the review can't open them.",
            size_label(total),
            count_label(files, "file"),
            copy_limits(omissions.file_limit()),
            omissions.too_large_label(),
        ))
    }
}

const SCAN_INCOMPLETE: &str =
    "This folder has too many entries to scan completely, so some files may be missing from the review's copy.";

fn copy_limits(max_files: usize) -> String {
    format!(
        "A review's copy holds up to {} files and {} in total, with no file over {}.",
        thousands(max_files),
        size_label(MAX_SCOPED_SOURCE_TOTAL_BYTES),
        size_label(MAX_FOLDER_COPY_FILE_BYTES),
    )
}

fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn count_label(count: usize, noun: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{} {noun}{plural}", thousands(count))
}

fn size_label(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let value = bytes as f64;
    if value >= 1024.0 * MB {
        format!("{:.1} GB", value / (1024.0 * MB))
    } else if value >= MB {
        format!("{:.0} MB", value / MB)
    } else {
        format!("{:.0} KB", (value / 1024.0).ceil())
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl FolderOmissions {
    pub(crate) fn record_folder(&mut self, path: &Path) {
        self.folder_count += 1;
        let path = display_path(path);
        if self.folders.len() < MAX_LISTED_FOLDERS && path.len() <= MAX_LISTED_PATH_BYTES {
            self.folders.push(path);
        }
    }

    pub(crate) fn record_too_large(&mut self, path: &Path, bytes: u64) {
        self.too_large_count += 1;
        self.too_large_bytes = self.too_large_bytes.saturating_add(bytes);
        let path = display_path(path);
        if self.too_large.len() < MAX_INVENTORY_FILES && path.len() <= MAX_LISTED_PATH_BYTES {
            self.too_large.push(OmittedFile { path, bytes });
        }
    }

    fn file_limit(&self) -> usize {
        self.shared_file_limit.unwrap_or(MAX_FOLDER_COPY_FILES)
    }

    fn too_large_label(&self) -> String {
        format!(
            "{} ({})",
            count_label(self.too_large_count, "file"),
            size_label(self.too_large_bytes)
        )
    }

    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Researcher-facing quality notes, one per kind of omission.
    pub(crate) fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.too_large_count > 0 {
            let mut examples = self
                .too_large
                .iter()
                .take(MAX_NOTE_EXAMPLES)
                .map(|file| format!("{} ({})", file.path, size_label(file.bytes)))
                .collect::<Vec<_>>()
                .join(", ");
            let others = self.too_large_count.saturating_sub(MAX_NOTE_EXAMPLES);
            if others > 0 {
                examples.push_str(&format!(", and {} more", thousands(others)));
            }
            notes.push(format!(
                "{} left out of the review's copy of this folder. {} Their names and sizes are listed, but they can't be opened: {examples}.",
                self.too_large_label(),
                copy_limits(self.file_limit()),
            ));
        }
        let mut skipped = self.folders.clone();
        if self.folder_count > self.folders.len() {
            skipped.push(count_label(
                self.folder_count - self.folders.len(),
                "more generated folder",
            ));
        }
        for (count, noun) in [
            (self.hidden, "hidden item"),
            (self.links, "symbolic link"),
            (self.special, "special file"),
        ] {
            if count > 0 {
                skipped.push(count_label(count, noun));
            }
        }
        if !skipped.is_empty() {
            notes.push(format!(
                "Also left out of the review's copy of this folder: {}.",
                skipped.join(", ")
            ));
        }
        if self.unreadable > 0 {
            notes.push(format!(
                "{} in this folder could not be read and are not in the review's copy.",
                count_label(self.unreadable, "item")
            ));
        }
        if self.incomplete {
            notes.push(SCAN_INCOMPLETE.to_string());
        }
        notes
    }

    /// The part of an inventory that names files the Review can't open.
    pub(crate) fn inventory_section(&self) -> String {
        if self.too_large.is_empty() {
            return String::new();
        }
        let mut text = String::from(
            "\n## Left out of this copy\n\nThese files are in the selected folder, but they are not under the root and cannot be opened.\n\n| File | Bytes |\n|---|---|\n",
        );
        for file in &self.too_large {
            text.push_str(&format!("| {} | {} |\n", file.path, file.bytes));
        }
        text
    }

    /// Record this beside the copied files. A complete copy has no record.
    pub(crate) fn save(&self, copy_root: &Path) -> Result<(), String> {
        if self.is_empty() {
            return Ok(());
        }
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        fs::write(copy_root.join(FOLDER_OMISSIONS_FILE), bytes)
            .map_err(|error| format!("Could not record the files left out of the copy: {error}"))
    }
}

/// What the copy at `root` left out. Empty for a complete copy.
pub(crate) fn folder_omissions(root: &Path) -> FolderOmissions {
    read_utf8_capped(&root.join(FOLDER_OMISSIONS_FILE), MAX_OMISSIONS_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sparse file: its size is recorded, but it uses no disk.
    fn sparse(path: &Path, bytes: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(path).unwrap().set_len(bytes).unwrap();
    }

    const MB: u64 = 1024 * 1024;

    #[test]
    fn a_large_folder_keeps_its_small_files_and_names_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("README.md"), "read me").unwrap();
        fs::write(dir.path().join("main.do"), "do clean").unwrap();
        // Each fits the per-file cap; together they exceed the total limit.
        for index in 0..20 {
            sparse(
                &dir.path().join(format!("data/part{index:02}.csv")),
                30 * MB,
            );
        }
        sparse(&dir.path().join("data/panel.dta"), 3 * 1024 * MB);

        let plan = plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || false).unwrap();

        assert!(plan.files.contains(&PathBuf::from("README.md")));
        assert!(plan.files.contains(&PathBuf::from("main.do")));
        assert!(plan.bytes <= MAX_FOLDER_COPY_BYTES);
        assert_eq!(plan.files.len(), 10, "{:?}", plan.files);
        let omissions = &plan.omissions;
        assert_eq!(omissions.too_large_count, 13);
        assert_eq!(omissions.too_large[0].path, "data/panel.dta");
        let notice = plan.notice(Path::new("/work/package")).unwrap();
        assert!(
            notice.starts_with("package holds 3.6 GB in 23 files."),
            "{notice}"
        );
        assert!(
            notice.contains("13 files (3.4 GB) will be left out"),
            "{notice}"
        );
        assert!(omissions.notes()[0].contains("data/panel.dta (3.0 GB)"));
    }

    #[test]
    fn a_folder_that_fits_needs_no_notice() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.do"), "do clean").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), "x").unwrap();

        let plan = plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || false).unwrap();

        assert_eq!(plan.files, vec![PathBuf::from("main.do")]);
        assert!(plan.notice(dir.path()).is_none());
        assert_eq!(plan.omissions.folders, vec![".git".to_string()]);
    }

    #[test]
    fn a_package_keeps_the_code_in_its_build_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("build/code")).unwrap();
        fs::write(dir.path().join("build/code/clean.do"), "do clean").unwrap();
        fs::create_dir_all(dir.path().join("node_modules/left")).unwrap();
        fs::write(dir.path().join("node_modules/left/index.js"), "x").unwrap();

        let plan = plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || false).unwrap();

        assert_eq!(plan.files, vec![PathBuf::from("build/code/clean.do")]);
        assert_eq!(plan.omissions.folders, vec!["node_modules".to_string()]);
    }

    #[test]
    fn the_copy_never_holds_more_files_than_an_inventory_lists() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..(MAX_INVENTORY_FILES + 3) {
            fs::write(dir.path().join(format!("{index:05}.txt")), "x").unwrap();
        }

        let plan = plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || false).unwrap();

        assert_eq!(plan.files.len(), MAX_INVENTORY_FILES);
        assert_eq!(plan.omissions.too_large_count, 3);
    }

    #[test]
    fn a_stopped_plan_reports_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || true).unwrap_err(),
            "Pipeline cancelled"
        );
    }

    #[test]
    fn a_stop_ends_the_scan_inside_one_long_directory() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..50 {
            fs::write(dir.path().join(format!("{index:02}.txt")), "x").unwrap();
        }
        let polls = std::cell::Cell::new(0);

        let stopped = plan_folder_copy(dir.path(), MAX_FOLDER_COPY_FILES, || {
            polls.set(polls.get() + 1);
            polls.get() > 10
        });

        assert_eq!(stopped.unwrap_err(), "Pipeline cancelled");
        assert_eq!(polls.get(), 11);
    }

    #[test]
    fn a_listing_is_cut_off_before_it_is_collected() {
        let never_stop = || false;
        let endless = std::iter::repeat(0);
        assert_eq!(read_listing(endless, 7, &never_stop).unwrap().len(), 7);
    }

    #[test]
    fn a_scan_that_reaches_its_entry_limit_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..20 {
            fs::write(dir.path().join(format!("{index:02}.txt")), "x").unwrap();
        }

        let plan = plan_within(dir.path(), MAX_FOLDER_COPY_FILES, 5, || false).unwrap();
        assert_eq!(plan.files.len(), 5);
        assert!(plan.omissions.incomplete);
        assert!(plan
            .notice(dir.path())
            .unwrap()
            .contains("too many entries"));

        let exact = plan_within(dir.path(), MAX_FOLDER_COPY_FILES, 20, || false).unwrap();
        assert_eq!(exact.files.len(), 20);
        assert!(!exact.omissions.incomplete);
    }

    #[test]
    fn a_shared_file_limit_is_the_one_the_researcher_reads() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..5 {
            fs::write(dir.path().join(format!("{index}.txt")), "x").unwrap();
        }

        let plan = plan_folder_copy(dir.path(), 3, || false).unwrap();

        assert_eq!(plan.files.len(), 3);
        assert!(plan.notice(dir.path()).unwrap().contains("up to 3 files"));
        assert!(plan.omissions.notes()[0].contains("up to 3 files"));
        let fits = plan_folder_copy(dir.path(), 5, || false).unwrap();
        assert!(fits.omissions.is_empty(), "a copy that fit needs no record");
    }

    #[test]
    fn a_folder_that_cannot_be_read_stops_the_launch() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        assert!(plan_folder_copy(&missing, MAX_FOLDER_COPY_FILES, || false)
            .unwrap_err()
            .contains("selected folder"));
    }

    #[test]
    fn labels_read_naturally() {
        assert_eq!(thousands(2000), "2,000");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(size_label(100), "1 KB");
        assert_eq!(size_label(32 * MB), "32 MB");
        assert_eq!(size_label(3 * 1024 * MB), "3.0 GB");
    }
}
