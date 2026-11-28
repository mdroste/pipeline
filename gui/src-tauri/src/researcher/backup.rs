//! What a research backup carries of About you, and how a restore puts it
//! back. The backup format itself belongs to the conversation store's archive;
//! this module only knows which files there are and what each may hold.
//!
//! A restore never replaces anything. A note already on this computer is
//! kept, and so is a sample that is already here: a sample's id comes from
//! its content, so the one here is the same paper.

use super::{
    parse_record, sample_kind, samples, valid_id, write_atomic, Note, Sample, MAX_NOTE_READ_BYTES,
    MAX_RECORD_BYTES, MAX_SAMPLES, MAX_SAMPLE_FILE_BYTES, MAX_SAMPLE_TEXT_BYTES, WRITING,
};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The background note or the style notes.
    Note,
    /// The record that makes a sample exist.
    Record,
    /// A sample's extracted text.
    Text,
    /// The file the researcher added. They still have their own copy, so a
    /// backup short of room leaves these out first.
    Original,
}

/// One file of About you, named by its path inside the folder with `/`
/// between its parts.
#[derive(Debug, Clone)]
pub struct BackupFile {
    pub relative: String,
    pub path: PathBuf,
    pub part: Part,
}

/// A sample and the files that travel with it.
#[derive(Debug, Clone)]
pub struct BackupSample {
    pub title: String,
    pub record: BackupFile,
    pub text: Option<BackupFile>,
    pub original: Option<BackupFile>,
}

#[derive(Debug, Clone, Default)]
pub struct BackupFiles {
    pub notes: Vec<BackupFile>,
    pub samples: Vec<BackupSample>,
}

/// The files Pipeline wrote, and nothing else in the folder: a sync client's
/// conflicted copy stays where it is.
pub fn files(folder: &Path) -> Result<BackupFiles, String> {
    let mut found = BackupFiles::default();
    for note in [Note::About, Note::Style] {
        let path = folder.join(note.file_name());
        if path.is_file() {
            found.notes.push(BackupFile {
                relative: note.file_name().into(),
                path,
                part: Part::Note,
            });
        }
    }
    let directory = folder.join(WRITING);
    for sample in samples(folder)? {
        let file = |name: String, part: Part| BackupFile {
            path: directory.join(&name),
            relative: format!("{WRITING}/{name}"),
            part,
        };
        let existing = |file: BackupFile| file.path.is_file().then_some(file);
        found.samples.push(BackupSample {
            record: file(format!("{}.json", sample.id), Part::Record),
            text: sample
                .words
                .map(|_| file(format!("{}.txt", sample.id), Part::Text)),
            original: existing(file(
                format!("{}.original.{}", sample.id, sample.extension),
                Part::Original,
            )),
            title: sample.title,
        });
    }
    Ok(found)
}

/// What a path inside the folder is, the sample it belongs to, and the most
/// bytes it may hold. `None` for any name Pipeline does not write.
pub fn part(relative: &str) -> Option<(Part, Option<&str>, u64)> {
    if relative == Note::About.file_name() || relative == Note::Style.file_name() {
        return Some((Part::Note, None, MAX_NOTE_READ_BYTES));
    }
    let name = relative.strip_prefix(WRITING)?.strip_prefix('/')?;
    let (id, rest) = name.split_once('.')?;
    if !valid_id(id) {
        return None;
    }
    let (part, limit) = match rest {
        "json" => (Part::Record, MAX_RECORD_BYTES),
        "txt" => (Part::Text, MAX_SAMPLE_TEXT_BYTES as u64),
        _ => {
            sample_kind(rest.strip_prefix("original.")?)?;
            (Part::Original, MAX_SAMPLE_FILE_BYTES)
        }
    };
    Some((part, Some(id), limit))
}

/// The sample a record holds, when it is the record for `id`.
pub fn record(bytes: &[u8], id: &str) -> Result<Sample, String> {
    parse_record(bytes, id)
}

/// What a restore did.
#[derive(Debug, Default)]
pub struct Restored {
    /// Every file written, so a restore that fails later can take them back.
    pub created: Vec<PathBuf>,
    pub notes_restored: usize,
    /// Notes already on this computer, which were kept.
    pub notes_kept: usize,
    pub samples_restored: usize,
    /// Samples already on this computer, which were kept.
    pub samples_kept: usize,
    /// Samples with no room left on the shelf.
    pub samples_over_limit: usize,
}

/// Copies About you from `staged`, a folder laid out like the real one that
/// holds exactly `relatives`. The caller has already checked each name with
/// [`part`], each record with [`record`], and every file's size and hash.
/// On an error, `restored.created` still lists what was written.
pub fn restore(
    folder: &Path,
    staged: &Path,
    relatives: &[String],
    restored: &mut Restored,
) -> Result<(), String> {
    let has = |relative: &str| relatives.iter().any(|name| name == relative);
    for note in [Note::About, Note::Style] {
        let name = note.file_name();
        if !has(name) {
            continue;
        }
        let target = folder.join(name);
        if target.exists() {
            restored.notes_kept += 1;
            continue;
        }
        let mut bytes = Vec::new();
        crate::safety::open_regular_file(&staged.join(name))?
            .take(MAX_NOTE_READ_BYTES)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read {name} from the backup: {error}"))?;
        write_atomic(&target, &bytes)
            .map_err(|error| format!("Could not restore {name}: {error}"))?;
        restored.created.push(target);
        restored.notes_restored += 1;
    }
    let directory = folder.join(WRITING);
    let mut on_shelf = samples(folder)?.len();
    for relative in relatives {
        let Some((Part::Record, Some(id), _)) = part(relative) else {
            continue;
        };
        if directory.join(format!("{id}.json")).exists() {
            restored.samples_kept += 1;
            continue;
        }
        if on_shelf >= MAX_SAMPLES {
            restored.samples_over_limit += 1;
            continue;
        }
        let prefix = format!("{WRITING}/{id}.");
        // The record goes last: a sample exists once its record does.
        let mut parts = relatives
            .iter()
            .filter(|name| name.starts_with(&prefix) && *name != relative)
            .collect::<Vec<_>>();
        parts.push(relative);
        for name in parts {
            let target = folder.join(name);
            copy_atomic(&staged.join(name), &target)
                .map_err(|error| format!("Could not restore your writing: {error}"))?;
            restored.created.push(target);
        }
        restored.samples_restored += 1;
        on_shelf += 1;
    }
    Ok(())
}

fn copy_atomic(source: &Path, target: &Path) -> std::io::Result<()> {
    let directory = target
        .parent()
        .ok_or_else(|| std::io::Error::other("File has no folder"))?;
    fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    std::io::copy(&mut fs::File::open(source)?, &mut temporary)?;
    crate::safety::sync_file(temporary.as_file())?;
    crate::safety::persist_file(temporary, target).map_err(|error| error.error)?;
    Ok(())
}
