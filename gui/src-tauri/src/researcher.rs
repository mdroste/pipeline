//! What the researcher has told Pipeline about themself, shared by every
//! project: a background note, style notes, and finished writing kept as
//! examples. The app calls this **About you**.
//!
//! Everything is a plain file in `researcher/` inside the research data
//! folder, beside the stores rather than in one:
//!
//! ```text
//! researcher/
//!   about.md            the background note, exactly as written
//!   style.md            style notes
//!   writing/
//!     <id>.json         the record that makes a sample exist
//!     <id>.txt          its extracted text, when there is any
//!     <id>.original.<ext>  the file the researcher added
//! ```
//!
//! Files are replaced atomically and there is no shared index, so a sync
//! client can carry the folder between computers: a conflict leaves a second
//! file that Pipeline ignores, never a damaged record. A sample's id comes from
//! its content, so the same paper added twice, or on two computers, is one
//! sample. Nothing is cached. Conversations read the note when a turn starts,
//! and neither the conversation store nor Reviews owns these files.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

pub mod backup;
pub mod commands;
#[cfg(test)]
mod tests;

/// The limit for the background note and for style notes, matching a
/// project's field and conventions.
pub const MAX_NOTE_BYTES: usize = 4 * 1024;
pub const MAX_SAMPLES: usize = 20;
/// The same limit as a document imported into a project.
const MAX_SAMPLE_FILE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_SAMPLE_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// A note edited outside Pipeline may be longer than Settings would save.
/// Read this much so Settings can show it and ask for it to be shortened.
const MAX_NOTE_READ_BYTES: u64 = 64 * 1024;
const MAX_RECORD_BYTES: u64 = 16 * 1024;
const MAX_TITLE_CHARS: usize = 300;
pub const MAX_READ_BYTES: usize = 64 * 1024;
const FOLDER: &str = "researcher";
const WRITING: &str = "writing";
const KINDS: [&str; 4] = ["paper", "grant", "response", "other"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    About,
    Style,
}

impl Note {
    fn file_name(self) -> &'static str {
        match self {
            Note::About => "about.md",
            Note::Style => "style.md",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Note::About => "Background",
            Note::Style => "Style notes",
        }
    }
}

