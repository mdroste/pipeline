//! Conversions for formats a webview cannot show: paged tables, a PNG of a
//! TIFF, and the reading text of a DOCX. These run off the database workers and
//! bound every read, because the input is whatever sits in the project folder.
use super::files::SafeRoot;
use super::*;
use crate::workbench::data::{self, FileTable, TableResume};
use std::io::Seek;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::ColorType;

const MAX_TIFF_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TIFF_PIXELS: u64 = 100_000_000;
/// Viewers show at most this many pixels on the long side.
const MAX_TIFF_SIDE: u32 = 4096;
const MAX_TIFF_PAGES: u32 = 1_000;
const MAX_DOCX_TEXT: usize = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileTableRequest {
    pub workspace_id: String,
    pub path: String,
    pub checkpoint_id: Option<String>,
    pub sheet: Option<String>,
    pub start: usize,
    /// Where Load more continues a CSV or TSV file.
    pub resume: Option<TableResume>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreview {
    pub base64: String,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub pages: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentText {
    pub text: String,
    pub truncated: bool,
}

fn opened(folder: &Path, path: &str) -> WorkbenchResult<(fs::File, u64)> {
    let (file, stat) = SafeRoot::open(folder)?
        .open_read(path)?
        .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {path}")))?;
    Ok((file, stat.size))
}
pub fn file_table(
    folder: &Path,
    path: &str,
    sheet: Option<&str>,
    start: usize,
    resume: Option<&TableResume>,
) -> WorkbenchResult<FileTable> {
    let extension = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "csv" | "tsv") {
        let (file, stat) = SafeRoot::open(folder)?
            .open_read(path)?
            .ok_or_else(|| WorkbenchError::invalid(format!("Missing file: {path}")))?;
        return data::delimited_table(file, &extension, resume, start, stat.stamp());
    }
    let (file, size) = opened(folder, path)?;
    data::file_table(file, size, &extension, sheet, start)
}
pub fn docx_text(folder: &Path, path: &str) -> WorkbenchResult<DocumentText> {
    let bytes = SafeRoot::open(folder)?.read(path)?;
    let mut copy = tempfile::Builder::new()
        .suffix(".docx")
        .tempfile()
        .map_err(err)?;
    copy.write_all(&bytes).map_err(err)?;
    copy.flush().map_err(err)?;
    let mut text =
        crate::document_bundle::extract_docx_text(copy.path()).map_err(WorkbenchError::invalid)?;
    let truncated = text.len() > MAX_DOCX_TEXT;
    if truncated {
        let mut end = MAX_DOCX_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    Ok(DocumentText { text, truncated })
}
fn unsupported(message: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::invalid(format!(
        "This TIFF cannot be shown here: {message}. Open it externally instead."
    ))
}
/// One sample scaled to 8 bits.
fn sample(values: &DecodingResult, index: usize) -> Option<u8> {
    match values {
        DecodingResult::U8(data) => data.get(index).copied(),
        DecodingResult::U16(data) => data.get(index).map(|v| (v >> 8) as u8),
        DecodingResult::F32(data) => data
            .get(index)
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
        _ => None,
    }
}
/// Decode the first page to a PNG, reduced to `MAX_TIFF_SIDE` by even sampling.
fn encode_tiff(reader: impl Read + Seek) -> WorkbenchResult<ImagePreview> {
    let mut limits = Limits::default();
    limits.decoding_buffer_size = (MAX_TIFF_PIXELS as usize) * 8;
    let mut decoder = Decoder::new(reader)
        .map_err(unsupported)?
        .with_limits(limits);
    let (width, height) = decoder.dimensions().map_err(unsupported)?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_TIFF_PIXELS {
        return Err(unsupported("it exceeds the 100-megapixel limit"));
    }
    let color = decoder.colortype().map_err(unsupported)?;
    let channels = match color {
        ColorType::Gray(_) => 1,
        ColorType::GrayA(_) => 2,
        ColorType::RGB(_) => 3,
        ColorType::RGBA(_) | ColorType::CMYK(_) => 4,
        other => return Err(unsupported(format!("{other:?} color is not supported"))),
    };
    let bits = match color {
        ColorType::Gray(n)
        | ColorType::GrayA(n)
        | ColorType::RGB(n)
        | ColorType::RGBA(n)
        | ColorType::CMYK(n) => n,
        _ => 0,
    };
    if !matches!(bits, 8 | 16 | 32) {
        return Err(unsupported(format!("{bits}-bit samples are not supported")));
    }
    let values = decoder.read_image().map_err(unsupported)?;
    let mut pages = 1;
    while pages < MAX_TIFF_PAGES && decoder.more_images() {
        if decoder.next_image().is_err() {
            break;
        }
        pages += 1;
    }
    let step = width.max(height).div_ceil(MAX_TIFF_SIDE).max(1);
    let (out_width, out_height) = (width.div_ceil(step), height.div_ceil(step));
    let mut pixels = Vec::with_capacity(out_width as usize * out_height as usize * 4);
    for y in (0..height).step_by(step as usize) {
        for x in (0..width).step_by(step as usize) {
            let at = (y as usize * width as usize + x as usize) * channels;
            let value = |channel: usize| {
                sample(&values, at + channel).ok_or_else(|| unsupported("its data is incomplete"))
            };
            let rgba = match color {
                ColorType::Gray(_) => [value(0)?, value(0)?, value(0)?, 255],
                ColorType::GrayA(_) => [value(0)?, value(0)?, value(0)?, value(1)?],
                ColorType::RGB(_) => [value(0)?, value(1)?, value(2)?, 255],
                ColorType::RGBA(_) => [value(0)?, value(1)?, value(2)?, value(3)?],
                _ => {
                    let black = 255 - u16::from(value(3)?);
                    let ink = |c: u8| ((255 - u16::from(c)) * black / 255) as u8;
                    [ink(value(0)?), ink(value(1)?), ink(value(2)?), 255]
                }
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, out_width, out_height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(err)?;
        writer.write_image_data(&pixels).map_err(err)?;
        writer.finish().map_err(err)?;
    }
    Ok(ImagePreview {
        base64: base64::engine::general_purpose::STANDARD.encode(&encoded),
        mime: "image/png",
        width: out_width,
        height: out_height,
        source_width: width,
        source_height: height,
        pages,
    })
}
pub fn tiff_preview(folder: &Path, path: &str) -> WorkbenchResult<ImagePreview> {
    let (file, size) = opened(folder, path)?;
    if size > MAX_TIFF_BYTES {
        return Err(unsupported("it is larger than 256 MB"));
    }
    encode_tiff(std::io::BufReader::new(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A single-strip, uncompressed little-endian TIFF.
    fn tiff(width: u32, height: u32, samples: u16, pixels: &[u8]) -> Vec<u8> {
        let photometric: u32 = if samples == 1 { 1 } else { 2 };
        let bits_offset = 8 + 2 + 9 * 12 + 4;
        let data_offset = bits_offset + 2 * u32::from(samples);
        let bits_value = if samples == 1 { 8 } else { bits_offset };
        let entries: [(u16, u16, u32, u32); 9] = [
            (256, 4, 1, width),
            (257, 4, 1, height),
            (258, 3, u32::from(samples), bits_value),
            (259, 3, 1, 1),
            (262, 3, 1, photometric),
            (273, 4, 1, data_offset),
            (277, 3, 1, u32::from(samples)),
            (278, 4, 1, height),
            (279, 4, 1, pixels.len() as u32),
        ];
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&9u16.to_le_bytes());
        for (tag, kind, count, value) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        for _ in 0..samples {
            out.extend_from_slice(&8u16.to_le_bytes());
        }
        out.extend_from_slice(pixels);
        out
    }

    #[test]
    fn csv_files_page_from_the_project_folder_and_refuse_a_stale_offset() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("panel.csv");
        let rows: String = (0..120).map(|row| format!("{row},x\n")).collect();
        fs::write(&file, format!("id,name\n{rows}")).unwrap();
        let first = file_table(folder.path(), "panel.csv", None, 0, None).unwrap();
        assert_eq!(first.rows.len(), 100);
        let preview = first.preview.as_ref().unwrap();
        let resume = TableResume {
            offset: preview.next_offset.unwrap(),
            stamp: preview.stamp.clone(),
        };
        let second = file_table(folder.path(), "panel.csv", None, 100, Some(&resume)).unwrap();
        assert_eq!(second.rows.len(), 20);
        assert_eq!(second.rows[0][0].as_deref(), Some("100"));
        assert_eq!(second.total_rows, 120);
        fs::write(&file, "id\n1\n").unwrap();
        let error = file_table(folder.path(), "panel.csv", None, 100, Some(&resume)).unwrap_err();
        assert_eq!(error.code, "file_changed");
    }

    #[test]
    fn table_requests_may_carry_a_resume_point() {
        let request = |resume| {
            serde_json::from_value::<FileTableRequest>(serde_json::json!({
                "workspaceId": "w", "path": "a.csv", "checkpointId": null,
                "sheet": null, "start": 100, "resume": resume,
            }))
        };
        assert!(request(serde_json::Value::Null).unwrap().resume.is_none());
        let resume = serde_json::json!({ "offset": 700, "stamp": "1:2:3:4" });
        assert_eq!(request(resume).unwrap().resume.unwrap().offset, 700);
    }

    #[test]
    fn rgb_and_gray_tiffs_become_pngs() {
        let rgb = tiff(2, 1, 3, &[255, 0, 0, 0, 0, 255]);
        let preview = encode_tiff(Cursor::new(rgb)).unwrap();
        assert_eq!((preview.width, preview.height), (2, 1));
        assert_eq!(preview.pages, 1);
        let png = base64::engine::general_purpose::STANDARD
            .decode(preview.base64)
            .unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let gray = tiff(1, 2, 1, &[10, 200]);
        assert_eq!(encode_tiff(Cursor::new(gray)).unwrap().height, 2);
    }

    #[test]
    fn large_images_are_reduced_and_oversized_ones_refused() {
        let pixels = vec![128u8; 5000 * 3];
        let preview = encode_tiff(Cursor::new(tiff(5000, 1, 3, &pixels))).unwrap();
        assert_eq!(preview.width, 2500);
        assert_eq!(preview.source_width, 5000);
        let error = encode_tiff(Cursor::new(tiff(20_000, 20_000, 3, &[0; 16]))).unwrap_err();
        assert!(error.message.contains("100-megapixel"));
    }

    #[test]
    fn corrupt_tiffs_fail_with_a_message() {
        assert!(encode_tiff(Cursor::new(b"II*\0garbage".to_vec())).is_err());
        let mut short = tiff(100, 100, 3, &[0; 30]);
        short.truncate(short.len() - 10);
        assert!(encode_tiff(Cursor::new(short)).is_err());
    }
}
