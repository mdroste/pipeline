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

fn marker_node_kind(marker_type: &str, role: &str) -> &'static str {
    match role {
        "footnote" => "footnote",
        "possible_footnote" => "possible_footnote",
        "page_header" => "page_header",
        "page_footer" => "page_footer",
        _ => match marker_type {
            "SectionHeader" => "section",
            "Equation" => "equation",
            "Table" | "TableGroup" => "table",
            "Figure" | "FigureGroup" | "Picture" | "PictureGroup" => "figure",
            "Caption" => "caption",
            "Code" => "code",
            "ListGroup" | "ListItem" => "list",
            _ => "text_block",
        },
    }
}

fn marker_node_label(marker_type: &str, role: &str, page: u32) -> String {
    match role {
        "footnote" => format!("Footnote on page {page}"),
        "possible_footnote" => format!("Possible footnote on page {page}"),
        "page_header" => format!("Page {page} header"),
        "page_footer" => format!("Page {page} footer"),
        _ => format!("Marker {marker_type} block"),
    }
}

fn marker_note_number(text: &str, role: &str) -> Option<String> {
    if !matches!(role, "footnote" | "possible_footnote") {
        return None;
    }
    Regex::new(r"^\s*(\d{1,3}|[*†‡])(?:[.)])?\s+")
        .unwrap()
        .captures(text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
}

fn marker_block_assets(
    bundle: &DocumentBundle,
    block: &crate::pipeline::extract::MarkerStructuredBlock,
    page: u32,
    kind: &str,
) -> Vec<String> {
    let image_names: HashSet<&str> = block.image_files.iter().map(String::as_str).collect();
    let mut asset_ids: Vec<String> = bundle
        .assets
        .iter()
        .filter(|asset| {
            image_names.contains(asset.label.as_str())
                || image_names
                    .iter()
                    .any(|name| asset.rel_path.ends_with(name))
        })
        .map(|asset| asset.id.clone())
        .collect();
    if asset_ids.is_empty() && matches!(kind, "figure" | "table") {
        if let Some(asset) = bundle
            .assets
            .iter()
            .find(|asset| asset.kind == "page" && asset.page == Some(page))
        {
            asset_ids.push(asset.id.clone());
        }
    }
    asset_ids.sort();
    asset_ids.dedup();
    asset_ids
}

fn add_marker_structured_nodes(
    bundle: &mut DocumentBundle,
    structure: &crate::pipeline::extract::MarkerStructure,
) {
    for page in &structure.pages {
        for block in &page.blocks {
            let kind = marker_node_kind(&block.marker_type, &block.role);
            let representation = DocumentRepresentation {
                format: "marker_block".to_string(),
                content: serde_json::json!({
                    "marker_id": block.marker_id,
                    "marker_type": block.marker_type,
                    "role": block.role,
                    "html": block.html,
                    "polygon": block.polygon,
                    "bbox": block.bbox,
                }),
            };
            let asset_ids = marker_block_assets(bundle, block, page.number, kind);
            let mut created = node(
                bundle,
                kind,
                Some(page.number),
                Some(marker_node_label(
                    &block.marker_type,
                    &block.role,
                    page.number,
                )),
                marker_note_number(&block.text, &block.role),
                block.text.clone(),
                asset_ids,
                vec![representation],
                "marker",
            );
            if block.role == "possible_footnote" {
                created.provenance.confidence = Some(0.6);
            }
            bundle.nodes.push(created);
        }
    }
}

fn paddle_node_kind(block: &crate::pipeline::extract::PaddleStructuredBlock) -> &'static str {
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    };
    match label.to_ascii_lowercase().as_str() {
        "footnote" => "footnote",
        "possible_footnote" => "possible_footnote",
        "page_header" | "header" | "header_image" => "page_header",
        "page_footer" | "footer" | "footer_image" | "number" => "page_footer",
        "doc_title" | "document_title" | "paragraph_title" | "section_title" | "abstract_title"
        | "reference_title" => "section",
        "formula" | "display_formula" | "inline_formula" | "equation" => "equation",
        // A formula number is a separate layout region, not a second equation.
        // `add_paddle_structured_nodes` attaches adjacent numbers to their
        // equation node and retains unmatched regions as ordinary text.
        "formula_number" => "text_block",
        "table" | "table_body" => "table",
        "image" | "figure" | "chart" | "seal" => "figure",
        "caption" | "figure_title" | "image_caption" | "table_title" | "table_caption"
        | "chart_title" | "vision_footnote" => "caption",
        "code" | "algorithm" => "code",
        "list" | "list_item" => "list",
        _ => {
            let markdown = block.markdown.trim();
            let lines: Vec<&str> = markdown.lines().collect();
            if Regex::new(r"^#{1,6}\s+").unwrap().is_match(markdown) {
                "section"
            } else if markdown.starts_with("```") || markdown.starts_with("~~~") {
                "code"
            } else if markdown.starts_with("$$")
                || markdown.starts_with(r"\[")
                || (markdown.starts_with('$') && markdown.ends_with('$'))
            {
                "equation"
            } else if lines.len() >= 2 && lines[0].contains('|') && is_markdown_separator(lines[1])
            {
                "table"
            } else if Regex::new(r"(?i)^(figure|fig\.?|table)\s+[A-Z]?\d")
                .unwrap()
                .is_match(markdown)
            {
                "caption"
            } else if Regex::new(r"(?m)^\s*(?:[-+*]|\d+[.)])\s+")
                .unwrap()
                .is_match(markdown)
            {
                "list"
            } else {
                "text_block"
            }
        }
    }
}

fn paddle_block_label(block: &crate::pipeline::extract::PaddleStructuredBlock) -> &str {
    if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    }
}

fn is_paddle_formula_number(block: &crate::pipeline::extract::PaddleStructuredBlock) -> bool {
    paddle_block_label(block).eq_ignore_ascii_case("formula_number")
}

fn parse_paddle_formula_number(value: &str) -> Option<String> {
    let mut value = value.trim();
    while value.len() >= 2 && value.starts_with('$') && value.ends_with('$') {
        value = value[1..value.len() - 1].trim();
    }
    if value.starts_with(r"\(") && value.ends_with(r"\)") && value.len() >= 4 {
        value = value[2..value.len() - 2].trim();
    } else if value.starts_with(r"\[") && value.ends_with(r"\]") && value.len() >= 4 {
        value = value[2..value.len() - 2].trim();
    }
    if let Some(capture) = Regex::new(r"^\\tag\*?\{([^{}]{1,40})\}$")
        .unwrap()
        .captures(value)
    {
        value = capture.get(1)?.as_str().trim();
    }
    if ((value.starts_with('(') && value.ends_with(')'))
        || (value.starts_with('[') && value.ends_with(']')))
        && value.len() >= 2
    {
        value = value[1..value.len() - 1].trim();
    }
    if Regex::new(r"(?i)^[a-z0-9]+(?:[.:-][a-z0-9]+)*$")
        .unwrap()
        .is_match(value)
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn paddle_formula_number(
    block: &crate::pipeline::extract::PaddleStructuredBlock,
) -> Option<String> {
    if !is_paddle_formula_number(block) {
        return None;
    }
    parse_paddle_formula_number(&block.text)
        .or_else(|| parse_paddle_formula_number(&block.markdown))
}

fn paddle_block_representation(
    block: &crate::pipeline::extract::PaddleStructuredBlock,
    structure: &crate::pipeline::extract::PaddleStructure,
) -> DocumentRepresentation {
    DocumentRepresentation {
        format: "paddle_block".to_string(),
        content: serde_json::json!({
            "block_id": block.block_id,
            "role": block.role,
            "block_label": block.block_label,
            "markdown": block.markdown,
            "boundary": block.boundary,
            "note_marker": block.note_marker,
            "order": block.order,
            "bbox": block.bbox,
            "polygon": block.polygon,
            "confidence": block.confidence,
            "asset_files": block.asset_files,
            "raw": block.raw,
            "parser": structure.parser,
            "parser_version": structure.parser_version,
            "parser_settings": structure.settings,
        }),
    }
}

fn paddle_node_label(kind: &str, page: u32) -> String {
    match kind {
        "footnote" => format!("Footnote on page {page}"),
        "possible_footnote" => format!("Possible footnote on page {page}"),
        "page_header" => format!("Page {page} header"),
        "page_footer" => format!("Page {page} footer"),
        "section" => format!("Section block on page {page}"),
        "equation" => format!("Equation block on page {page}"),
        "table" => format!("Table block on page {page}"),
        "figure" => format!("Figure block on page {page}"),
        "caption" => format!("Caption block on page {page}"),
        _ => format!("PaddleOCR-VL block on page {page}"),
    }
}

fn paddle_heading_level(block: &crate::pipeline::extract::PaddleStructuredBlock) -> Option<usize> {
    if let Some(capture) = Regex::new(r"^(#{1,6})\s+")
        .unwrap()
        .captures(block.markdown.trim_start())
    {
        return capture.get(1).map(|marker| marker.as_str().len());
    }
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    };
    match label.to_ascii_lowercase().as_str() {
        "doc_title" | "document_title" => Some(1),
        "paragraph_title" | "section_title" | "abstract_title" | "reference_title" => Some(2),
        _ => None,
    }
}