/// One finished piece of the researcher's writing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub year: Option<u16>,
    pub kind: String,
    /// The name of the file the researcher added, for display.
    pub file_name: String,
    /// The extension the original is stored under.
    pub extension: String,
    pub content_hash: String,
    /// Words of extracted text. `None` when no text could be read.
    #[serde(default)]
    pub words: Option<u64>,
    #[serde(default)]
    pub use_as_example: bool,
    pub added_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleUpdate {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub year: Option<u16>,
    pub kind: String,
    pub use_as_example: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub about: String,
    pub style: String,
    pub samples: Vec<Sample>,
    /// Where the files are, for display.
    pub folder: String,
    pub max_note_bytes: usize,
    pub max_samples: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleText {
    pub id: String,
    pub start: usize,
    pub end: usize,
    pub total: usize,
    pub text: String,
}

/// The folder in the active research data folder.
pub fn folder() -> Result<PathBuf, String> {
    Ok(crate::storage::data_root()?.join(FOLDER))
}

/// The folder that goes with a conversation store. The store is the research
/// data folder's `workbench` folder, so this is the same place as [`folder`].
/// A store anywhere else has none: a sibling of an arbitrary folder, such as a
/// temporary folder's parent, is not the researcher's, so it is never read,
/// restored into, or backed up. That includes a development store opened with
/// `PIPELINE_WORKBENCH_DEV_ROOT` under another name.
pub fn folder_beside(store_root: &Path) -> Option<PathBuf> {
    if store_root.file_name()? != "workbench" {
        return None;
    }
    store_root.parent().map(|parent| parent.join(FOLDER))
}

/// The background note as conversations receive it: trimmed, and at most
/// [`MAX_NOTE_BYTES`]. `None` when there is none or it cannot be read, so a
/// missing or unreadable note never stops a conversation.
pub fn background(folder: &Path) -> Option<String> {
    let text = read_note(folder, Note::About).ok()?;
    let text = prefix(text.trim(), MAX_NOTE_BYTES).trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

pub fn load(folder: &Path) -> Result<Profile, String> {
    Ok(Profile {
        about: read_note(folder, Note::About)?,
        style: read_note(folder, Note::Style)?,
        samples: samples(folder)?,
        folder: folder.display().to_string(),
        max_note_bytes: MAX_NOTE_BYTES,
        max_samples: MAX_SAMPLES,
    })
}

/// Saves a note, or removes its file when the text is empty. Returns the text
/// as saved.
pub fn save_note(folder: &Path, note: Note, text: &str) -> Result<String, String> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.contains('\0') {
        return Err(format!("{} must be plain text.", note.label()));
    }
    if text.len() > MAX_NOTE_BYTES {
        return Err(format!(
            "Shorten {} to 4 KiB or less, about 4,000 characters.",
            note.label()
        ));
    }
    let path = folder.join(note.file_name());
    if text.is_empty() {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(String::new()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(format!("Could not clear {}: {error}", note.label())),
        };
    }
    write_atomic(&path, format!("{text}\n").as_bytes())
        .map_err(|error| format!("Could not save {}: {error}", note.label()))?;
    Ok(text.to_string())
}

/// Samples, newest first.
pub fn samples(folder: &Path) -> Result<Vec<Sample>, String> {
    let directory = folder.join(WRITING);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not read your writing: {error}")),
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let id = path.file_stem().and_then(|stem| stem.to_str());
        let is_record = path.extension().and_then(|ext| ext.to_str()) == Some("json");
        // A sync client's conflicted copy, or anything else Pipeline did not
        // name, is not a sample. Neither is a record that cannot be read.
        let Some(id) = id.filter(|id| is_record && valid_id(id)) else {
            continue;
        };
        if let Ok(sample) = read_record(&directory, id) {
            found.push(sample);
        }
    }
    found.sort_by(|a, b| {
        b.added_at
            .cmp(&a.added_at)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(found)
}

/// Copies a finished piece of writing into the folder and reads its text.
/// Adding a file that is already there returns the existing sample.
pub fn add_sample(folder: &Path, source: &Path) -> Result<Sample, String> {
    let source = fs::canonicalize(source)
        .map_err(|error| format!("Could not find {}: {error}", source.display()))?;
    let extension = sample_extension(&source)?;
    let mut file = crate::safety::open_regular_file(&source)?;
    let size = file
        .metadata()
        .map_err(|error| format!("Could not inspect {}: {error}", source.display()))?
        .len();
    if size > MAX_SAMPLE_FILE_BYTES {
        return Err("Files over 200 MB can't be added to Your writing.".into());
    }
    let directory = folder.join(WRITING);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the folder for your writing: {error}"))?;
    // Copy and hash in one pass, so the stored bytes are the hashed bytes.
    let mut copy = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|error| format!("Could not copy the file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
        if count == 0 {
            break;
        }
        copied += count as u64;
        if copied > MAX_SAMPLE_FILE_BYTES {
            return Err("Files over 200 MB can't be added to Your writing.".into());
        }
        hasher.update(&buffer[..count]);
        copy.write_all(&buffer[..count])
            .map_err(|error| format!("Could not copy the file: {error}"))?;
    }
    let content_hash: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let id = content_hash[..16].to_string();
    if let Ok(existing) = read_record(&directory, &id) {
        return Ok(existing);
    }
    if samples(folder)?.len() >= MAX_SAMPLES {
        return Err(format!(
            "Your writing holds up to {MAX_SAMPLES} items. Remove one to add another."
        ));
    }
    crate::safety::sync_file(copy.as_file())
        .map_err(|error| format!("Could not copy the file: {error}"))?;
    let original = directory.join(format!("{id}.original.{extension}"));
    crate::safety::persist_file(copy, &original)
        .map_err(|error| format!("Could not copy the file: {}", error.error))?;
    let text = extract_text(&original, extension);
    let words = match text.as_deref() {
        Some(text) => {
            write_atomic(&directory.join(format!("{id}.txt")), text.as_bytes())
                .map_err(|error| format!("Could not save the paper's text: {error}"))?;
            Some(text.split_whitespace().count() as u64)
        }
        None => None,
    };
    let sample = Sample {
        id,
        title: guessed_title(&source),
        year: None,
        kind: "paper".into(),
        file_name: source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        extension: extension.into(),
        content_hash,
        words,
        use_as_example: words.is_some(),
        added_at: chrono::Utc::now().to_rfc3339(),
    };
    // Written last: a sample exists once its record does.
    write_record(&directory, &sample)?;
    Ok(sample)
}

pub fn update_sample(folder: &Path, update: SampleUpdate) -> Result<Sample, String> {
    let directory = folder.join(WRITING);
    let mut sample = read_record(&directory, &update.id)?;
    let title = update.title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS || title.contains('\0') {
        return Err("Give the item a title of at most 300 characters.".into());
    }
    if !KINDS.contains(&update.kind.as_str()) {
        return Err("Choose Paper, Grant proposal, Referee response, or Other.".into());
    }
    if update
        .year
        .is_some_and(|year| !(1900..=2200).contains(&year))
    {
        return Err("Enter a four-digit year, or leave it empty.".into());
    }
    if update.use_as_example && sample.words.is_none() {
        return Err("This item has no readable text, so it can't be a style example.".into());
    }
    sample.title = title.to_string();
    sample.year = update.year;
    sample.kind = update.kind;
    sample.use_as_example = update.use_as_example;
    write_record(&directory, &sample)?;
    Ok(sample)
}

/// Removes Pipeline's copy. The file the researcher added is untouched.
pub fn remove_sample(folder: &Path, id: &str) -> Result<(), String> {
    let directory = folder.join(WRITING);
    let sample = read_record(&directory, id)?;
    // The record goes first, so a failure part-way leaves no half sample.
    fs::remove_file(directory.join(format!("{id}.json")))
        .map_err(|error| format!("Could not remove the item: {error}"))?;
    for name in [
        format!("{id}.txt"),
        format!("{id}.original.{}", sample.extension),
    ] {
        let _ = fs::remove_file(directory.join(name));
    }
    Ok(())
}

/// A bounded span of a sample's text. `start` and `length` are in bytes and
/// are moved back to the nearest character.
pub fn read_sample(
    folder: &Path,
    id: &str,
    start: usize,
    length: usize,
) -> Result<SampleText, String> {
    let directory = folder.join(WRITING);
    read_record(&directory, id)?;
    let path = directory.join(format!("{id}.txt"));
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&path)
        .map_err(|_| "This item has no readable text.".to_string())?
        .take(MAX_SAMPLE_TEXT_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the item's text: {error}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let total = text.len();
    let start = prefix(&text, start).len();
    let rest = &text[start..];
    let span = prefix(rest, length.clamp(1, MAX_READ_BYTES));
    Ok(SampleText {
        id: id.to_string(),
        start,
        end: start + span.len(),
        total,
        text: span.to_string(),
    })
}

fn read_note(folder: &Path, note: Note) -> Result<String, String> {
    let path = folder.join(note.file_name());
    if !path.exists() {
        return Ok(String::new());
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&path)?
        .take(MAX_NOTE_READ_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {}: {error}", note.label()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .replace("\r\n", "\n")
        .replace(['\r', '\0'], "\n")
        .trim()
        .to_string())
}

fn read_record(directory: &Path, id: &str) -> Result<Sample, String> {
    if !valid_id(id) {
        return Err("Unknown item in Your writing.".into());
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&directory.join(format!("{id}.json")))
        .map_err(|_| "That item is no longer in Your writing.".to_string())?
        .take(MAX_RECORD_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the item: {error}"))?;
    parse_record(&bytes, id)
}

fn parse_record(bytes: &[u8], id: &str) -> Result<Sample, String> {
    let sample: Sample = serde_json::from_slice(bytes)
        .map_err(|error| format!("Could not read the item: {error}"))?;
    // The file name is the identity; a record copied under another name, or
    // edited to point elsewhere, is not trusted.
    if sample.id != id || sample_kind(&sample.extension).is_none() {
        return Err("Could not read the item: its record does not match its file.".into());
    }
    Ok(sample)
}

fn write_record(directory: &Path, sample: &Sample) -> Result<(), String> {
    let mut encoded = serde_json::to_vec_pretty(sample)
        .map_err(|error| format!("Could not save the item: {error}"))?;
    encoded.push(b'\n');
    write_atomic(&directory.join(format!("{}.json", sample.id)), &encoded)
        .map_err(|error| format!("Could not save the item: {error}"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| std::io::Error::other("File has no folder"))?;
    fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    crate::safety::sync_file(temporary.as_file())?;
    crate::safety::persist_file(temporary, path).map_err(|error| error.error)?;
    Ok(())
}

/// Sixteen lowercase hexadecimal characters: nothing that could name a file
/// outside the folder.
fn valid_id(id: &str) -> bool {
    id.len() == 16
        && id
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// How a stored extension is read, or `None` for one Pipeline never writes.
fn sample_kind(extension: &str) -> Option<&'static str> {
    match extension {
        "pdf" => Some("pdf"),
        "docx" => Some("docx"),
        "tex" | "md" | "txt" => Some("text"),
        _ => None,
    }
}

fn sample_extension(path: &Path) -> Result<&'static str, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["pdf", "docx", "tex", "md", "txt"]
        .into_iter()
        .find(|known| *known == extension)
        .ok_or_else(|| "Add a PDF, Word, TeX, Markdown, or text file.".to_string())
}

/// The file's text, or `None` when none can be read: a scanned PDF, for
/// example. The sample is still kept, and says so.
fn extract_text(path: &Path, extension: &str) -> Option<String> {
    let text = match sample_kind(extension)? {
        "pdf" => crate::pipeline::extract::extract_pdftotext(path).ok()?,
        "docx" => crate::document_bundle::extract_docx_text(path).ok()?,
        _ => {
            let bytes = fs::read(path).ok()?;
            if bytes.contains(&0) {
                return None;
            }
            // Older TeX sources are often Latin-1, where each byte is one character.
            String::from_utf8(bytes)
                .unwrap_or_else(|error| error.into_bytes().into_iter().map(char::from).collect())
        }
    };
    let text = text.replace("\r\n", "\n");
    (!text.trim().is_empty() && text.len() <= MAX_SAMPLE_TEXT_BYTES).then_some(text)
}

/// A readable title from a file name: `minimum_wages-final.pdf` becomes
/// "minimum wages final". The researcher can change it.
fn guessed_title(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = stem
        .split(|character: char| character == '_' || character == '-' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let title: String = title.chars().take(MAX_TITLE_CHARS).collect();
    if title.is_empty() {
        "Untitled".into()
    } else {
        title
    }
}

/// The most a passage sent with every message may hold, and how many go.
pub const MAX_PASSAGE_BYTES: usize = 3 * 1024;
pub const MAX_PASSAGES: usize = 3;
/// How far into a paper to look for its introduction.
const PASSAGE_SEARCH_BYTES: u64 = 256 * 1024;

/// What assistants may use of the researcher's writing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Writing {
    /// Style notes, trimmed and within [`MAX_NOTE_BYTES`].
    pub style: Option<String>,
    /// Samples marked as style examples whose text can be read, newest first.
    pub examples: Vec<Sample>,
}

impl Writing {
    pub fn is_empty(&self) -> bool {
        self.style.is_none() && self.examples.is_empty()
    }
}

/// A short stretch of one example, sent as a reference for voice.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Passage {
    pub title: String,
    pub year: Option<u16>,
    pub kind: String,
    pub text: String,
}

/// Style notes and the readable examples. Anything that can't be read counts
/// as absent, so it never stops a conversation.
pub fn writing(folder: &Path) -> Writing {
    let style = read_note(folder, Note::Style).ok().and_then(|text| {
        let text = prefix(text.trim(), MAX_NOTE_BYTES).trim_end();
        (!text.is_empty()).then(|| text.to_string())
    });
    let directory = folder.join(WRITING);
    let examples = samples(folder)
        .unwrap_or_default()
        .into_iter()
        .filter(|sample| {
            sample.use_as_example
                && sample.words.is_some()
                && directory.join(format!("{}.txt", sample.id)).is_file()
        })
        .collect();
    Writing { style, examples }
}

/// Up to [`MAX_PASSAGES`] passages, from examples of `preferred_kind` first and
/// then the rest, each newest first by year and then by when it was added.
pub fn passages(folder: &Path, examples: &[Sample], preferred_kind: &str) -> Vec<Passage> {
    excerpts(
        folder,
        examples,
        preferred_kind,
        MAX_PASSAGES,
        MAX_PASSAGE_BYTES,
    )
}

/// Like [`passages`], with room for `count` excerpts of up to `limit` bytes.
pub fn excerpts(
    folder: &Path,
    examples: &[Sample],
    preferred_kind: &str,
    count: usize,
    limit: usize,
) -> Vec<Passage> {
    let mut ordered = examples.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        (b.kind == preferred_kind)
            .cmp(&(a.kind == preferred_kind))
            .then_with(|| b.year.cmp(&a.year))
            .then_with(|| b.added_at.cmp(&a.added_at))
    });
    let directory = folder.join(WRITING);
    ordered
        .into_iter()
        .filter_map(|sample| {
            let mut bytes = Vec::new();
            crate::safety::open_regular_file(&directory.join(format!("{}.txt", sample.id)))
                .ok()?
                .take(PASSAGE_SEARCH_BYTES)
                .read_to_end(&mut bytes)
                .ok()?;
            let text = passage_within(&String::from_utf8_lossy(&bytes), limit);
            (!text.is_empty()).then(|| Passage {
                title: sample.title.clone(),
                year: sample.year,
                kind: sample.kind.clone(),
                text,
            })
        })
        .take(count)
        .collect()
}

