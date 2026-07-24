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
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

pub const DOCUMENT_BUNDLE_SCHEMA_VERSION: &str = "1.0";
const MAX_DOCX_XML_BYTES: u64 = 25_000_000;
const MAX_DOCX_MEDIA_BYTES: u64 = 20_000_000;
const MAX_DOCX_TOTAL_MEDIA_BYTES: u64 = 150_000_000;
const MAX_DOCX_MEDIA_FILES: usize = 500;
const MAX_REPRESENTATION_BYTES: usize = 1_000_000;

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

fn png_dimensions(path: &Path) -> (Option<u32>, Option<u32>) {
    let Ok(mut file) = crate::safety::open_regular_file(path) else {
        return (None, None);
    };
    let mut header = [0u8; 24];
    if file.read_exact(&mut header).is_err() || &header[..8] != b"\x89PNG\r\n\x1a\n" {
        return (None, None);
    }
    (
        Some(u32::from_be_bytes(header[16..20].try_into().unwrap())),
        Some(u32::from_be_bytes(header[20..24].try_into().unwrap())),
    )
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
    let (width, height) = png_dimensions(&path);
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
    let mut xml = String::with_capacity(document.size() as usize);
    document
        .read_to_string(&mut xml)
        .map_err(|error| format!("DOCX document XML is not valid UTF-8: {error}"))?;
    Ok(xml)
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

fn copy_docx_media(path: &Path, run_dir: &Path) -> Result<Vec<(String, String, String)>, String> {
    let file = crate::safety::open_regular_file(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Invalid DOCX archive: {error}"))?;
    let destination = run_dir.join("artifacts/document/figures");
    let mut added = Vec::new();
    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        if added.len() >= MAX_DOCX_MEDIA_FILES {
            break;
        }
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Failed to inspect DOCX media: {error}"))?;
        let name = entry.name().to_string();
        if !name.starts_with("word/media/") || entry.is_dir() || entry.size() > MAX_DOCX_MEDIA_BYTES
        {
            continue;
        }
        total_bytes = total_bytes.saturating_add(entry.size());
        if total_bytes > MAX_DOCX_TOTAL_MEDIA_BYTES {
            break;
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
        fs::create_dir_all(&destination)
            .map_err(|error| format!("Failed to create DOCX media directory: {error}"))?;
        let safe_name = format!("{:03}_{}", added.len() + 1, file_name);
        let rel_path = format!("artifacts/document/figures/{safe_name}");
        let mut output = fs::File::create(run_dir.join(&rel_path))
            .map_err(|error| format!("Failed to create DOCX media artifact: {error}"))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| format!("Failed to extract DOCX media: {error}"))?;
        added.push((rel_path, file_name.to_string(), "figures".to_string()));
    }
    Ok(added)
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
    add_markdown_nodes(&mut bundle, &extraction.text, &extraction.method);
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
        serde_json::to_string_pretty(self)
            .map_err(|error| format!("Failed to serialize document bundle: {error}"))
    }

    pub fn to_jsonl(&self) -> Result<String, String> {
        let mut output = String::new();
        for document_node in &self.nodes {
            output.push_str(
                &serde_json::to_string(document_node)
                    .map_err(|error| format!("Failed to serialize document node: {error}"))?,
            );
            output.push('\n');
        }
        Ok(output)
    }

    pub fn to_markdown(&self) -> String {
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
