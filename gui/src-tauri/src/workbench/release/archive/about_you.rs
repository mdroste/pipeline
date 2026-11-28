//! Version-four archive payload: About you (`crate::researcher`), which is the
//! researcher's background note, style notes, and past writing. It is a folder
//! of plain files beside the store, so it travels as listed files like
//! conversation working files do, under `about-you/`.
//!
//! Notes, and each sample's record with its text, go in before conversation
//! files. The files the researcher added go in last, smallest first, while
//! there is room: they still have their own copies. What does not fit or
//! cannot be read is left out and reported, and never fails the backup.
//!
//! A restore replaces nothing. Notes and samples already on this computer are
//! kept, and a restore that fails later takes back what this one wrote.
use super::conversation_files::{copy_within, manifest_cost, Budget};
use super::format::{BlobManifest, MAX_ENTRY_BYTES};
use super::hash_reader;
use crate::researcher::backup::{self, BackupFile, Part, Restored};
use crate::researcher::{Sample, MAX_SAMPLES};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

const PREFIX: &str = "about-you";

fn io(error: std::io::Error) -> WorkbenchError {
    WorkbenchError::storage("About you archive failed", error)
}

fn invalid() -> WorkbenchError {
    WorkbenchError::invalid("Invalid or duplicate About you entry in the archive")
}

fn papers(count: usize) -> String {
    if count == 1 {
        "1 paper".into()
    } else {
        format!("{count} papers")
    }
}

/// What one export copied and what it left out.
#[derive(Default)]
pub(super) struct Staged {
    pub(super) files: Vec<BlobManifest>,
    /// Each staged sample's record, with its original still to be staged.
    pending: Vec<(BlobManifest, BackupFile)>,
    unreadable: bool,
    notes_left_out: usize,
    samples_left_out: usize,
    originals_left_out: usize,
}

impl Staged {
    /// What to tell the researcher, or `None` when there is no About you.
    pub(super) fn note(&self) -> Option<String> {
        if self.unreadable {
            return Some("About you could not be read, so it was left out.".into());
        }
        if self.files.is_empty() && self.notes_left_out + self.samples_left_out == 0 {
            return None;
        }
        let mut note = String::from("About you is included.");
        if self.notes_left_out > 0 {
            note.push_str(" Your background or style notes could not be added and were left out.");
        }
        if self.samples_left_out > 0 {
            note.push_str(&format!(
                " {} in Your writing could not be added and were left out.",
                papers(self.samples_left_out)
            ));
        }
        if self.originals_left_out > 0 {
            note.push_str(&format!(
                " The original files of {} were left out to keep the backup within its size limit; their text is included.",
                papers(self.originals_left_out)
            ));
        }
        Some(note)
    }
}

/// Stages `unit` together or not at all, and takes what it used from `budget`.
fn stage_unit(
    unit: &[&BackupFile],
    staging: &Path,
    budget: &mut Budget,
) -> WorkbenchResult<Option<Vec<BlobManifest>>> {
    let mut staged = Vec::<BlobManifest>::new();
    let mut bytes = 0u64;
    let mut manifest_bytes = 0u64;
    let mut fits = unit.len() <= budget.entries;
    for file in unit {
        if !fits {
            break;
        }
        let Some((_, _, limit)) = backup::part(&file.relative) else {
            fits = false;
            break;
        };
        let archive_path = format!("{PREFIX}/{}", file.relative);
        let target = staging.join(&archive_path);
        let room = limit.min(MAX_ENTRY_BYTES).min(budget.bytes - bytes);
        let Some(count) = copy_within(&file.path, &target, room) else {
            fits = false;
            break;
        };
        let entry = BlobManifest {
            archive_path,
            original_reference: file.relative.clone(),
            size_bytes: count,
            sha256: hash_reader(File::open(&target).map_err(io)?)?,
        };
        bytes += count;
        manifest_bytes = manifest_bytes.saturating_add(manifest_cost(&entry));
        staged.push(entry);
        fits = manifest_bytes <= budget.manifest_bytes;
    }
    if !fits {
        for entry in &staged {
            let _ = fs::remove_file(staging.join(&entry.archive_path));
        }
        return Ok(None);
    }
    budget.entries -= staged.len();
    budget.bytes -= bytes;
    budget.manifest_bytes -= manifest_bytes;
    Ok(Some(staged))
}

