//! Open a project file for the viewers. The backend picks the viewer from the
//! file's first bytes as well as its name, so a renamed file cannot reach the
//! wrong renderer. Rendered kinds get a stream token instead of bytes in IPC.
//! Text and PDF files within `MAX_FILE` keep their content hash, which saves and
//! passages check; other files use their stamp and are not read here at all.
use super::file_browser::base;
use super::file_workspace::{editable_source, read_workspace_source, FilePreview, FileReadRequest};
use super::files::SafeRoot;
use super::*;
use crate::file_stream::{self, FileSource, OpenedSource, Presentation};
use std::io::{Seek, SeekFrom};
use std::sync::Arc;

const TEXT_LIMIT: usize = 2 * 1024 * 1024;
const HEAD: u64 = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Format {
    pub kind: &'static str,
    pub mime: &'static str,
}
const fn format(kind: &'static str, mime: &'static str) -> Format {
    Format { kind, mime }
}
const TEXT: Format = format("text", "text/plain");
const BINARY: Format = format("binary", "application/octet-stream");
/// Stata datasets are usually too large to show, so opening one reads only its head.
const DATA: Format = format("data", "application/octet-stream");
/// CSV and TSV viewers page through the file, so opening one reads only its head.
const DELIMITED: Format = format("table", "application/octet-stream");

