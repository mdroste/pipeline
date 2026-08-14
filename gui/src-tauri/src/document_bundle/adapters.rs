use super::*;

pub(super) fn is_markdown_separator(line: &str) -> bool {
    let trimmed = line.trim().trim_matches('|');
    !trimmed.is_empty()
        && trimmed
            .split('|')
            .all(|cell| cell.trim().trim_matches(':').chars().all(|ch| ch == '-'))
}

pub(super) fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

pub(super) fn closest_assets(
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

pub(super) fn add_tex_nodes(bundle: &mut DocumentBundle, source: &str, method: &str) {
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

pub(super) fn copy_tex_assets(
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

pub(super) fn xml_text(fragment: &str) -> String {
    let text_re = Regex::new(r"(?s)<(?:w|m):t(?:\s[^>]*)?>(.*?)</(?:w|m):t>").unwrap();
    text_re
        .captures_iter(fragment)
        .filter_map(|capture| capture.get(1))
        .map(|value| xml_decode(value.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn read_docx_entry_limited(
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
pub(super) struct ParsedDocx {
    pub(super) markdown: String,
    pub(super) tables: Vec<Vec<Vec<String>>>,
    pub(super) equations: Vec<String>,
}

fn read_docx_xml(path: &Path) -> Result<String, String> {
    read_docx_optional_xml(path, "word/document.xml")?
        .ok_or_else(|| "DOCX is missing word/document.xml".to_string())
}

/// Read one XML part with the bounded entry reader. `Ok(None)` means the part
/// is absent (footnotes/endnotes are optional); an oversized or malformed
/// present part is an error the caller decides how to survive.
fn read_docx_optional_xml(path: &Path, name: &str) -> Result<Option<String>, String> {
    let file = crate::safety::open_regular_file(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("Invalid DOCX archive: {error}"))?;
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("Failed to open {name}: {error}")),
    };
    if entry.size() > MAX_DOCX_XML_BYTES {
        return Err(format!(
            "{name} exceeds the {} MB safety limit",
            MAX_DOCX_XML_BYTES / 1_000_000
        ));
    }
    let bytes = read_docx_entry_limited(&mut entry, MAX_DOCX_XML_BYTES, name)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| format!("{name} is not valid UTF-8: {error}"))
}

/// Collect the substantive note bodies from a footnotes/endnotes part.
/// Separator and continuation stubs (`w:type` other than `normal`) are layout
/// scaffolding, not notes.
fn docx_note_items(xml: &str, tag: &str) -> Vec<String> {
    let note_re = Regex::new(&format!(r"(?s)<w:{tag}(\s[^>]*)?>(.*?)</w:{tag}>")).unwrap();
    let type_re = Regex::new(r#"w:type="([^"]*)""#).unwrap();
    let paragraph_re = Regex::new(r"(?s)<w:p(?:\s[^>]*)?>(.*?)</w:p>").unwrap();
    let mut items = Vec::new();
    for capture in note_re.captures_iter(xml).take(1_000) {
        let attributes = capture.get(1).map(|value| value.as_str()).unwrap_or("");
        if type_re
            .captures(attributes)
            .is_some_and(|kind| &kind[1] != "normal")
        {
            continue;
        }
        let body = capture.get(2).map(|value| value.as_str()).unwrap_or("");
        let paragraphs: Vec<String> = paragraph_re
            .captures_iter(body)
            .map(|paragraph| xml_text(&paragraph[1]).trim().to_string())
            .filter(|text| !text.is_empty())
            .collect();
        let text = if paragraphs.is_empty() {
            xml_text(body).trim().to_string()
        } else {
            paragraphs.join(" ")
        };
        if !text.is_empty() {
            items.push(text);
        }
    }
    items
}

pub(super) fn parse_docx(path: &Path) -> Result<ParsedDocx, String> {
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

    // Footnotes and endnotes live in separate parts that word/document.xml
    // never references textually; without these sections they vanish from the
    // extraction entirely. A part that exists but cannot be read within
    // bounds costs its section, not the document — with an inline note, since
    // this stage has no other quality-note channel.
    for (name, heading, tag) in [
        ("word/footnotes.xml", "Footnotes", "footnote"),
        ("word/endnotes.xml", "Endnotes", "endnote"),
    ] {
        match read_docx_optional_xml(path, name) {
            Ok(Some(notes_xml)) => {
                let items = docx_note_items(&notes_xml, tag);
                if items.is_empty() {
                    continue;
                }
                markdown.push_str(&format!("## {heading}\n\n"));
                for (index, item) in items.iter().enumerate() {
                    markdown.push_str(&format!("{}. {item}\n", index + 1));
                }
                markdown.push('\n');
            }
            Ok(None) => {}
            Err(error) => {
                markdown.push_str(&format!(
                    "> Extraction note: {heading} were omitted ({error}).\n\n"
                ));
            }
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

pub(super) fn copy_docx_media_with_limits(
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

pub(super) fn copy_docx_media(
    path: &Path,
    run_dir: &Path,
) -> Result<Vec<(String, String, String)>, String> {
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
