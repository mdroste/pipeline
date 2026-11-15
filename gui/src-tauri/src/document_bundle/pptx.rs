//! Native PowerPoint ingestion. Presentation relationships define slide order;
//! numeric ZIP filenames do not. XML and media are read with cumulative limits,
//! and relationship targets never authorize filesystem or network reads.
use super::*;
use quick_xml::{events::Event, Reader};

const MAX_XML_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SLIDES: usize = 2_000;

type Archive = zip::ZipArchive<std::fs::File>;

fn open(path: &Path) -> Result<Archive, String> {
    let archive = zip::ZipArchive::new(crate::safety::open_regular_file(path)?)
        .map_err(|e| format!("Invalid PowerPoint archive: {e}"))?;
    if archive.len() > MAX_DOCX_ARCHIVE_ENTRIES {
        return Err("PowerPoint archive has too many entries".into());
    }
    Ok(archive)
}

fn xml(archive: &mut Archive, name: &str, remaining: &mut u64) -> Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("Missing PowerPoint part {name}: {e}"))?;
    let bytes =
        adapters::read_docx_entry_limited(&mut entry, (*remaining).min(8 * 1024 * 1024), name)?;
    *remaining -= bytes.len() as u64;
    String::from_utf8(bytes).map_err(|e| format!("Invalid PowerPoint XML encoding: {e}"))
}

fn walk(
    source: &str,
    mut visit: impl FnMut(&Event<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let mut reader = Reader::from_str(source);
    let mut depth = 0usize;
    let mut events = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("Invalid PowerPoint XML: {e}"))?;
        events += 1;
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::DocType(_) => return Err("PowerPoint XML document types are unsupported".into()),
            Event::Eof => {
                return if depth == 0 {
                    Ok(())
                } else {
                    Err("Unclosed PowerPoint XML element".into())
                };
            }
            _ => {}
        }
        if depth > 128 || events > 500_000 {
            return Err("PowerPoint XML exceeds structural limits".into());
        }
        visit(&event)?;
    }
}

fn attributes(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<HashMap<String, String>, String> {
    event
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(|e| e.to_string())?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|e| e.to_string())?
                .into_owned();
            Ok((key, value))
        })
        .collect()
}

/// Resolve OPC package targets, allowing parent components within the ZIP only.
fn target(part: &str, raw: &str) -> Option<String> {
    if raw.contains(['\\', ':', '#', '?']) {
        return None;
    }
    let mut components: Vec<&str> = if raw.starts_with('/') {
        vec![]
    } else {
        part.rsplit_once('/')?.0.split('/').collect()
    };
    for component in raw.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            value => components.push(value),
        }
    }
    Some(components.join("/"))
}

#[derive(Clone)]
struct Relationship {
    id: String,
    kind: String,
    target: String,
}

fn relationships(
    archive: &mut Archive,
    part: &str,
    remaining: &mut u64,
) -> Result<Vec<Relationship>, String> {
    let (dir, name) = part.rsplit_once('/').ok_or("Invalid PowerPoint part")?;
    let rel_name = format!("{dir}/_rels/{name}.rels");
    if archive.index_for_name(&rel_name).is_none() {
        return Ok(vec![]);
    }
    let source = xml(archive, &rel_name, remaining)?;
    let mut result = Vec::new();
    walk(&source, |event| {
        if let Event::Start(e) | Event::Empty(e) = event {
            if e.local_name().as_ref() == b"Relationship" {
                let attrs = attributes(e)?;
                if attrs.get("TargetMode").is_some_and(|v| v == "External") {
                    return Ok(());
                }
                if let (Some(id), Some(kind), Some(raw)) =
                    (attrs.get("Id"), attrs.get("Type"), attrs.get("Target"))
                {
                    let target =
                        target(part, raw).ok_or("Unsafe PowerPoint relationship target")?;
                    result.push(Relationship {
                        id: id.clone(),
                        kind: kind.clone(),
                        target,
                    });
                }
            }
        }
        Ok(())
    })?;
    Ok(result)
}