fn unstage(entries: &[BlobManifest], staging: &Path, budget: &mut Budget) {
    for entry in entries {
        let _ = fs::remove_file(staging.join(&entry.archive_path));
        budget.entries += 1;
        budget.bytes += entry.size_bytes;
        budget.manifest_bytes += manifest_cost(entry);
    }
}

/// Stages the notes and each sample's record and text. Call
/// [`stage_originals`] once everything else has had its share of the budget.
pub(super) fn stage(store: &Store, staging: &Path, budget: &mut Budget) -> WorkbenchResult<Staged> {
    let mut staged = Staged::default();
    let Some(folder) = crate::researcher::folder_beside(store.root_path()) else {
        return Ok(staged);
    };
    let Ok(found) = backup::files(&folder) else {
        staged.unreadable = true;
        return Ok(staged);
    };
    for note in &found.notes {
        match stage_unit(&[note], staging, budget)? {
            Some(entries) => staged.files.extend(entries),
            None => staged.notes_left_out += 1,
        }
    }
    for sample in found.samples {
        let mut unit = vec![&sample.record];
        unit.extend(&sample.text);
        let Some(entries) = stage_unit(&unit, staging, budget)? else {
            staged.samples_left_out += 1;
            continue;
        };
        // A record that changed while it was copied would make a backup that
        // restore refuses, so it is held to the same checks here.
        if check(&entries, staging).is_err() {
            unstage(&entries, staging, budget);
            staged.samples_left_out += 1;
            continue;
        }
        if let Some(original) = sample.original {
            staged.pending.push((entries[0].clone(), original));
        }
        staged.files.extend(entries);
    }
    Ok(staged)
}

/// Stages the files the researcher added, smallest first, while they fit.
pub(super) fn stage_originals(
    staged: &mut Staged,
    staging: &Path,
    budget: &mut Budget,
) -> WorkbenchResult<()> {
    let mut pending = std::mem::take(&mut staged.pending)
        .into_iter()
        .map(|(record, original)| {
            let size = fs::metadata(&original.path).map_or(u64::MAX, |metadata| metadata.len());
            (size, record, original)
        })
        .collect::<Vec<_>>();
    pending.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.relative.cmp(&b.2.relative)));
    for (_, record, original) in pending {
        let Some(entries) = stage_unit(&[&original], staging, budget)? else {
            staged.originals_left_out += 1;
            continue;
        };
        let mut pair = vec![record];
        pair.extend(entries);
        if belongs(&pair[1], &records(&pair[..1], staging)?) {
            staged.files.push(pair.remove(1));
        } else {
            unstage(&pair[1..], staging, budget);
            staged.originals_left_out += 1;
        }
    }
    Ok(())
}

/// The samples whose records are among `entries`, by id.
fn records(entries: &[BlobManifest], staging: &Path) -> WorkbenchResult<HashMap<String, Sample>> {
    let mut found = HashMap::new();
    for entry in entries {
        let Some((Part::Record, Some(id), _)) = backup::part(&entry.original_reference) else {
            continue;
        };
        let bytes = fs::read(staging.join(&entry.archive_path)).map_err(io)?;
        let sample = backup::record(&bytes, id).map_err(|_| invalid())?;
        found.insert(id.to_string(), sample);
    }
    Ok(found)
}

/// Whether a sample's text or original goes with a record in `records`.
fn belongs(entry: &BlobManifest, records: &HashMap<String, Sample>) -> bool {
    match backup::part(&entry.original_reference) {
        Some((Part::Text, Some(id), _)) => {
            records.get(id).is_some_and(|sample| sample.words.is_some())
        }
        Some((Part::Original, Some(id), _)) => records.get(id).is_some_and(|sample| {
            entry.original_reference == format!("writing/{id}.original.{}", sample.extension)
        }),
        Some(_) => true,
        None => false,
    }
}