fn paddle_block_assets(
    bundle: &DocumentBundle,
    block: &crate::pipeline::extract::PaddleStructuredBlock,
    page: u32,
    kind: &str,
) -> Vec<String> {
    let names: HashSet<&str> = block
        .asset_files
        .iter()
        .filter_map(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
        .collect();
    let mut asset_ids: Vec<String> = bundle
        .assets
        .iter()
        .filter(|asset| {
            names.contains(asset.label.as_str())
                || names.iter().any(|name| asset.rel_path.ends_with(name))
        })
        .map(|asset| asset.id.clone())
        .collect();
    if asset_ids.is_empty() && matches!(kind, "figure" | "table") {
        if let Some(asset) = bundle
            .assets
            .iter()
            .find(|asset| asset.kind == "page" && asset.page == Some(page))
        {
            asset_ids.push(asset.id.clone());
        }
    }
    asset_ids.sort();
    asset_ids.dedup();
    asset_ids
}

fn add_paddle_structured_nodes(
    bundle: &mut DocumentBundle,
    structure: &crate::pipeline::extract::PaddleStructure,
) {
    let mut heading_stack: [Option<String>; 6] = std::array::from_fn(|_| None);
    for page in &structure.pages {
        for (block_index, block) in page.blocks.iter().enumerate() {
            if let Some(number) = paddle_formula_number(block) {
                let follows_equation = block_index > 0
                    && !is_paddle_formula_number(&page.blocks[block_index - 1])
                    && paddle_node_kind(&page.blocks[block_index - 1]) == "equation";
                if follows_equation {
                    if let Some(equation) = bundle
                        .nodes
                        .last_mut()
                        .filter(|node| node.kind == "equation" && node.page == Some(page.number))
                    {
                        if equation.number.is_none() {
                            equation.number = Some(number);
                        }
                        equation
                            .representations
                            .push(paddle_block_representation(block, structure));
                        continue;
                    }
                }
                let precedes_equation = page.blocks.get(block_index + 1).is_some_and(|next| {
                    !is_paddle_formula_number(next) && paddle_node_kind(next) == "equation"
                });
                if precedes_equation {
                    // The following equation will absorb this leading number
                    // and retain this block as an additional representation.
                    continue;
                }
            }
            let kind = paddle_node_kind(block);
            let asset_ids = paddle_block_assets(bundle, block, page.number, kind);
            let mut representations = vec![paddle_block_representation(block, structure)];
            let leading_number = (kind == "equation" && block_index > 0)
                .then(|| &page.blocks[block_index - 1])
                .filter(|previous| is_paddle_formula_number(previous));
            let number = leading_number.and_then(paddle_formula_number);
            if let Some(number_block) = leading_number {
                representations.push(paddle_block_representation(number_block, structure));
            }
            let mut created = node(
                bundle,
                kind,
                Some(page.number),
                Some(paddle_node_label(kind, page.number)),
                number.or_else(|| block.note_marker.clone()),
                block.text.clone(),
                asset_ids,
                representations,
                if structure.parser == "paddleocr-vl-full" {
                    "paddleocr-vl-full"
                } else {
                    "paddleocr-vl"
                },
            );
            created.provenance.confidence = block.confidence.or(match block.role.as_str() {
                "possible_footnote" => Some(0.6),
                "page_header" | "page_footer" => Some(0.8),
                _ => None,
            });
            if !matches!(kind, "page_header" | "page_footer") {
                if let Some(level) = paddle_heading_level(block) {
                    created.parent_id = heading_stack[..level.saturating_sub(1)]
                        .iter()
                        .rev()
                        .find_map(|value| value.clone());
                    heading_stack[level - 1] = Some(created.id.clone());
                    for slot in &mut heading_stack[level..] {
                        *slot = None;
                    }
                } else {
                    created.parent_id = heading_stack.iter().rev().find_map(|value| value.clone());
                }
            };
            bundle.nodes.push(created);
        }
    }
}

fn is_markdown_separator(line: &str) -> bool {
    let trimmed = line.trim().trim_matches('|');
    !trimmed.is_empty()
        && trimmed
            .split('|')
            .all(|cell| cell.trim().trim_matches(':').chars().all(|ch| ch == '-'))
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn closest_assets(
    bundle: &DocumentBundle,
    kind: &str,
    page: Option<u32>,
    number: &str,
) -> Vec<String> {
    let needle = format!("{kind} {number}").to_ascii_lowercase();
    let mut matched: Vec<String> = bundle
        .assets
        .iter()
        .filter(|asset| {
            asset.kind == kind
                || asset.label.to_ascii_lowercase().contains(&needle)
                || page.is_some_and(|page| asset.page == Some(page))
        })
        .map(|asset| asset.id.clone())
        .collect();
    if matched.is_empty() {
        if let Some(page) = page {
            if let Some(asset) = bundle
                .assets
                .iter()
                .find(|asset| asset.kind == "page" && asset.page == Some(page))
            {
                matched.push(asset.id.clone());
            }
        }
    }
    matched.truncate(3);
    matched
}

fn add_tex_nodes(bundle: &mut DocumentBundle, source: &str, method: &str) {
    let section_re = Regex::new(
        r"(?s)\\(?P<kind>section|subsection|subsubsection)\*?\{(?P<title>[^{}]{1,500})\}",
    )
    .unwrap();
    for capture in section_re.captures_iter(source) {
        let title = capture["title"].trim().to_string();
        let representation = DocumentRepresentation {
            format: "latex".to_string(),
            content: serde_json::Value::String(capture[0].to_string()),
        };
        let section = node(
            bundle,
            "section",
            None,
            Some(title.clone()),
            None,
            title,
            Vec::new(),
            vec![representation],
            method,
        );
        bundle.nodes.push(section);
    }

    for environment in [
        "equation",
        "equation*",
        "align",
        "align*",
        "gather",
        "multline",
    ] {
        let pattern = format!(
            r"(?s)\\begin\{{{}\}}(.*?)\\end\{{{}\}}",
            regex::escape(environment),
            regex::escape(environment)
        );
        let regex = Regex::new(&pattern).unwrap();
        for capture in regex.captures_iter(source) {
            let latex = capture.get(1).unwrap().as_str().trim().to_string();
            let equation = node(
                bundle,
                "equation",
                None,
                Some("LaTeX equation".to_string()),
                extract_tex_label(capture.get(0).unwrap().as_str()),
                latex.clone(),
                Vec::new(),
                vec![DocumentRepresentation {
                    format: "latex".to_string(),
                    content: serde_json::Value::String(latex),
                }],
                method,
            );
            bundle.nodes.push(equation);
        }
    }

    for kind in ["figure", "table"] {
        let pattern = format!(
            r"(?s)\\begin\{{{}\}}(.*?)\\end\{{{}\}}",
            regex::escape(kind),
            regex::escape(kind)
        );
        let regex = Regex::new(&pattern).unwrap();
        for capture in regex.captures_iter(source) {
            let body = capture.get(1).unwrap().as_str();
            let caption = extract_tex_command(body, "caption").unwrap_or_default();
            let number = extract_tex_label(body);
            let asset_ids = if kind == "figure" {
                tex_graphic_names(body)
                    .into_iter()
                    .flat_map(|name| {
                        bundle
                            .assets
                            .iter()
                            .filter(move |asset| asset.label.contains(&name))
                            .map(|asset| asset.id.clone())
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let structure = node(
                bundle,
                kind,
                None,
                Some(if caption.is_empty() {
                    capitalize(kind)
                } else {
                    caption.clone()
                }),
                number,
                caption,
                asset_ids,
                vec![DocumentRepresentation {
                    format: "latex".to_string(),
                    content: serde_json::Value::String(body.trim().to_string()),
                }],
                method,
            );
            bundle.nodes.push(structure);
        }
    }
}

fn extract_tex_command(source: &str, command: &str) -> Option<String> {
    let regex = Regex::new(&format!(r"(?s)\\{}\s*\{{([^{{}}]{{0,4000}})\}}", command)).ok()?;
    regex
        .captures(source)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_string())
}

fn extract_tex_label(source: &str) -> Option<String> {
    extract_tex_command(source, "label")
}

fn tex_graphic_names(source: &str) -> Vec<String> {
    let regex = Regex::new(r"\\includegraphics(?:\[[^\]]*\])?\{([^}]+)\}").unwrap();
    regex
        .captures_iter(source)
        .filter_map(|capture| capture.get(1))
        .map(|value| value.as_str().trim().to_string())
        .collect()
}

fn resolve_tex_graphic(root: &Path, raw: &str) -> Option<PathBuf> {
    let raw_path = Path::new(raw);
    if raw_path.is_absolute()
        || raw_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    let candidates: Vec<PathBuf> = if raw_path.extension().is_some() {
        vec![root.join(raw_path)]
    } else {
        ["pdf", "png", "jpg", "jpeg", "webp"]
            .iter()
            .map(|extension| root.join(raw_path).with_extension(extension))
            .collect()
    };
    let canonical_root = root.canonicalize().ok()?;
    candidates.into_iter().find_map(|candidate| {
        let resolved = candidate.canonicalize().ok()?;
        (resolved.starts_with(&canonical_root) && resolved.is_file()).then_some(resolved)
    })
}

fn copy_tex_assets(
    extraction: &ExtractionResult,
    run_dir: &Path,
) -> Result<Vec<(String, String, String)>, String> {
    let source_path = Path::new(&extraction.source_path);
    let root = source_path.parent().unwrap_or_else(|| Path::new("."));
    let names = tex_graphic_names(&extraction.text);
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let destination = run_dir.join("artifacts/document/figures");
    fs::create_dir_all(&destination)
        .map_err(|error| format!("Failed to create document figure directory: {error}"))?;
    let mut added = Vec::new();
    let mut seen = HashSet::new();
    for (index, raw) in names.iter().enumerate() {
        let Some(source) = resolve_tex_graphic(root, raw) else {
            continue;
        };
        let Some(file_name) = source.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let safe_name = format!("{:03}_{}", index + 1, file_name.replace(['/', '\\'], "_"));
        let rel_path = format!("artifacts/document/figures/{safe_name}");
        if !seen.insert(rel_path.clone()) {
            continue;
        }
        fs::copy(&source, run_dir.join(&rel_path))
            .map_err(|error| format!("Failed to copy {}: {error}", source.display()))?;
        added.push((rel_path, raw.clone(), "figures".to_string()));
    }
    Ok(added)
}

fn xml_decode(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn xml_text(fragment: &str) -> String {
    let text_re = Regex::new(r"(?s)<(?:w|m):t(?:\s[^>]*)?>(.*?)</(?:w|m):t>").unwrap();
    text_re
        .captures_iter(fragment)
        .filter_map(|capture| capture.get(1))
        .map(|value| xml_decode(value.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

fn read_docx_entry_limited(
    reader: &mut impl std::io::Read,
    limit: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity((limit.min(64 * 1024)) as usize);
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read {label}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{label} exceeds the {} MB decompressed safety limit",
            limit / 1_000_000
        ));
    }
    Ok(bytes)
}

#[derive(Default)]
struct ParsedDocx {
    markdown: String,
    tables: Vec<Vec<Vec<String>>>,
    equations: Vec<String>,
}

fn read_docx_xml(path: &Path) -> Result<String, String> {
    let file = crate::safety::open_regular_file(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Invalid DOCX archive: {error}"))?;
    let mut document = archive
        .by_name("word/document.xml")
        .map_err(|error| format!("DOCX is missing word/document.xml: {error}"))?;
    if document.size() > MAX_DOCX_XML_BYTES {
        return Err(format!(
            "DOCX document XML exceeds the {} MB safety limit",
            MAX_DOCX_XML_BYTES / 1_000_000
        ));
    }
    let bytes = read_docx_entry_limited(&mut document, MAX_DOCX_XML_BYTES, "DOCX document XML")?;
    String::from_utf8(bytes)
        .map_err(|error| format!("DOCX document XML is not valid UTF-8: {error}"))
}

fn parse_docx(path: &Path) -> Result<ParsedDocx, String> {
    let xml = read_docx_xml(path)?;
    let paragraph_re = Regex::new(r"(?s)<w:p(?:\s[^>]*)?>(.*?)</w:p>").unwrap();
    let heading_re =
        Regex::new(r#"<w:pStyle[^>]*w:val="(?:Heading|heading)(\d+)"[^>]*/?>"#).unwrap();
    let mut markdown = String::new();
    for capture in paragraph_re.captures_iter(&xml) {
        let body = capture.get(1).unwrap().as_str();
        let text = xml_text(body);
        if text.trim().is_empty() {
            continue;
        }
        if let Some(heading) = heading_re.captures(body) {
            let level: usize = heading[1].parse().unwrap_or(1).clamp(1, 6);
            markdown.push_str(&"#".repeat(level));
            markdown.push(' ');
        }
        markdown.push_str(text.trim());
        markdown.push_str("\n\n");
    }

    let table_re = Regex::new(r"(?s)<w:tbl(?:\s[^>]*)?>(.*?)</w:tbl>").unwrap();
    let row_re = Regex::new(r"(?s)<w:tr(?:\s[^>]*)?>(.*?)</w:tr>").unwrap();
    let cell_re = Regex::new(r"(?s)<w:tc(?:\s[^>]*)?>(.*?)</w:tc>").unwrap();
    let mut tables = Vec::new();
    for table_capture in table_re.captures_iter(&xml) {
        let mut grid = Vec::new();
        for row_capture in row_re.captures_iter(&table_capture[1]) {
            let row: Vec<String> = cell_re
                .captures_iter(&row_capture[1])
                .map(|cell| xml_text(&cell[1]).trim().to_string())
                .collect();
            if !row.is_empty() {
                grid.push(row);
            }
        }
        if !grid.is_empty() {
            tables.push(grid);
        }
    }
    if !tables.is_empty() {
        markdown.push_str("## Extracted tables\n\n");
        for (index, grid) in tables.iter().enumerate() {
            markdown.push_str(&format!("### Table {}\n\n", index + 1));
            let width = grid.iter().map(Vec::len).max().unwrap_or(0);
            if width == 0 {
                continue;
            }
            let first = &grid[0];
            markdown.push('|');
            for column in 0..width {
                markdown.push(' ');
                markdown.push_str(first.get(column).map(String::as_str).unwrap_or(""));
                markdown.push_str(" |");
            }
            markdown.push('\n');
            markdown.push('|');
            for _ in 0..width {
                markdown.push_str(" --- |");
            }
            markdown.push('\n');
            for row in grid.iter().skip(1) {
                markdown.push('|');
                for column in 0..width {
                    markdown.push(' ');
                    markdown.push_str(row.get(column).map(String::as_str).unwrap_or(""));
                    markdown.push_str(" |");
                }
                markdown.push('\n');
            }
            markdown.push('\n');
        }
    }

    let math_re = Regex::new(r"(?s)<m:oMath(?:Para)?(?:\s[^>]*)?>.*?</m:oMath(?:Para)?>").unwrap();
    let equations = math_re
        .find_iter(&xml)
        .take(1_000)
        .map(|value| {
            let raw = value.as_str();
            let mut end = raw.len().min(MAX_REPRESENTATION_BYTES);
            while !raw.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            raw[..end].to_string()
        })
        .collect();
    Ok(ParsedDocx {
        markdown,
        tables,
        equations,
    })
}

pub fn extract_docx_text(path: &Path) -> Result<String, String> {
    let parsed = parse_docx(path)?;
    if parsed.markdown.trim().is_empty() {
        Err("DOCX extraction produced empty output".to_string())
    } else {
        Ok(parsed.markdown)
    }
}

fn copy_docx_media_with_limits(
    path: &Path,
    run_dir: &Path,
    max_entry_bytes: u64,
    max_total_bytes: u64,
    max_files: usize,
    max_archive_entries: usize,
) -> Result<Vec<(String, String, String)>, String> {
    let file = crate::safety::open_regular_file(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Invalid DOCX archive: {error}"))?;
    if archive.len() > max_archive_entries {
        return Err(format!(
            "DOCX archive contains {} entries; the safety limit is {max_archive_entries}",
            archive.len()
        ));
    }
    let destination = run_dir.join("artifacts/document/figures");
    let staging = tempfile::Builder::new()
        .prefix(".pipeline-docx-media-")
        .tempdir_in(run_dir)
        .map_err(|error| format!("Failed to create DOCX media staging directory: {error}"))?;
    let staged_destination = staging.path().join("figures");
    let mut added = Vec::new();
    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Failed to inspect DOCX media: {error}"))?;
        let name = entry.name().to_string();
        if !name.starts_with("word/media/") || entry.is_dir() {
            continue;
        }
        if entry.size() > max_entry_bytes {
            return Err(format!(
                "DOCX media entry '{}' exceeds the {max_entry_bytes} byte decompressed safety limit",
                entry.name()
            ));
        }
        let Some(file_name) = Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
        else {
            continue;
        };
        if !matches!(
            Path::new(file_name)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg"
        ) {
            continue;
        }
        if added.len() >= max_files {
            return Err(format!(
                "DOCX contains more than {max_files} supported media files"
            ));
        }
        let remaining = max_total_bytes.saturating_sub(total_bytes);
        if remaining == 0 || entry.size() > remaining {
            return Err(format!(
                "DOCX media exceeds the {max_total_bytes} byte cumulative decompressed safety limit"
            ));
        }
        fs::create_dir_all(&staged_destination)
            .map_err(|error| format!("Failed to create DOCX media directory: {error}"))?;
        let safe_name = format!("{:03}_{}", added.len() + 1, file_name);
        let rel_path = format!("artifacts/document/figures/{safe_name}");
        let limit = max_entry_bytes.min(remaining);
        let mut output = fs::File::create(staged_destination.join(&safe_name))
            .map_err(|error| format!("Failed to stage DOCX media artifact: {error}"))?;
        let copied = std::io::copy(&mut (&mut entry).take(limit.saturating_add(1)), &mut output)
            .map_err(|error| format!("Failed to extract DOCX media: {error}"))?;
        if copied > limit {
            return Err(format!(
                "DOCX media entry '{file_name}' exceeds the {limit} byte decompressed safety limit"
            ));
        }
        output
            .flush()
            .map_err(|error| format!("Failed to flush DOCX media artifact: {error}"))?;
        total_bytes = total_bytes.saturating_add(copied);
        added.push((rel_path, file_name.to_string(), "figures".to_string()));
    }
    if !added.is_empty() {
        let parent = destination
            .parent()
            .ok_or("DOCX media destination has no parent")?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create DOCX artifact directory: {error}"))?;
        if destination.exists() {
            return Err(format!(
                "DOCX media destination already exists: {}",
                destination.display()
            ));
        }
        fs::rename(&staged_destination, &destination)
            .map_err(|error| format!("Failed to publish DOCX media artifacts: {error}"))?;
    }
    Ok(added)
}

fn copy_docx_media(path: &Path, run_dir: &Path) -> Result<Vec<(String, String, String)>, String> {
    copy_docx_media_with_limits(
        path,
        run_dir,
        MAX_DOCX_MEDIA_BYTES,
        MAX_DOCX_TOTAL_MEDIA_BYTES,
        MAX_DOCX_MEDIA_FILES,
        MAX_DOCX_ARCHIVE_ENTRIES,
    )
}

pub fn companion_pdf(source_path: &str) -> Option<PathBuf> {
    let source = Path::new(source_path);
    if !source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("tex"))
    {
        return None;
    }
    let parent = source.parent()?;
    let same_stem = source.with_extension("pdf");
    if same_stem.is_file() {
        return Some(same_stem);
    }
    for name in ["paper.pdf", "main.pdf", "manuscript.pdf"] {
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn build(extraction: &ExtractionResult, run_dir: &Path) -> Result<BundleBuild, String> {
    let kind = source_kind(extraction);
    let mut added_artifacts = Vec::new();
    if kind == "latex" {
        added_artifacts.extend(copy_tex_assets(extraction, run_dir)?);
    } else if kind == "docx" {
        added_artifacts.extend(copy_docx_media(
            Path::new(&extraction.source_path),
            run_dir,
        )?);
    }

    let mut assets = scan_asset_dir(run_dir, "artifacts/pages", "page", &extraction.method);
    assets.extend(scan_asset_dir(
        run_dir,
        "artifacts/figures",
        "figure",
        &extraction.method,
    ));
    assets.extend(scan_asset_dir(
        run_dir,
        "artifacts/document/figures",
        "figure",
        &extraction.method,
    ));
    let mut page_assets: BTreeMap<u32, DocumentAsset> = BTreeMap::new();
    for asset in &assets {
        if asset.kind == "page" {
            if let Some(page) = asset.page {
                page_assets.insert(page, asset.clone());
            }
        }
    }
    let mut text_pages: Vec<u32> = split_pages(&extraction.text)
        .into_iter()
        .filter_map(|(page, _)| page)
        .collect();
    text_pages.extend(page_assets.keys().copied());
    text_pages.sort_unstable();
    text_pages.dedup();
    let pages = text_pages
        .into_iter()
        .map(|number| {
            let asset = page_assets.get(&number);
            DocumentPage {
                number,
                label: format!("Page {number}"),
                asset_id: asset.map(|asset| asset.id.clone()),
                width: asset.and_then(|asset| asset.width),
                height: asset.and_then(|asset| asset.height),
            }
        })
        .collect();

    let mut origins = vec![DocumentOrigin {
        id: "origin-primary".to_string(),
        kind: kind.clone(),
        path: extraction.source_path.clone(),
        role: "primary".to_string(),
    }];
    if let Some(pdf) = companion_pdf(&extraction.source_path) {
        origins.push(DocumentOrigin {
            id: "origin-companion-pdf".to_string(),
            kind: "pdf".to_string(),
            path: pdf.to_string_lossy().to_string(),
            role: "visual-companion".to_string(),
        });
    }
    let mut bundle = DocumentBundle {
        schema_version: DOCUMENT_BUNDLE_SCHEMA_VERSION.to_string(),
        bundle_id: format!("doc-{}", extraction.paper_hash),
        source_kind: kind.clone(),
        origins,
        pages,
        nodes: Vec::new(),
        assets,
        links: Vec::new(),
        extraction: DocumentExtraction {
            method: extraction.method.clone(),
            source_path: extraction.source_path.clone(),
            paper_hash: extraction.paper_hash.clone(),
        },
        quality: extraction
            .quality_notes
            .iter()
            .map(|message| DocumentQualityNote {
                severity: "warning".to_string(),
                scope: "extraction".to_string(),
                message: message.clone(),
            })
            .collect(),
    };
    let mut has_native_structure = false;
    if extraction.method == "marker" {
        if let Some(structure) =
            crate::pipeline::extract::read_marker_structure(&extraction.paper_hash)?
        {
            add_marker_structured_nodes(&mut bundle, &structure);
            has_native_structure = true;
        }
    } else if matches!(
        extraction.method.as_str(),
        "paddleocr-vl" | "paddleocr-vl-full"
    ) {
        if let Some(structure) = crate::pipeline::extract::read_paddle_structure_for_method(
            &extraction.paper_hash,
            &extraction.method,
        )? {
            add_paddle_structured_nodes(&mut bundle, &structure);
            has_native_structure = true;
        }
    }
    // Marker and Paddle already provide block-level semantics. Re-inferring
    // the same headings, equations, captions, and tables from their Markdown
    // doubles both meaning and payload size. Keep Markdown inference only as a
    // compatibility fallback when no native structure sidecar is available.
    if !has_native_structure {
        add_markdown_nodes(&mut bundle, &extraction.text, &extraction.method);
    }
    if kind == "latex" {
        add_tex_nodes(&mut bundle, &extraction.text, &extraction.method);
    } else if kind == "docx" {
        let parsed = parse_docx(Path::new(&extraction.source_path))?;
        for (index, grid) in parsed.tables.into_iter().enumerate() {
            let table = node(
                &bundle,
                "table",
                None,
                Some(format!("Word table {}", index + 1)),
                Some((index + 1).to_string()),
                grid.iter()
                    .map(|row| row.join(" | "))
                    .collect::<Vec<_>>()
                    .join("\n"),
                Vec::new(),
                vec![DocumentRepresentation {
                    format: "grid".to_string(),
                    content: serde_json::json!(grid),
                }],
                "ooxml",
            );
            bundle.nodes.push(table);
        }
        for (index, omml) in parsed.equations.into_iter().enumerate() {
            let equation = node(
                &bundle,
                "equation",
                None,
                Some(format!("Word equation {}", index + 1)),
                Some((index + 1).to_string()),
                xml_text(&omml),
                Vec::new(),
                vec![DocumentRepresentation {
                    format: "omml".to_string(),
                    content: serde_json::Value::String(omml),
                }],
                "ooxml",
            );
            bundle.nodes.push(equation);
        }
    }
    bundle.validate()?;
    Ok(BundleBuild {
        bundle,
        added_artifacts,
    })
}

impl DocumentBundle {
    pub fn enrich_from_orientation(&mut self, value: &serde_json::Value) {
        let Ok(orientation) = serde_json::from_value::<OrientationMap>(value.clone()) else {
            return;
        };
        for entry in orientation.tables_figures {
            let kind = entry.kind.to_ascii_lowercase();
            let normalized_kind = if kind.starts_with("fig") {
                "figure"
            } else if kind.starts_with("tab") {
                "table"
            } else {
                continue;
            };
            let matched_asset_ids =
                closest_assets(self, normalized_kind, entry.page, &entry.number);
            if let Some(existing) = self.nodes.iter_mut().find(|node| {
                node.kind == normalized_kind
                    && node.number.as_deref() == Some(entry.number.as_str())
            }) {
                if existing.page.is_none() {
                    existing.page = entry.page;
                }
                if existing.text.is_empty() {
                    existing.text = entry.caption_summary.clone();
                }
                if existing.asset_ids.is_empty() {
                    existing.asset_ids = matched_asset_ids;
                }
                existing.representations.push(DocumentRepresentation {
                    format: "orientation_summary".to_string(),
                    content: serde_json::json!({
                        "caption_summary": entry.caption_summary,
                        "what_it_shows": entry.what_it_shows,
                    }),
                });
                continue;
            }
            let created = node(
                self,
                normalized_kind,
                entry.page,
                Some(format!("{} {}", capitalize(normalized_kind), entry.number)),
                Some(entry.number),
                entry.caption_summary,
                matched_asset_ids,
                vec![DocumentRepresentation {
                    format: "orientation_summary".to_string(),
                    content: serde_json::json!({ "what_it_shows": entry.what_it_shows }),
                }],
                "orientation",
            );
            self.nodes.push(created);
        }
        for result in orientation.formal_results {
            let normalized = result.kind.to_ascii_lowercase();
            if self.nodes.iter().any(|node| {
                node.kind == normalized && node.number.as_deref() == Some(result.number.as_str())
            }) {
                continue;
            }
            let created = node(
                self,
                &normalized,
                result.page,
                Some(format!("{} {}", capitalize(&normalized), result.number)),
                Some(result.number),
                result.summary,
                Vec::new(),
                vec![DocumentRepresentation {
                    format: "orientation_summary".to_string(),
                    content: serde_json::json!({ "proof_location": result.proof_location }),
                }],
                "orientation",
            );
            self.nodes.push(created);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != DOCUMENT_BUNDLE_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported document bundle schema {}",
                self.schema_version
            ));
        }
        let mut ids = HashSet::new();
        for id in self
            .origins
            .iter()
            .map(|entry| entry.id.as_str())
            .chain(self.nodes.iter().map(|entry| entry.id.as_str()))
            .chain(self.assets.iter().map(|entry| entry.id.as_str()))
        {
            if id.is_empty() || !ids.insert(id) {
                return Err(format!(
                    "Document bundle contains an empty or duplicate id: {id}"
                ));
            }
        }
        let asset_ids: HashSet<&str> = self.assets.iter().map(|asset| asset.id.as_str()).collect();
        for asset in &self.assets {
            if !safe_relative_path(&asset.rel_path) {
                return Err(format!(
                    "Document bundle asset has an unsafe path: {}",
                    asset.rel_path
                ));
            }
        }
        for document_node in &self.nodes {
            for asset_id in &document_node.asset_ids {
                if !asset_ids.contains(asset_id.as_str()) {
                    return Err(format!(
                        "Document node {} references unknown asset {asset_id}",
                        document_node.id
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Failed to serialize document bundle: {error}"))?;
        if json.len() > MAX_DOCUMENT_BUNDLE_BYTES {
            return Err(format!(
                "Document bundle is {} MB; the durable artifact limit is {} MB",
                json.len().div_ceil(1024 * 1024),
                MAX_DOCUMENT_BUNDLE_BYTES / (1024 * 1024)
            ));
        }
        Ok(json)
    }

    /// Compact, model-facing structural index. The readable `document.md`
    /// already contains page and paragraph prose, so repeating those large
    /// node bodies in every reviewer's tool loop only inflates context. Keep
    /// semantic content (equations, results, captions, tables, figures) while
    /// omitting duplicate page/paragraph/footer nodes entirely.
    pub fn to_model_index_pretty(&self) -> Result<String, String> {
        let keep_node = |node: &DocumentNode| {
            !matches!(
                node.kind.as_str(),
                "page_text" | "text_block" | "page_footer"
            )
        };
        let mut retained_ids: HashSet<&str> = self
            .nodes
            .iter()
            .filter(|node| keep_node(node))
            .map(|node| node.id.as_str())
            .collect();
        retained_ids.extend(self.assets.iter().map(|asset| asset.id.as_str()));
        retained_ids.extend(self.origins.iter().map(|origin| origin.id.as_str()));
        let nodes = self
            .nodes
            .iter()
            .filter(|node| keep_node(node))
            .map(|node| ModelDocumentNode {
                id: &node.id,
                kind: &node.kind,
                order: node.order,
                page: node.page,
                parent_id: node
                    .parent_id
                    .as_deref()
                    .filter(|parent| retained_ids.contains(*parent)),
                label: node.label.as_deref(),
                number: node.number.as_deref(),
                text: (!node.text.is_empty()).then_some(node.text.as_str()),
                representations: (!node.representations.is_empty())
                    .then_some(node.representations.as_slice()),
                asset_ids: &node.asset_ids,
                provenance: &node.provenance,
            })
            .collect();
        let index = ModelDocumentIndex {
            format: "pipeline.document-index.v1",
            schema_version: &self.schema_version,
            bundle_id: &self.bundle_id,
            source_kind: &self.source_kind,
            origins: &self.origins,
            pages: &self.pages,
            nodes,
            assets: &self.assets,
            links: self
                .links
                .iter()
                .filter(|link| {
                    retained_ids.contains(link.from_id.as_str())
                        && retained_ids.contains(link.to_id.as_str())
                })
                .collect(),
            extraction: &self.extraction,
            quality: &self.quality,
        };
        let json = serde_json::to_string_pretty(&index)
            .map_err(|error| format!("Failed to serialize document index: {error}"))?;
        if json.len() > MAX_MODEL_DOCUMENT_INDEX_BYTES {
            return Err(format!(
                "Document index is {} MB; the model-readable limit is {} MB",
                json.len().div_ceil(1024 * 1024),
                MAX_MODEL_DOCUMENT_INDEX_BYTES / (1024 * 1024)
            ));
        }
        Ok(json)
    }

    pub fn to_jsonl(&self) -> Result<String, String> {
        let mut output = String::new();
        for document_node in &self.nodes {
            let line = serde_json::to_string(document_node)
                .map_err(|error| format!("Failed to serialize document node: {error}"))?;
            if line.len().saturating_add(1).saturating_add(output.len()) > MAX_DOCUMENT_BUNDLE_BYTES
            {
                return Err(format!(
                    "Document block stream exceeds the {} MB durable artifact limit",
                    MAX_DOCUMENT_BUNDLE_BYTES / (1024 * 1024)
                ));
            }
            output.push_str(&line);
            output.push('\n');
        }
        Ok(output)
    }

    fn markdown_preamble(&self) -> String {
        let counts = self.kind_counts();
        let mut output = format!(
            "# Document bundle\n\n\
             - Schema: `{}`\n\
             - Source kind: `{}`\n\
             - Extraction: `{}`\n\
             - Pages: {}\n\
             - Nodes: {}\n\
             - Assets: {}\n\n",
            self.schema_version,
            self.source_kind,
            self.extraction.method,
            self.pages.len(),
            self.nodes.len(),
            self.assets.len()
        );
        if !counts.is_empty() {
            output.push_str("## Structural inventory\n\n");
            for (kind, count) in counts {
                output.push_str(&format!("- {kind}: {count}\n"));
            }
            output.push('\n');
        }
        if !self.assets.is_empty() {
            output.push_str("## Visual assets\n\n");
            for asset in &self.assets {
                let page = asset
                    .page
                    .map(|page| format!(" (page {page})"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "- `{}` — {}{} — `{}`\n",
                    asset.id, asset.label, page, asset.rel_path
                ));
            }
            output.push('\n');
        }
        if !self.quality.is_empty() {
            output.push_str("## Extraction quality\n\n");
            for note in &self.quality {
                output.push_str(&format!(
                    "- **{} / {}:** {}\n",
                    note.severity, note.scope, note.message
                ));
            }
            output.push('\n');
        }
        output
    }

    pub fn to_markdown(&self) -> String {
        let mut output = self.markdown_preamble();
        output.push_str(
            "## Extracted document\n\n\
             Stable block markers below can be cross-referenced with `document_bundle.json`.\n\n",
        );
        for document_node in self.nodes.iter().filter(|node| node.kind == "page_text") {
            let page = document_node
                .page
                .map(|page| format!(" · page {page}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "<!-- DOCUMENT_NODE {}{} -->\n\n{}\n\n",
                document_node.id, page, document_node.text
            ));
        }
        output
    }

    /// Readable projection backed by the exact verified extraction text.
    /// Native Marker/Paddle bundles intentionally do not duplicate that text
    /// into page nodes, so callers that have the compatibility view should use
    /// this projection for `document.md` and model-facing primary text.
    pub fn to_markdown_with_text(&self, extracted_text: &str) -> String {
        let mut output = self.markdown_preamble();
        output.push_str(
            "## Extracted document\n\n\
             The exact verified extraction view follows. Structural references are in `document_bundle.json`.\n\n",
        );
        output.push_str(extracted_text);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output
    }

    pub fn kind_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for document_node in &self.nodes {
            *counts.entry(document_node.kind.clone()).or_insert(0) += 1;
        }
        counts
    }

    pub fn asset_map(&self) -> HashMap<&str, &DocumentAsset> {
        self.assets
            .iter()
            .map(|asset| (asset.id.as_str(), asset))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docx_entry_limit_uses_bytes_actually_read() {
        let mut exact = std::io::Cursor::new(b"12345678");
        assert_eq!(
            read_docx_entry_limited(&mut exact, 8, "test entry").unwrap(),
            b"12345678"
        );

        let mut oversized = std::io::Cursor::new(b"123456789");
        let error = read_docx_entry_limited(&mut oversized, 8, "test entry").unwrap_err();
        assert!(error.contains("decompressed safety limit"), "{error}");
    }

    #[test]
    fn docx_media_failure_publishes_no_partial_files() {
        use std::io::Write as _;
        let source = tempfile::tempdir().unwrap();
        let path = source.path().join("oversized.docx");
        let file = fs::File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("word/media/first.png", options).unwrap();
        archive.write_all(b"12345678").unwrap();
        archive
            .start_file("word/media/second.png", options)
            .unwrap();
        archive.write_all(b"abcdefgh").unwrap();
        archive.finish().unwrap();

        let run = tempfile::tempdir().unwrap();
        let error = copy_docx_media_with_limits(&path, run.path(), 16, 12, 10, 100).unwrap_err();
        assert!(error.contains("cumulative"), "{error}");
        assert!(!run.path().join("artifacts/document/figures").exists());
        assert!(fs::read_dir(run.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pipeline-docx-media-")));
    }

    #[test]
    fn docx_archive_entry_count_is_bounded_before_media_scan() {
        use std::io::Write as _;
        let source = tempfile::tempdir().unwrap();
        let path = source.path().join("many-entries.docx");
        let file = fs::File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..3 {
            archive
                .start_file(format!("metadata/{index}.xml"), options)
                .unwrap();
            archive.write_all(b"x").unwrap();
        }
        archive.finish().unwrap();

        let run = tempfile::tempdir().unwrap();
        let error = copy_docx_media_with_limits(&path, run.path(), 16, 64, 10, 2).unwrap_err();
        assert!(error.contains("contains 3 entries"), "{error}");
        assert!(!run.path().read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pipeline-docx-media-")));
    }

    #[test]
    fn jpeg_dimension_parser_reads_start_of_frame() {
        let bytes = [
            0xff, 0xd8, // SOI
            0xff, 0xc0, // baseline SOF
            0x00, 0x11, // segment length
            0x08, // precision
            0x04, 0xb0, // 1200px high
            0x03, 0xf0, // 1008px wide
            0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x00, 0x03, 0x11, 0x00,
        ];
        assert_eq!(jpeg_dimensions(&bytes), (Some(1008), Some(1200)));
    }

    fn extraction(text: &str) -> ExtractionResult {
        ExtractionResult {
            text: text.to_string(),
            method: "llm".to_string(),
            source_path: "/tmp/paper.pdf".to_string(),
            paper_hash: "0123456789abcdef".to_string(),
            quality_notes: Vec::new(),
        }
    }

    #[test]
    fn builds_page_and_structural_nodes() {
        let temp = tempfile::tempdir().unwrap();
        let build = build(
            &extraction(
                "<!-- PAGE 1 -->\n# Introduction\n\nText\n\n$$y=x$$\n\nFigure 1: Impulse response",
            ),
            temp.path(),
        )
        .unwrap();
        assert_eq!(build.bundle.schema_version, "1.0");
        assert!(build
            .bundle
            .nodes
            .iter()
            .any(|node| node.kind == "page_text"));
        assert!(build.bundle.nodes.iter().any(|node| node.kind == "section"));
        assert!(build
            .bundle
            .nodes
            .iter()
            .any(|node| node.kind == "equation"));
        assert!(build.bundle.nodes.iter().any(|node| node.kind == "figure"));
    }

    #[test]
    fn rejects_unsafe_asset_paths() {
        let mut bundle = build(&extraction("Text"), tempfile::tempdir().unwrap().path())
            .unwrap()
            .bundle;
        bundle.assets.push(DocumentAsset {
            id: "bad".to_string(),
            kind: "figure".to_string(),
            label: "Bad".to_string(),
            rel_path: "../outside.png".to_string(),
            media_type: "image/png".to_string(),
            page: None,
            width: None,
            height: None,
            provenance: DocumentProvenance {
                origin_id: "origin-primary".to_string(),
                method: "test".to_string(),
                confidence: None,
            },
        });
        assert!(bundle.validate().is_err());
    }

    #[test]
    fn markdown_view_contains_stable_node_markers() {
        let bundle = build(
            &extraction("A complete paragraph."),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        let markdown = bundle.to_markdown();
        assert!(markdown.contains("DOCUMENT_NODE page_text-00001"));
        assert!(markdown.contains("A complete paragraph."));
    }

    #[test]
    fn readable_projection_can_use_exact_text_without_page_node_duplication() {
        let mut bundle = build(
            &extraction("Compatibility text"),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        bundle.nodes.retain(|node| node.kind != "page_text");

        let markdown = bundle.to_markdown_with_text("<!-- PAGE 1 -->\nExact native text");
        assert!(markdown.contains("Exact native text"));
        assert!(!markdown.contains("DOCUMENT_NODE page_text"));
    }

    #[test]
    fn canonical_bundle_serialization_enforces_artifact_limit() {
        let mut bundle = build(
            &extraction("Compatibility text"),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        bundle.nodes[0].text = "x".repeat(MAX_DOCUMENT_BUNDLE_BYTES);

        assert!(bundle
            .to_json_pretty()
            .unwrap_err()
            .contains("durable artifact limit"));
        assert!(bundle
            .to_jsonl()
            .unwrap_err()
            .contains("durable artifact limit"));
    }

    #[test]
    fn model_index_enforces_direct_provider_limit() {
        let mut bundle = build(
            &extraction("Compatibility text"),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        bundle.nodes[0].kind = "equation".to_string();
        bundle.nodes[0].text = "x".repeat(MAX_MODEL_DOCUMENT_INDEX_BYTES);

        assert!(bundle
            .to_model_index_pretty()
            .unwrap_err()
            .contains("model-readable limit"));
    }

    #[test]
    fn model_index_omits_duplicate_prose_but_keeps_semantic_content() {
        let bundle = build(
            &extraction("<!-- PAGE 1 -->\nA paragraph.\n\n$$y=x$$"),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        let index: serde_json::Value =
            serde_json::from_str(&bundle.to_model_index_pretty().unwrap()).unwrap();
        let nodes = index["nodes"].as_array().unwrap();
        let equation = nodes
            .iter()
            .find(|node| node["kind"] == "equation")
            .unwrap();

        assert!(!nodes.iter().any(|node| node["kind"] == "page_text"));
        assert_eq!(equation["text"], "$$y=x$$");
        assert_eq!(index["format"], "pipeline.document-index.v1");
    }

    #[test]
    fn marker_structure_preserves_semantic_and_uncertain_blocks() {
        let mut bundle = build(
            &extraction("<!-- PAGE 1 -->\nA complete paragraph."),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        let structure = crate::pipeline::extract::MarkerStructure {
            schema_version: 1,
            quality_notes: Vec::new(),
            pages: vec![crate::pipeline::extract::MarkerStructuredPage {
                number: 1,
                marker_id: "/page/0/Page/0".to_string(),
                polygon: vec![
                    vec![0.0, 0.0],
                    vec![612.0, 0.0],
                    vec![612.0, 792.0],
                    vec![0.0, 792.0],
                ],
                bbox: vec![0.0, 0.0, 612.0, 792.0],
                blocks: vec![
                    crate::pipeline::extract::MarkerStructuredBlock {
                        marker_id: "/page/0/PageHeader/0".to_string(),
                        marker_type: "PageHeader".to_string(),
                        role: "page_header".to_string(),
                        html: "<p>Running title</p>".to_string(),
                        text: "Running title".to_string(),
                        polygon: Vec::new(),
                        bbox: vec![72.0, 20.0, 540.0, 40.0],
                        image_files: Vec::new(),
                    },
                    crate::pipeline::extract::MarkerStructuredBlock {
                        marker_id: "/page/0/Text/1".to_string(),
                        marker_type: "Text".to_string(),
                        role: "possible_footnote".to_string(),
                        html: "<p>1 Qualification</p>".to_string(),
                        text: "1 Qualification".to_string(),
                        polygon: Vec::new(),
                        bbox: vec![72.0, 680.0, 540.0, 720.0],
                        image_files: Vec::new(),
                    },
                ],
            }],
        };
        add_marker_structured_nodes(&mut bundle, &structure);

        let header = bundle
            .nodes
            .iter()
            .find(|node| node.kind == "page_header")
            .unwrap();
        assert_eq!(header.text, "Running title");
        let possible = bundle
            .nodes
            .iter()
            .find(|node| node.kind == "possible_footnote")
            .unwrap();
        assert_eq!(possible.number.as_deref(), Some("1"));
        assert_eq!(possible.provenance.confidence, Some(0.6));
        assert!(possible
            .representations
            .iter()
            .any(|representation| representation.format == "marker_block"));
    }

    #[test]
    fn paddle_structure_preserves_roles_exact_markdown_and_page_evidence() {
        let mut bundle = build(
            &extraction("<!-- PAGE 2 -->\nA complete paragraph."),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        let structure = crate::pipeline::extract::PaddleStructure {
            schema_version: 1,
            parser: "paddleocr-vl-fast".to_string(),
            parser_version: "1.6".to_string(),
            settings: serde_json::Value::Null,
            quality_notes: Vec::new(),
            pages: vec![crate::pipeline::extract::PaddleStructuredPage {
                number: 2,
                markdown: String::new(),
                source_text_chars: None,
                width: None,
                height: None,
                blocks: vec![
                    crate::pipeline::extract::PaddleStructuredBlock {
                        block_id: "paddle-page-0002-block-0001".to_string(),
                        role: "page_header".to_string(),
                        block_label: String::new(),
                        markdown: "Running title".to_string(),
                        text: "Running title".to_string(),
                        boundary: Some("top".to_string()),
                        note_marker: None,
                        order: Some(1),
                        bbox: Vec::new(),
                        polygon: Vec::new(),
                        confidence: None,
                        asset_files: Vec::new(),
                        raw: serde_json::Value::Null,
                    },
                    crate::pipeline::extract::PaddleStructuredBlock {
                        block_id: "paddle-page-0002-block-0003".to_string(),
                        role: "possible_footnote".to_string(),
                        block_label: String::new(),
                        markdown: "2 A qualification with *source emphasis*.".to_string(),
                        text: "2 A qualification with source emphasis.".to_string(),
                        boundary: Some("bottom".to_string()),
                        note_marker: Some("2".to_string()),
                        order: Some(2),
                        bbox: Vec::new(),
                        polygon: Vec::new(),
                        confidence: None,
                        asset_files: Vec::new(),
                        raw: serde_json::Value::Null,
                    },
                ],
            }],
        };
        add_paddle_structured_nodes(&mut bundle, &structure);

        let header = bundle
            .nodes
            .iter()
            .find(|node| node.kind == "page_header")
            .unwrap();
        assert_eq!(header.text, "Running title");
        assert_eq!(header.provenance.confidence, Some(0.8));
        let possible = bundle
            .nodes
            .iter()
            .find(|node| node.kind == "possible_footnote")
            .unwrap();
        assert_eq!(possible.number.as_deref(), Some("2"));
        assert_eq!(possible.provenance.confidence, Some(0.6));
        let representation = possible
            .representations
            .iter()
            .find(|representation| representation.format == "paddle_block")
            .unwrap();
        assert_eq!(
            representation.content["markdown"],
            "2 A qualification with *source emphasis*."
        );
    }

    #[test]
    fn full_paddle_structure_preserves_layout_semantics_and_merges_formula_numbers() {
        let mut bundle = build(
            &extraction("<!-- PAGE 1 -->\n# Results\n\nA table follows."),
            tempfile::tempdir().unwrap().path(),
        )
        .unwrap()
        .bundle;
        let block = |id: &str, label: &str, markdown: &str, order: u32, confidence: f32| {
            crate::pipeline::extract::PaddleStructuredBlock {
                block_id: id.to_string(),
                role: label.to_string(),
                block_label: label.to_string(),
                markdown: markdown.to_string(),
                text: markdown.trim_start_matches('#').trim().to_string(),
                boundary: None,
                note_marker: None,
                order: Some(order),
                bbox: vec![10.0, order as f64 * 20.0, 500.0, order as f64 * 20.0 + 18.0],
                polygon: Vec::new(),
                confidence: Some(confidence),
                asset_files: Vec::new(),
                raw: serde_json::json!({ "block_order": order }),
            }
        };
        let structure = crate::pipeline::extract::PaddleStructure {
            schema_version: 2,
            parser: "paddleocr-vl-full".to_string(),
            parser_version: "3.7.0".to_string(),
            settings: serde_json::json!({ "merge_tables": true, "relevel_titles": true }),
            quality_notes: Vec::new(),
            pages: vec![crate::pipeline::extract::PaddleStructuredPage {
                number: 1,
                markdown: "# Results\n\n## Baseline\n\n| x | y |".to_string(),
                source_text_chars: None,
                width: Some(1200),
                height: Some(1600),
                blocks: vec![
                    block("title", "doc_title", "# Results", 1, 0.98),
                    block("subtitle", "paragraph_title", "## Baseline", 2, 0.94),
                    block("table", "table", "| x | y |\n|---|---|\n|1|2|", 3, 0.91),
                    block("formula", "formula", r"$$y=\beta x$$", 4, 0.89),
                    block("formula-number", "formula_number", "(1)", 5, 0.96),
                    block("caption", "vision_footnote", "Figure 1. Fit", 6, 0.87),
                    block("orphan-number", "formula_number", "(99)", 7, 0.82),
                    block("prose", "text", "Not an equation", 8, 0.90),
                    block("leading-number", "formula_number", r"$[A.2]$", 9, 0.95),
                    block("formula-2", "display_formula", r"$$z=\gamma x$$", 10, 0.88),
                ],
            }],
        };
        add_paddle_structured_nodes(&mut bundle, &structure);

        let title = bundle
            .nodes
            .iter()
            .find(|node| node.text == "Results" && node.provenance.method == "paddleocr-vl-full")
            .unwrap();
        let subtitle = bundle
            .nodes
            .iter()
            .find(|node| node.text == "Baseline" && node.provenance.method == "paddleocr-vl-full")
            .unwrap();
        let table = bundle
            .nodes
            .iter()
            .find(|node| node.kind == "table")
            .unwrap();
        let equations: Vec<&DocumentNode> = bundle
            .nodes
            .iter()
            .filter(|node| node.kind == "equation" && node.provenance.method == "paddleocr-vl-full")
            .collect();
        let equation = equations[0];
        let caption = bundle
            .nodes
            .iter()
            .find(|node| node.text == "Figure 1. Fit")
            .unwrap();
        assert_eq!(title.kind, "section");
        assert_eq!(subtitle.parent_id.as_deref(), Some(title.id.as_str()));
        assert_eq!(table.parent_id.as_deref(), Some(subtitle.id.as_str()));
        assert_eq!(equations.len(), 2);
        assert_eq!(equations[0].number.as_deref(), Some("1"));
        assert_eq!(equations[1].number.as_deref(), Some("A.2"));
        assert_eq!(equations[0].representations.len(), 2);
        assert_eq!(equations[1].representations.len(), 2);
        assert_eq!(
            equations[0].representations[1].content["block_label"],
            "formula_number"
        );
        assert!(bundle.nodes.iter().any(|node| {
            node.kind == "text_block"
                && node.text == "(99)"
                && node.provenance.method == "paddleocr-vl-full"
        }));
        assert_eq!(equation.provenance.method, "paddleocr-vl-full");
        assert_eq!(caption.kind, "caption");
        assert_eq!(table.provenance.confidence, Some(0.91));
        assert_eq!(table.representations[0].content["bbox"][0], 10.0);
        assert_eq!(
            table.representations[0].content["parser_settings"]["merge_tables"],
            true
        );
    }

    #[test]
    fn docx_preserves_tables_equations_and_media() {
        use std::io::Write as _;
        let source_dir = tempfile::tempdir().unwrap();
        let path = source_dir.path().join("paper.docx");
        let file = fs::File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("word/document.xml", options).unwrap();
        archive
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
                <w:document xmlns:w="w" xmlns:m="m"><w:body>
                  <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>
                  <w:p><w:r><w:t>Aggregate output falls.</w:t></w:r><m:oMath><m:r><m:t>y_t=x_t</m:t></m:r></m:oMath></w:p>
                  <w:tbl><w:tr><w:tc><w:p><w:r><w:t>Variable</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Estimate</w:t></w:r></w:p></w:tc></w:tr>
                  <w:tr><w:tc><w:p><w:r><w:t>Output</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>-0.2</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
                </w:body></w:document>"#,
            )
            .unwrap();
        archive
            .start_file("word/media/image1.png", options)
            .unwrap();
        let mut png = vec![0u8; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[16..20].copy_from_slice(&640u32.to_be_bytes());
        png[20..24].copy_from_slice(&480u32.to_be_bytes());
        archive.write_all(&png).unwrap();
        archive.finish().unwrap();

        let text = extract_docx_text(&path).unwrap();
        assert!(text.contains("# Introduction"));
        assert!(text.contains("| Variable | Estimate |"));

        let run = tempfile::tempdir().unwrap();
        let build = build(
            &ExtractionResult {
                text,
                method: "docx".to_string(),
                source_path: path.to_string_lossy().to_string(),
                paper_hash: "0123456789abcdef".to_string(),
                quality_notes: Vec::new(),
            },
            run.path(),
        )
        .unwrap();
        assert!(build.bundle.nodes.iter().any(|node| {
            node.kind == "equation"
                && node
                    .representations
                    .iter()
                    .any(|representation| representation.format == "omml")
        }));
        assert!(build.bundle.nodes.iter().any(|node| {
            node.kind == "table"
                && node
                    .representations
                    .iter()
                    .any(|representation| representation.format == "grid")
        }));
        assert_eq!(build.bundle.assets.len(), 1);
        assert_eq!(build.bundle.assets[0].width, Some(640));
    }
}
