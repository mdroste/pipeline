//! Versioned, source-neutral representation of an input document.
//!
//! `ExtractionResult::text` remains the compatibility view used by older
//! profiles. `DocumentBundle` is the durable source of truth for new runs: it
//! preserves page images, semantic blocks, equations, tables, figures, and
//! provenance without forcing every source format through Markdown.

use crate::models::{ExtractionResult, OrientationMap};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

pub const DOCUMENT_BUNDLE_SCHEMA_VERSION: &str = "1.0";
const MAX_DOCX_XML_BYTES: u64 = 25_000_000;
const MAX_DOCX_MEDIA_BYTES: u64 = 20_000_000;
const MAX_DOCX_TOTAL_MEDIA_BYTES: u64 = 150_000_000;
const MAX_DOCX_MEDIA_FILES: usize = 500;
const MAX_DOCX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_REPRESENTATION_BYTES: usize = 1_000_000;
/// The artifact viewer accepts a canonical bundle up to 16 MiB.
pub const MAX_DOCUMENT_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
/// Direct-provider Read is capped at 5 MiB, so the derived model index must fit
/// the strictest provider consumer even when the durable canonical bundle is
/// larger.
const MAX_MODEL_DOCUMENT_INDEX_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBundle {
    pub schema_version: String,
    pub bundle_id: String,
    pub source_kind: String,
    pub origins: Vec<DocumentOrigin>,
    pub pages: Vec<DocumentPage>,
    pub nodes: Vec<DocumentNode>,
    pub assets: Vec<DocumentAsset>,
    #[serde(default)]
    pub links: Vec<DocumentLink>,
    pub extraction: DocumentExtraction,
    #[serde(default)]
    pub quality: Vec<DocumentQualityNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentOrigin {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPage {
    pub number: u32,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentNode {
    pub id: String,
    pub kind: String,
    pub order: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub asset_ids: Vec<String>,
    #[serde(default)]
    pub representations: Vec<DocumentRepresentation>,
    pub provenance: DocumentProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRepresentation {
    pub format: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentProvenance {
    pub origin_id: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAsset {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub rel_path: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub provenance: DocumentProvenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentLink {
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentExtraction {
    pub method: String,
    pub source_path: String,
    pub paper_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentQualityNote {
    pub severity: String,
    pub scope: String,
    pub message: String,
}

#[derive(Serialize)]
struct ModelDocumentIndex<'a> {
    format: &'static str,
    schema_version: &'a str,
    bundle_id: &'a str,
    source_kind: &'a str,
    origins: &'a [DocumentOrigin],
    pages: &'a [DocumentPage],
    nodes: Vec<ModelDocumentNode<'a>>,
    assets: &'a [DocumentAsset],
    links: Vec<&'a DocumentLink>,
    extraction: &'a DocumentExtraction,
    quality: &'a [DocumentQualityNote],
}

#[derive(Serialize)]
struct ModelDocumentNode<'a> {
    id: &'a str,
    kind: &'a str,
    order: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    representations: Option<&'a [DocumentRepresentation]>,
    asset_ids: &'a [String],
    provenance: &'a DocumentProvenance,
}

#[derive(Debug, Clone)]
pub struct BundleBuild {
    pub bundle: DocumentBundle,
    /// Files copied into the run by the builder and not yet in the manifest.
    pub added_artifacts: Vec<(String, String, String)>,
}

fn source_kind(extraction: &ExtractionResult) -> String {
    let lower = extraction.source_path.to_ascii_lowercase();
    if extraction.method == "folder" {
        "folder"
    } else if extraction.method == "none" {
        "none"
    } else if lower.ends_with(".docx") {
        "docx"
    } else if lower.ends_with(".tex") || extraction.method == "latex" {
        "latex"
    } else if lower.ends_with(".pdf") {
        "pdf"
    } else {
        "text"
    }
    .to_string()
}

fn stable_suffix(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")[..12].to_string()
}

fn mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn image_dimensions(path: &Path) -> (Option<u32>, Option<u32>) {
    let Ok(mut file) = crate::safety::open_regular_file(path) else {
        return (None, None);
    };
    let mut header = [0u8; 24];
    if file.read_exact(&mut header).is_err() {
        return (None, None);
    }
    if &header[..8] == b"\x89PNG\r\n\x1a\n" {
        return (
            Some(u32::from_be_bytes(header[16..20].try_into().unwrap())),
            Some(u32::from_be_bytes(header[20..24].try_into().unwrap())),
        );
    }
    let mut bytes = header.to_vec();
    if file
        .take((1024 * 1024 - header.len()) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return (None, None);
    }
    jpeg_dimensions(&bytes)
}

fn jpeg_dimensions(bytes: &[u8]) -> (Option<u32>, Option<u32>) {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return (None, None);
    }
    let mut cursor = 2usize;
    while cursor + 4 <= bytes.len() {
        while cursor < bytes.len() && bytes[cursor] != 0xff {
            cursor += 1;
        }
        while cursor < bytes.len() && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;
        if marker == 0x01 || (0xd0..=0xd9).contains(&marker) {
            continue;
        }
        if cursor + 2 > bytes.len() {
            break;
        }
        let segment_len = u16::from_be_bytes([bytes[cursor], bytes[cursor + 1]]) as usize;
        if segment_len < 2 || cursor + segment_len > bytes.len() {
            break;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) && segment_len >= 7
        {
            let height = u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]) as u32;
            let width = u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]) as u32;
            return (Some(width), Some(height));
        }
        cursor += segment_len;
    }
    (None, None)
}

fn page_number_from_name(name: &str) -> Option<u32> {
    let stem = name.rsplit_once('.').map(|(left, _)| left).unwrap_or(name);
    stem.rsplit(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

fn safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn asset_from_file(
    run_dir: &Path,
    rel_path: &str,
    kind: &str,
    label: String,
    page: Option<u32>,
    method: &str,
) -> Option<DocumentAsset> {
    if !safe_relative_path(rel_path) {
        return None;
    }
    let path = run_dir.join(rel_path);
    if !path.is_file() {
        return None;
    }
    let (width, height) = image_dimensions(&path);
    Some(DocumentAsset {
        id: format!("asset-{}", stable_suffix(rel_path)),
        kind: kind.to_string(),
        label,
        rel_path: rel_path.replace('\\', "/"),
        media_type: mime_type(&path).to_string(),
        page,
        width,
        height,
        provenance: DocumentProvenance {
            origin_id: "origin-primary".to_string(),
            method: method.to_string(),
            confidence: Some(1.0),
        },
    })
}

fn scan_asset_dir(run_dir: &Path, rel_dir: &str, kind: &str, method: &str) -> Vec<DocumentAsset> {
    let dir = run_dir.join(rel_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            (file_type.is_file() && !file_type.is_symlink()).then_some(entry.path())
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            let rel_path = format!("{rel_dir}/{name}");
            let page = (kind == "page")
                .then(|| page_number_from_name(&name))
                .flatten();
            let label = match page {
                Some(page) => format!("Page {page}"),
                None => name,
            };
            asset_from_file(run_dir, &rel_path, kind, label, page, method)
        })
        .collect()
}

fn split_pages(text: &str) -> Vec<(Option<u32>, String)> {
    let marker = Regex::new(r"(?im)^\s*<!--\s*PAGE\s+(\d+)\s*-->\s*$").unwrap();
    let mut output = Vec::new();
    let mut last = 0;
    let mut current_page = None;
    for capture in marker.captures_iter(text) {
        let full = capture.get(0).unwrap();
        let content = text[last..full.start()].trim();
        if !content.is_empty() {
            output.push((current_page, content.to_string()));
        }
        current_page = capture.get(1).and_then(|value| value.as_str().parse().ok());
        last = full.end();
    }
    let content = text[last..].trim();
    if !content.is_empty() {
        output.push((current_page, content.to_string()));
    }
    if output.is_empty() && !text.trim().is_empty() {
        output.push((None, text.trim().to_string()));
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn node(
    bundle: &DocumentBundle,
    kind: &str,
    page: Option<u32>,
    label: Option<String>,
    number: Option<String>,
    text: String,
    asset_ids: Vec<String>,
    representations: Vec<DocumentRepresentation>,
    method: &str,
) -> DocumentNode {
    let order = bundle.nodes.len() as u32 + 1;
    DocumentNode {
        id: format!("{kind}-{order:05}"),
        kind: kind.to_string(),
        order,
        page,
        parent_id: page.map(|page| format!("page-{page:04}")),
        label,
        number,
        text,
        asset_ids,
        representations,
        provenance: DocumentProvenance {
            origin_id: "origin-primary".to_string(),
            method: method.to_string(),
            confidence: Some(0.9),
        },
    }
}

fn add_markdown_nodes(bundle: &mut DocumentBundle, text: &str, method: &str) {
    let caption_re =
        Regex::new(r"(?i)^\s*(figure|fig\.?|table)\s+([A-Z]?\d+(?:\.\d+)*)\s*[:.—-]?\s*(.*)$")
            .unwrap();
    let formal_re =
        Regex::new(r"(?i)^\s*(theorem|proposition|lemma|corollary)\s*([A-Z]?\d*(?:\.\d+)*)")
            .unwrap();
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();

    for (page, page_text) in split_pages(text) {
        let page_assets: Vec<String> = page
            .into_iter()
            .flat_map(|number| {
                bundle
                    .pages
                    .iter()
                    .filter(move |entry| entry.number == number)
                    .filter_map(|entry| entry.asset_id.clone())
            })
            .collect();
        let page_node = node(
            bundle,
            "page_text",
            page,
            page.map(|number| format!("Page {number}")),
            page.map(|number| number.to_string()),
            page_text.clone(),
            page_assets,
            Vec::new(),
            method,
        );
        bundle.nodes.push(page_node);

        let lines: Vec<&str> = page_text.lines().collect();
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index].trim();
            if let Some(capture) = heading_re.captures(line) {
                let title = capture[2].trim().to_string();
                let heading = node(
                    bundle,
                    "section",
                    page,
                    Some(title.clone()),
                    None,
                    title,
                    Vec::new(),
                    vec![DocumentRepresentation {
                        format: "heading_level".to_string(),
                        content: serde_json::json!(capture[1].len()),
                    }],
                    method,
                );
                bundle.nodes.push(heading);
            } else if let Some(capture) = caption_re.captures(line) {
                let raw_kind = capture[1].to_ascii_lowercase();
                let kind = if raw_kind.starts_with("fig") {
                    "figure"
                } else {
                    "table"
                };
                let number = capture[2].to_string();
                let text = capture
                    .get(3)
                    .map(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let asset_ids = closest_assets(bundle, kind, page, &number);
                let caption = node(
                    bundle,
                    kind,
                    page,
                    Some(format!("{} {number}", capitalize(kind))),
                    Some(number),
                    text.to_string(),
                    asset_ids,
                    Vec::new(),
                    method,
                );
                bundle.nodes.push(caption);
            } else if let Some(capture) = formal_re.captures(line) {
                let kind = capture[1].to_ascii_lowercase();
                let number = capture
                    .get(2)
                    .map(|value| value.as_str().to_string())
                    .filter(|value| !value.is_empty());
                let formal = node(
                    bundle,
                    &kind,
                    page,
                    Some(capitalize(&kind)),
                    number,
                    line.to_string(),
                    Vec::new(),
                    Vec::new(),
                    method,
                );
                bundle.nodes.push(formal);
            } else if line.starts_with("$$") || line.starts_with("\\[") {
                let close = if line.starts_with("$$") { "$$" } else { "\\]" };
                let mut equation = line.to_string();
                while !equation[2..].contains(close) && index + 1 < lines.len() {
                    index += 1;
                    equation.push('\n');
                    equation.push_str(lines[index]);
                }
                let equation_node = node(
                    bundle,
                    "equation",
                    page,
                    Some("Display equation".to_string()),
                    None,
                    equation.clone(),
                    Vec::new(),
                    vec![DocumentRepresentation {
                        format: "latex".to_string(),
                        content: serde_json::Value::String(equation),
                    }],
                    method,
                );
                bundle.nodes.push(equation_node);
            } else if line.contains('|')
                && index + 1 < lines.len()
                && is_markdown_separator(lines[index + 1])
            {
                let mut table_lines = vec![lines[index].to_string(), lines[index + 1].to_string()];
                index += 2;
                while index < lines.len() && lines[index].contains('|') {
                    table_lines.push(lines[index].to_string());
                    index += 1;
                }
                index = index.saturating_sub(1);
                let grid: Vec<Vec<String>> = table_lines
                    .iter()
                    .filter(|line| !is_markdown_separator(line))
                    .map(|line| {
                        line.trim_matches('|')
                            .split('|')
                            .map(|cell| cell.trim().to_string())
                            .collect()
                    })
                    .collect();
                let table = node(
                    bundle,
                    "table",
                    page,
                    Some("Extracted table".to_string()),
                    None,
                    table_lines.join("\n"),
                    Vec::new(),
                    vec![DocumentRepresentation {
                        format: "grid".to_string(),
                        content: serde_json::json!(grid),
                    }],
                    method,
                );
                bundle.nodes.push(table);
            }
            index += 1;
        }
    }
}

mod adapters;
mod bundle;
mod paddle;

use adapters::{
    add_tex_nodes, capitalize, closest_assets, copy_docx_media, copy_tex_assets,
    is_markdown_separator, parse_docx, xml_text,
};
use paddle::add_paddle_structured_nodes;

pub use adapters::{companion_pdf, extract_docx_text};
pub use bundle::build;

#[cfg(test)]
use adapters::{copy_docx_media_with_limits, read_docx_entry_limited};

#[cfg(test)]
mod tests;