fn text(source: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut in_text = false;
    walk(source, |event| {
        match event {
            Event::Start(e) if e.local_name().as_ref() == b"t" => in_text = true,
            Event::End(e) if e.local_name().as_ref() == b"t" => in_text = false,
            Event::End(e) if e.local_name().as_ref() == b"p" => result.push('\n'),
            Event::End(e) if e.local_name().as_ref() == b"tc" => result.push('\t'),
            Event::Empty(e) if e.local_name().as_ref() == b"br" => result.push('\n'),
            Event::Text(e) if in_text => result.push_str(&e.decode().map_err(|e| e.to_string())?),
            Event::CData(e) if in_text => result.push_str(&e.decode().map_err(|e| e.to_string())?),
            Event::GeneralRef(e) if in_text => {
                let encoded = format!("&{};", e.decode().map_err(|e| e.to_string())?);
                result.push_str(&quick_xml::escape::unescape(&encoded).map_err(|e| e.to_string())?);
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok(result.trim().to_string())
}

struct Slide {
    text: String,
    media: Vec<String>,
}

fn slides(archive: &mut Archive) -> Result<Vec<Slide>, String> {
    let mut remaining = MAX_XML_BYTES;
    let presentation = xml(archive, "ppt/presentation.xml", &mut remaining)?;
    let rels = relationships(archive, "ppt/presentation.xml", &mut remaining)?;
    let mut order = Vec::new();
    walk(&presentation, |event| {
        if let Event::Start(e) | Event::Empty(e) = event {
            if e.local_name().as_ref() == b"sldId" {
                let attrs = attributes(e)?;
                let mut ids = attrs.iter().filter(|(key, _)| key.ends_with(":id"));
                let (_, id) = ids
                    .next()
                    .ok_or("PowerPoint slide has no relationship ID")?;
                if ids.next().is_some() {
                    return Err("PowerPoint slide has ambiguous relationship IDs".into());
                }
                let rel = rels
                    .iter()
                    .find(|rel| &rel.id == id && rel.kind.ends_with("/slide"))
                    .ok_or("PowerPoint slide relationship is missing")?;
                order.push(rel.target.clone());
                if order.len() > MAX_SLIDES {
                    return Err("PowerPoint deck has too many slides".into());
                }
            }
        }
        Ok(())
    })?;
    if order.is_empty() {
        return Err("PowerPoint deck contains no slides".into());
    }
    let mut result = Vec::new();
    for part in order {
        let mut slide_text = text(&xml(archive, &part, &mut remaining)?)?;
        let rels = relationships(archive, &part, &mut remaining)?;
        for notes in rels.iter().filter(|rel| rel.kind.ends_with("/notesSlide")) {
            let notes = text(&xml(archive, &notes.target, &mut remaining)?)?;
            if !notes.is_empty() {
                slide_text.push_str(&format!(
                    "\n\nSpeaker notes (not visible on slide):\n{notes}"
                ));
            }
        }
        let media = rels
            .into_iter()
            .filter(|rel| rel.kind.ends_with("/image"))
            .map(|rel| rel.target)
            .collect();
        result.push(Slide {
            text: slide_text,
            media,
        });
    }
    Ok(result)
}

pub fn extract_pptx_text(path: &Path) -> Result<String, String> {
    let slides = slides(&mut open(path)?)?;
    Ok(slides
        .iter()
        .enumerate()
        .map(|(index, slide)| {
            let number = index + 1;
            format!(
                "<!-- PAGE {number} -->\n# Slide {number}\n\n{}\n",
                slide.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub(super) fn add_media(
    bundle: &mut DocumentBundle,
    run_dir: &Path,
) -> Result<Vec<(String, String, String)>, String> {
    let mut archive = open(Path::new(&bundle.extraction.source_path))?;
    let slides = slides(&mut archive)?;
    let staging = tempfile::Builder::new()
        .prefix(".pipeline-pptx-media-")
        .tempdir_in(run_dir)
        .map_err(|e| format!("Cannot stage PowerPoint media: {e}"))?;
    let mut remaining = MAX_DOCX_TOTAL_MEDIA_BYTES;
    let mut artifacts = Vec::new();
    let mut copied = HashMap::<String, String>::new();
    for (index, slide) in slides.into_iter().enumerate() {
        for name in slide.media.into_iter().collect::<BTreeSet<_>>() {
            let extension = Path::new(&name)
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
                continue;
            }
            let rel = if let Some(rel) = copied.get(&name) {
                rel.clone()
            } else {
                if copied.len() >= MAX_DOCX_MEDIA_FILES {
                    return Err("PowerPoint has too many images".into());
                }
                let mut entry = archive
                    .by_name(&name)
                    .map_err(|e| format!("Missing PowerPoint image: {e}"))?;
                let bytes = adapters::read_docx_entry_limited(
                    &mut entry,
                    remaining.min(MAX_DOCX_MEDIA_BYTES),
                    &name,
                )?;
                remaining -= bytes.len() as u64;
                let rel = format!(
                    "artifacts/document/figures/pptx-{}.{}",
                    stable_suffix(&name),
                    extension
                );
                let path = staging.path().join(&rel);
                fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                fs::write(&path, bytes).map_err(|e| e.to_string())?;
                artifacts.push((rel.clone(), name.clone(), "figures".into()));
                copied.insert(name.clone(), rel.clone());
                rel
            };
            if let Some(mut asset) = asset_from_file(
                staging.path(),
                &rel,
                "figure",
                format!("Slide {} image", index + 1),
                Some(index as u32 + 1),
                "pptx",
            ) {
                asset.id = format!("{}-slide-{}", asset.id, index + 1);
                for node in bundle
                    .nodes
                    .iter_mut()
                    .filter(|node| node.page == Some(index as u32 + 1))
                {
                    node.asset_ids.push(asset.id.clone());
                }
                bundle.assets.push(asset);
            }
        }
    }
    if !artifacts.is_empty() {
        let relative = "artifacts/document/figures";
        let destination = run_dir.join(relative);
        if destination.exists() {
            return Err("PowerPoint media destination already exists".into());
        }
        fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::rename(staging.path().join(relative), destination).map_err(|e| e.to_string())?;
    }
    Ok(artifacts)
}

#[cfg(test)]
mod tests;