/// A BMP names the size of its second header. "BM" alone also starts ordinary
/// text, such as a table whose first column is `BMI`.
fn is_bmp(head: &[u8]) -> bool {
    head.len() >= 18
        && head.starts_with(b"BM")
        && head[6..10] == [0; 4]
        && matches!(
            u32::from_le_bytes([head[14], head[15], head[16], head[17]]),
            12 | 40 | 52 | 56 | 108 | 124
        )
}
fn magic(head: &[u8]) -> Option<Format> {
    let riff = |tag: &[u8]| head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == tag;
    if head.starts_with(b"%PDF-") {
        Some(format("pdf", "application/pdf"))
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(format("image", "image/png"))
    } else if head.starts_with(b"\xff\xd8\xff") {
        Some(format("image", "image/jpeg"))
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some(format("image", "image/gif"))
    } else if riff(b"WEBP") {
        Some(format("image", "image/webp"))
    } else if is_bmp(head) {
        Some(format("image", "image/bmp"))
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        Some(format("tiff", "image/tiff"))
    } else if head.len() >= 12 && &head[4..8] == b"ftyp" {
        match &head[8..12] {
            b"avif" | b"avis" => Some(format("image", "image/avif")),
            b"M4A " | b"M4B " => Some(format("audio", "audio/mp4")),
            b"qt  " => Some(format("video", "video/quicktime")),
            b"heic" | b"heix" | b"mif1" | b"msf1" => Some(BINARY),
            _ => Some(format("video", "video/mp4")),
        }
    } else if head.starts_with(b"\x1aE\xdf\xa3") {
        Some(format("video", "video/webm"))
    } else if head.starts_with(b"OggS") {
        Some(format("audio", "audio/ogg"))
    } else if head.starts_with(b"fLaC") {
        Some(format("audio", "audio/flac"))
    } else if head.starts_with(b"ID3") {
        Some(format("audio", "audio/mpeg"))
    } else if riff(b"WAVE") {
        Some(format("audio", "audio/wav"))
    } else {
        None
    }
}
/// The viewer for a file. Signed binary formats win over the name; a name that
/// claims a binary format the bytes do not match falls through to text or binary.
pub(super) fn classify(path: &str, head: &[u8]) -> Format {
    if let Some(found) = magic(head) {
        return found;
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let zip = head.starts_with(b"PK\x03\x04");
    match ext.as_str() {
        "docx" if zip => return format("docx", "application/octet-stream"),
        "xlsx" | "xlsm" if zip => return format("table", "application/octet-stream"),
        "dta"
            if !head.contains(&0)
                || head.starts_with(b"<stata_dta>")
                || head.first() < Some(&120) =>
        {
            return DATA
        }
        // MPEG audio has no required signature beyond its frame sync.
        "mp3" if head.len() >= 2 && head[0] == 0xff && head[1] & 0xe0 == 0xe0 => {
            return format("audio", "audio/mpeg")
        }
        _ => {}
    }
    if head.contains(&0) {
        return BINARY;
    }
    match ext.as_str() {
        "svg" => format("image", "image/svg+xml"),
        "html" | "htm" => format("html", "text/html"),
        "ipynb" => format("notebook", "application/json"),
        "csv" | "tsv" => DELIMITED,
        _ => TEXT,
    }
}

struct WorkspaceSource {
    folder: PathBuf,
    path: String,
}
impl FileSource for WorkspaceSource {
    fn open(&self) -> Result<Option<OpenedSource>, String> {
        let safe = SafeRoot::open(&self.folder).map_err(|error| error.message)?;
        let Some((file, stat)) = safe.open_read(&self.path).map_err(|error| error.message)? else {
            return Ok(None);
        };
        Ok(Some(OpenedSource {
            file,
            stamp: stat.stamp(),
            size: stat.size,
        }))
    }
}
/// Plain UTF-8 stays editable. Anything else is decoded for reading only, and
/// the caller reports that the text is approximate.
fn decode_text(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
    }
}
/// Cut at a character boundary, so a truncated view holds only valid text.
fn utf8_prefix(bytes: &[u8]) -> &[u8] {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes,
        Err(error) if error.error_len().is_none() => &bytes[..error.valid_up_to()],
        Err(_) => bytes,
    }
}
/// A captured PDF reaches the reader over the file scheme, not as base64 in IPC.
/// Reading the revision has just checked its bytes against the recorded hash.
fn captured_preview(store: &Store, request: FileReadRequest) -> WorkbenchResult<FilePreview> {
    let (mut preview, source) = read_workspace_source(store, request)?;
    if let (true, Some((folder, path))) = (preview.kind == "pdf", source) {
        let stat = SafeRoot::open(&folder)?
            .stat(&path)?
            .ok_or_else(|| WorkbenchError::invalid("The captured file is unavailable"))?;
        let token = file_stream::issue(
            Arc::new(WorkspaceSource { folder, path }),
            "application/pdf",
            stat.stamp(),
            Presentation::Bytes,
        )
        .map_err(err)?;
        preview.stream_token = Some(token);
        preview.base64 = None;
    }
    Ok(preview)
}
pub fn open_workspace_file(
    store: &Store,
    request: FileReadRequest,
) -> WorkbenchResult<FilePreview> {
    scope(store, &request.workspace_id)?;
    if request.revision_id.is_some() {
        return captured_preview(store, request);
    }
    if request.checkpoint_id.is_some() && request.path.is_empty() {
        return Err(WorkbenchError::invalid("Choose a file"));
    }
    files::relative(&request.path)?;
    let folder = base(
        store,
        &request.workspace_id,
        request.checkpoint_id.as_deref(),
    )?;
    let (mut file, stat) = SafeRoot::open(&folder)?
        .open_read(&request.path)?
        .ok_or_else(|| {
            let mut error = WorkbenchError::invalid(format!("Missing file: {}", request.path));
            error.code = "file_missing".into();
            error
        })?;
    let mut head = Vec::new();
    Read::by_ref(&mut file)
        .take(HEAD)
        .read_to_end(&mut head)
        .map_err(err)?;
    file.seek(SeekFrom::Start(0)).map_err(err)?;
    let found = classify(&request.path, &head);
    let textual =
        matches!(found.kind, "text" | "html" | "notebook") || found.mime == "image/svg+xml";
    let whole = ((textual || found.kind == "pdf") && stat.size <= MAX_FILE)
        .then(|| {
            let mut bytes = Vec::with_capacity(stat.size as usize);
            Read::by_ref(&mut file)
                .take(MAX_FILE)
                .read_to_end(&mut bytes)
                .map_err(err)?;
            Ok::<_, WorkbenchError>(bytes)
        })
        .transpose()?;
    let hash = whole.as_deref().map_or_else(|| stat.stamp(), hash);
    let (text, truncated, lossy, text_bytes) = if !textual {
        (None, false, false, None)
    } else {
        let mut sample = Vec::new();
        match &whole {
            Some(bytes) => sample.extend_from_slice(&bytes[..bytes.len().min(TEXT_LIMIT)]),
            None => {
                file.seek(SeekFrom::Start(0)).map_err(err)?;
                Read::by_ref(&mut file)
                    .take(TEXT_LIMIT as u64)
                    .read_to_end(&mut sample)
                    .map_err(err)?;
            }
        }
        let truncated = stat.size > TEXT_LIMIT as u64;
        let covered = if truncated {
            utf8_prefix(&sample)
        } else {
            &sample
        };
        let (text, lossy) = decode_text(covered);
        (Some(text), truncated, lossy, Some(covered.len()))
    };
    let presentation = match found.kind {
        "html" => Some(Presentation::Document),
        "pdf" | "image" | "audio" | "video" | "notebook" => Some(Presentation::Bytes),
        _ if truncated => Some(Presentation::Bytes),
        _ => None,
    };
    let streamed = presentation.is_some() && found.mime != "image/svg+xml";
    let stream_token = streamed
        .then(|| {
            file_stream::issue(
                Arc::new(WorkspaceSource {
                    folder: folder.clone(),
                    path: request.path.clone(),
                }),
                found.mime,
                stat.stamp(),
                presentation.unwrap_or(Presentation::Bytes),
            )
            .map_err(err)
        })
        .transpose()?;
    let editable = files::REPLACEMENT
        && !truncated
        && !lossy
        && text.is_some()
        && editable_source(&request.path)
        && !ignored(&request.path, &[]);
    Ok(FilePreview {
        external_path: Some(folder.join(&request.path).to_string_lossy().into_owned()),
        scope_root: Some(folder.to_string_lossy().into_owned()),
        bytes: stat.size as usize,
        path: request.path,
        hash,
        text,
        base64: None,
        mime: found.mime.into(),
        editable,
        truncated,
        kind: found.kind.into(),
        stream_token,
        stamp: Some(stat.stamp()),
        modified_ms: Some(stat.modified_ms),
        approximate_text: lossy,
        text_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_win_over_names() {
        let png = b"\x89PNG\r\n\x1a\n0000";
        assert_eq!(classify("figure.png", png).mime, "image/png");
        assert_eq!(classify("figure.jpg", png).mime, "image/png");
        assert_eq!(classify("notes.txt", b"%PDF-1.7\n").kind, "pdf");
        assert_eq!(classify("paper.pdf", b"not really a pdf").kind, "text");
        assert_eq!(classify("paper.pdf", b"\0\x01\x02").kind, "binary");
        assert_eq!(classify("page.html", b"\0\x01\x02").kind, "binary");
        assert_eq!(classify("scan.tif", b"II*\0rest").kind, "tiff");
        assert_eq!(classify("scan.tif", b"MM\0*rest").kind, "tiff");
    }

    #[test]
    fn text_that_starts_like_a_bitmap_stays_text() {
        assert_eq!(classify("a.csv", b"BMI,age\n1,2\n").kind, "table");
        assert_eq!(classify("n.txt", b"BMW sales\n").kind, "text");
        assert_eq!(classify("n.txt", b"BM").kind, "text");
        // Long enough for a header, with text where its fields would be.
        let long = b"BMI,age,height,weight,income\n";
        assert_eq!(classify("a.txt", long).kind, "text");
        assert_eq!(classify("fake.bmp", long).kind, "text");
        let mut image = b"BM\x46\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0".to_vec();
        image.extend_from_slice(&[0; 36]);
        assert_eq!(classify("figure", &image).mime, "image/bmp");
        assert_eq!(classify("figure.bmp", &image).mime, "image/bmp");
        image[14] = 41;
        assert_ne!(classify("figure", &image).mime, "image/bmp");
    }

    #[test]
    fn office_and_data_formats_need_their_signatures() {
        assert_eq!(classify("a.docx", b"PK\x03\x04").kind, "docx");
        assert_eq!(classify("a.docx", b"plain text").kind, "text");
        assert_eq!(classify("a.xlsx", b"PK\x03\x04").kind, "table");
        assert_eq!(classify("a.xlsx", b"\xd0\xcf\x11\xe0\0").kind, "binary");
        assert_eq!(classify("a.dta", b"<stata_dta><header>").kind, "data");
        assert_eq!(classify("a.csv", b"x,y\n1,2\n").kind, "table");
        assert_eq!(classify("a.tsv", b"x\ty\n1\t2\n").kind, "table");
        assert_eq!(classify("a.csv", b"x,y\0\n").kind, "binary");
    }

    #[test]
    fn media_is_recognized_from_its_container() {
        assert_eq!(classify("a.mp4", b"\0\0\0\x20ftypisom0000").kind, "video");
        assert_eq!(
            classify("a.mov", b"\0\0\0\x14ftypqt  0000").mime,
            "video/quicktime"
        );
        assert_eq!(classify("a.m4a", b"\0\0\0\x20ftypM4A 0000").kind, "audio");
        assert_eq!(classify("a.webm", b"\x1aE\xdf\xa3000").kind, "video");
        assert_eq!(classify("a.wav", b"RIFF0000WAVEfmt ").kind, "audio");
        assert_eq!(classify("a.webp", b"RIFF0000WEBPVP8 ").kind, "image");
        assert_eq!(classify("a.mp3", b"\xff\xfb\x90\x00").kind, "audio");
        assert_eq!(classify("a.heic", b"\0\0\0\x18ftypheic0000").kind, "binary");
    }

    #[test]
    fn text_formats_use_their_names_only_when_bytes_look_like_text() {
        assert_eq!(classify("a.svg", b"<svg></svg>").mime, "image/svg+xml");
        assert_eq!(classify("a.html", b"<html>").kind, "html");
        assert_eq!(classify("a.ipynb", b"{\"cells\":[]}").kind, "notebook");
        assert_eq!(classify("a.py", b"print(1)").kind, "text");
        assert_eq!(classify("LICENSE", b"MIT").kind, "text");
    }

    #[test]
    fn truncation_never_splits_a_character() {
        assert_eq!(utf8_prefix("aé".as_bytes()), "aé".as_bytes());
        assert_eq!(utf8_prefix(&"aé".as_bytes()[..2]), b"a");
        assert_eq!(decode_text(b"caf\xe9"), ("caf\u{fffd}".to_string(), true));
        assert_eq!(decode_text(b"plain"), ("plain".to_string(), false));
    }
}