#[cfg(test)]
fn passage(text: &str) -> String {
    passage_within(text, MAX_PASSAGE_BYTES)
}

/// The opening of a paper's introduction when there is one, otherwise of its
/// body, ending at a paragraph or sentence when one is near `limit`.
fn passage_within(text: &str, limit: usize) -> String {
    let start = introduction(text)
        .or_else(|| {
            text.find("\\begin{document}")
                .map(|at| at + "\\begin{document}".len())
        })
        .unwrap_or(0);
    let body = text[start..].trim_start();
    let cut = prefix(body, limit);
    let end = if cut.len() < body.len() {
        let half = cut.len() / 2;
        cut.rfind("\n\n")
            .filter(|&at| at >= half)
            .or_else(|| cut.rfind(". ").map(|at| at + 1).filter(|&at| at >= half))
            .unwrap_or(cut.len())
    } else {
        cut.len()
    };
    cut[..end].trim_end().to_string()
}

/// Where the text after an "Introduction" heading begins: a line such as
/// `1 Introduction`, `I. INTRODUCTION`, `# Introduction`, or
/// `\section{Introduction}`.
fn introduction(text: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        offset += line.len();
        let heading = line.trim().to_ascii_lowercase();
        let heading = heading
            .strip_prefix("\\section*{")
            .or_else(|| heading.strip_prefix("\\section{"))
            .map(|rest| rest.trim_end_matches('}'))
            .unwrap_or(&heading)
            .trim_start_matches('#')
            .trim_start();
        let numbering = heading
            .split_once(' ')
            .filter(|(number, _)| {
                let number = number.trim_end_matches(['.', ')']);
                !number.is_empty()
                    && (number.bytes().all(|byte| byte.is_ascii_digit()) || number == "i")
            })
            .map_or(heading, |(_, rest)| rest.trim_start());
        if numbering == "introduction" {
            return Some(offset);
        }
    }
    None
}

/// As much of the start of `text` as fits in `limit` bytes.
fn prefix(text: &str, limit: usize) -> &str {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