/// Every text and original has its record, and every record that counts
/// words has its text.
fn check(entries: &[BlobManifest], staging: &Path) -> WorkbenchResult<()> {
    let records = records(entries, staging)?;
    let texts = entries
        .iter()
        .filter_map(|entry| match backup::part(&entry.original_reference) {
            Some((Part::Text, Some(id), _)) => Some(id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    if records.len() > MAX_SAMPLES
        || !entries.iter().all(|entry| belongs(entry, &records))
        || records
            .iter()
            .any(|(id, sample)| sample.words.is_some() && !texts.contains(id.as_str()))
    {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn validate(
    files: &[BlobManifest],
    staging: &Path,
    expected: &mut HashSet<String>,
) -> WorkbenchResult<()> {
    for file in files {
        let Some((_, _, limit)) = backup::part(&file.original_reference) else {
            return Err(invalid());
        };
        if file.archive_path != format!("{PREFIX}/{}", file.original_reference)
            || file.size_bytes > limit
            || !expected.insert(file.archive_path.clone())
        {
            return Err(invalid());
        }
        let path = staging.join(&file.archive_path);
        if fs::metadata(&path).map_err(io)?.len() != file.size_bytes
            || hash_reader(File::open(path).map_err(io)?)? != file.sha256
        {
            return Err(WorkbenchError::invalid(
                "About you file failed its size or hash check",
            ));
        }
    }
    check(files, staging)
}

/// What a restore wrote. Dropped without `commit`, it takes those files back
/// and leaves everything that was already there.
pub(super) struct Installed {
    folder: Option<PathBuf>,
    created: Vec<PathBuf>,
    /// What to tell the researcher, or `None` when the backup had no About you.
    pub(super) note: Option<String>,
}

impl Installed {
    pub(super) fn commit(&mut self) {
        self.created.clear();
        self.folder = None;
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        for path in &self.created {
            let _ = fs::remove_file(path);
        }
        // Only folders this restore left empty go; `remove_dir` refuses others.
        if let Some(folder) = &self.folder {
            let _ = fs::remove_dir(folder.join("writing"));
            let _ = fs::remove_dir(folder);
        }
    }
}

pub(super) fn install(
    store: &Store,
    files: &[BlobManifest],
    staging: &Path,
) -> WorkbenchResult<Installed> {
    if files.is_empty() {
        return Ok(Installed {
            folder: None,
            created: Vec::new(),
            note: None,
        });
    }
    let folder = crate::researcher::folder_beside(store.root_path())
        .ok_or_else(|| WorkbenchError::invalid("This store has no place for About you"))?;
    let relatives = files
        .iter()
        .map(|file| file.original_reference.clone())
        .collect::<Vec<_>>();
    let mut restored = Restored::default();
    let result = backup::restore(&folder, &staging.join(PREFIX), &relatives, &mut restored);
    let mut installed = Installed {
        folder: Some(folder),
        created: std::mem::take(&mut restored.created),
        note: None,
    };
    result.map_err(|error| WorkbenchError::storage("Failed to restore About you", error))?;
    let mut note = String::from(if restored.notes_restored + restored.samples_restored > 0 {
        "About you was restored."
    } else {
        "About you on this computer was left as it is."
    });
    if restored.notes_kept > 0 {
        note.push_str(" Your background or style notes on this computer were kept, not replaced.");
    }
    if restored.samples_kept > 0 {
        note.push_str(&format!(
            " {} already in Your writing stayed as they are.",
            papers(restored.samples_kept)
        ));
    }
    if restored.samples_over_limit > 0 {
        note.push_str(&format!(
            " {} could not be restored because Your writing holds up to {MAX_SAMPLES}.",
            papers(restored.samples_over_limit)
        ));
    }
    installed.note = Some(note);
    Ok(installed)
}

#[cfg(test)]
#[path = "about_you_tests.rs"]
mod tests;
