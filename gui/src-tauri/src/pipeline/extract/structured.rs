use super::*;

/// Where marker writes its output for a given paper, so the run can pick up
/// extracted figure images afterwards: ~/.pipeline/cache/marker/{hash}/.
pub fn marker_output_dir(paper_hash: &str) -> Option<PathBuf> {
    // Hashes are 16 lowercase hex chars (compute_hash); reject anything else
    // before using it as a path component.
    if paper_hash.len() != 16 || !paper_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("marker")
            .join(paper_hash),
    )
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct MarkerJsonDocument {
    #[serde(default)]
    block_type: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    children: Vec<MarkerJsonBlock>,
}

#[cfg(test)]
#[derive(Debug, Clone, Deserialize)]
pub(super) struct MarkerJsonBlock {
    #[serde(default)]
    id: String,
    #[serde(default)]
    block_type: String,
    #[serde(default)]
    html: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    polygon: Vec<Vec<f64>>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    bbox: Vec<f64>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    children: Vec<MarkerJsonBlock>,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    images: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructure {
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) quality_notes: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<MarkerStructuredPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructuredPage {
    pub(crate) number: u32,
    pub(crate) marker_id: String,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) blocks: Vec<MarkerStructuredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MarkerStructuredBlock {
    pub(crate) marker_id: String,
    pub(crate) marker_type: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) html: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) image_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructure {
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) parser: String,
    #[serde(default)]
    pub(crate) parser_version: String,
    #[serde(default)]
    pub(crate) settings: serde_json::Value,
    #[serde(default)]
    pub(crate) quality_notes: Vec<String>,
    #[serde(default)]
    pub(crate) pages: Vec<PaddleStructuredPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredPage {
    pub(crate) number: u32,
    #[serde(default)]
    pub(crate) markdown: String,
    /// Substantive recognition characters measured before Paddle's
    /// cross-page restructuring can move blocks between neighboring pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_text_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<u32>,
    #[serde(default)]
    pub(crate) blocks: Vec<PaddleStructuredBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PaddleStructuredBlock {
    pub(crate) block_id: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) block_label: String,
    #[serde(default)]
    pub(crate) markdown: String,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) boundary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note_marker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) order: Option<u32>,
    #[serde(default)]
    pub(crate) bbox: Vec<f64>,
    #[serde(default)]
    pub(crate) polygon: Vec<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f32>,
    #[serde(default)]
    pub(crate) asset_files: Vec<String>,
    #[serde(default)]
    pub(crate) raw: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug)]
pub(super) struct MarkerExtraction {
    pub(super) text: String,
    pub(super) quality_notes: Vec<String>,
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct MarkerImageBudget {
    files: usize,
    bytes: usize,
    materialized: HashMap<String, String>,
    warned: HashSet<String>,
}

pub(super) fn marker_structure_path(paper_hash: &str) -> Option<PathBuf> {
    Some(marker_output_dir(paper_hash)?.join(MARKER_STRUCTURE_FILE))
}

pub(crate) fn read_marker_structure(paper_hash: &str) -> Result<Option<MarkerStructure>, String> {
    let Some(path) = marker_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
    let structure: MarkerStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != MARKER_STRUCTURE_SCHEMA {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_marker_structure_json(paper_hash: &str) -> Result<Option<String>, String> {
    let Some(path) = marker_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES).map(Some)
}

pub(super) fn paddle_structure_path(paper_hash: &str) -> Option<PathBuf> {
    if paper_hash.len() != 16 || !paper_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("paddleocr-vl")
            .join(paper_hash)
            .join(PADDLE_STRUCTURE_FILE),
    )
}

pub(crate) fn read_paddle_structure(paper_hash: &str) -> Result<Option<PaddleStructure>, String> {
    let Some(path) = paddle_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if !(1..=PADDLE_STRUCTURE_SCHEMA).contains(&structure.schema_version) {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_paddle_structure_json(paper_hash: &str) -> Result<Option<String>, String> {
    let Some(path) = paddle_structure_path(paper_hash) else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES).map(Some)
}

pub(super) fn paddle_full_cache_root(paper_hash: &str) -> Option<PathBuf> {
    if paper_hash.len() != 16 || !paper_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        dirs::home_dir()?
            .join(".pipeline")
            .join("cache")
            .join("paddleocr-vl-full")
            .join(paper_hash),
    )
}

pub(super) fn valid_cache_fingerprint(value: &str) -> bool {
    value.len() == 16 && value.chars().all(|character| character.is_ascii_hexdigit())
}

pub(super) fn paddle_full_active_dir(paper_hash: &str) -> Result<Option<PathBuf>, String> {
    let Some(root) = paddle_full_cache_root(paper_hash) else {
        return Ok(None);
    };
    let active_path = root.join(PADDLE_FULL_ACTIVE_FILE);
    if !active_path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&active_path, 64 * 1024)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", active_path.display()))?;
    if value.get("schema").and_then(serde_json::Value::as_u64)
        != Some(EXTRACTION_CACHE_SCHEMA as u64)
    {
        return Ok(None);
    }
    let Some(fingerprint) = value.get("fingerprint").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    if !valid_cache_fingerprint(fingerprint) {
        return Err("PaddleOCR-VL full-parser cache has an invalid active fingerprint".to_string());
    }
    let directory = root.join(fingerprint);
    Ok(directory.is_dir().then_some(directory))
}

pub(crate) fn read_paddle_full_structure(
    paper_hash: &str,
) -> Result<Option<PaddleStructure>, String> {
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(None);
    };
    let path = directory.join(PADDLE_FULL_STRUCTURE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
    {
        return Ok(None);
    }
    Ok(Some(structure))
}

pub(crate) fn read_paddle_structure_for_method(
    paper_hash: &str,
    method: &str,
) -> Result<Option<PaddleStructure>, String> {
    if method == "paddleocr-vl-full" {
        read_paddle_full_structure(paper_hash)
    } else {
        read_paddle_structure(paper_hash)
    }
}

pub(crate) fn read_paddle_structure_json_for_method(
    paper_hash: &str,
    method: &str,
) -> Result<Option<String>, String> {
    if method != "paddleocr-vl-full" {
        return read_paddle_structure_json(paper_hash);
    }
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(None);
    };
    let path = directory.join(PADDLE_FULL_STRUCTURE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text = read_utf8_capped(&path, crate::pipeline::claude::MAX_STDOUT_BYTES)?;
    let structure: PaddleStructure = serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?;
    if structure.schema_version != PADDLE_STRUCTURE_SCHEMA
        || structure.parser != "paddleocr-vl-full"
    {
        return Ok(None);
    }
    Ok(Some(text))
}

pub(super) fn paddle_full_image_files(paper_hash: &str) -> Result<Vec<PathBuf>, String> {
    let Some(directory) = paddle_full_active_dir(paper_hash)? else {
        return Ok(Vec::new());
    };
    let assets = directory.join("assets");
    if !assets.is_dir() {
        return Ok(Vec::new());
    }
    let mut images = Vec::new();
    let mut walk = crate::safety::WalkBudget::new("PaddleOCR-VL full-parser image discovery");
    for entry in fs::read_dir(&assets)
        .map_err(|error| format!("Failed to read parser image directory: {error}"))?
    {
        walk.entry()?;
        let entry = entry.map_err(|error| format!("Failed to read parser image: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect parser image: {error}"))?;
        let path = entry.path();
        if file_type.is_file()
            && ["png", "jpg", "jpeg", "gif", "webp"]
                .iter()
                .any(|extension| ext_eq(&path, extension))
        {
            images.push(path);
        }
    }
    images.sort();
    Ok(images)
}

#[derive(Debug, Clone)]
pub(crate) struct PaddleFullImageArtifact {
    pub(crate) source_path: PathBuf,
    pub(crate) display_name: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PaddleFullImageInventory {
    pub(crate) artifacts: Vec<PaddleFullImageArtifact>,
    /// Number of paper-level labels (for example, `figure_1`) that referred
    /// to more than one distinct caption and therefore needed disambiguation.
    pub(crate) disambiguated_labels: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct PaddleFigureIdentity {
    kind: String,
    number: String,
    page: u32,
    caption_id: String,
}

pub(super) fn normalized_figure_number(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

pub(super) fn paddle_figure_identity(
    block: &PaddleStructuredBlock,
    page: u32,
) -> Option<PaddleFigureIdentity> {
    let label = if block.block_label.is_empty() {
        block.role.as_str()
    } else {
        block.block_label.as_str()
    }
    .to_ascii_lowercase();
    if !matches!(
        label.as_str(),
        "caption"
            | "figure_title"
            | "image_caption"
            | "chart_title"
            | "table_title"
            | "table_caption"
            | "paragraph_title"
            | "section_title"
            | "figure"
            | "chart"
            | "table"
    ) {
        return None;
    }
    let caption = if block.text.trim().is_empty() {
        block.markdown.as_str()
    } else {
        block.text.as_str()
    };
    let captures = PADDLE_NUMBERED_CAPTION.captures(caption)?;
    let raw_kind = captures.get(1)?.as_str().to_ascii_lowercase();
    let kind = if raw_kind.starts_with("fig") {
        "figure"
    } else {
        "table"
    };
    let number = normalized_figure_number(captures.get(2)?.as_str());
    if number.is_empty() {
        return None;
    }
    Some(PaddleFigureIdentity {
        kind: kind.to_string(),
        number,
        page,
        caption_id: block.block_id.clone(),
    })
}

pub(super) fn paddle_asset_page(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_prefix("page-")?.split('-').next()?;
    digits.parse().ok()
}

pub(super) fn paddle_identity_base(identity: &PaddleFigureIdentity) -> String {
    if identity.number.is_empty() {
        format!("figure_page_{:04}", identity.page)
    } else {
        format!("{}_{}", identity.kind, identity.number)
    }
}

pub(super) fn paddle_full_image_inventory_from(
    structure: &PaddleStructure,
    images: Vec<PathBuf>,
) -> PaddleFullImageInventory {
    let mut asset_identities: HashMap<String, PaddleFigureIdentity> = HashMap::new();
    let mut first_page_identity: HashMap<u32, PaddleFigureIdentity> = HashMap::new();
    for page in &structure.pages {
        let mut blocks: Vec<&PaddleStructuredBlock> = page.blocks.iter().collect();
        blocks.sort_by_key(|block| block.order.unwrap_or(u32::MAX));
        let captions: Vec<(u32, PaddleFigureIdentity)> = blocks
            .iter()
            .filter_map(|block| {
                paddle_figure_identity(block, page.number)
                    .map(|identity| (block.order.unwrap_or(u32::MAX), identity))
            })
            .collect();
        if let Some((_, identity)) = captions.first() {
            first_page_identity.insert(page.number, identity.clone());
        }
        for block in blocks {
            let block_order = block.order.unwrap_or(u32::MAX);
            let closest = captions.iter().min_by_key(|(caption_order, _)| {
                (
                    caption_order.abs_diff(block_order),
                    u8::from(*caption_order > block_order),
                )
            });
            if let Some((_, identity)) = closest {
                for asset in &block.asset_files {
                    if let Some(name) = Path::new(asset).file_name().and_then(|name| name.to_str())
                    {
                        asset_identities.insert(name.to_string(), identity.clone());
                    }
                }
            }
        }
    }

    let records: Vec<(PathBuf, PaddleFigureIdentity)> = images
        .into_iter()
        .map(|path| {
            let page = paddle_asset_page(&path).unwrap_or(0);
            let identity = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| asset_identities.get(name))
                .cloned()
                .or_else(|| first_page_identity.get(&page).cloned())
                .unwrap_or_else(|| PaddleFigureIdentity {
                    kind: "figure".to_string(),
                    number: String::new(),
                    page,
                    caption_id: format!("unlabeled-page-{page:04}"),
                });
            (path, identity)
        })
        .collect();

    let mut identities_by_base: HashMap<String, BTreeSet<PaddleFigureIdentity>> = HashMap::new();
    let mut assets_by_identity: HashMap<PaddleFigureIdentity, usize> = HashMap::new();
    for (_, identity) in &records {
        identities_by_base
            .entry(paddle_identity_base(identity))
            .or_default()
            .insert(identity.clone());
        *assets_by_identity.entry(identity.clone()).or_default() += 1;
    }
    let disambiguated_labels = identities_by_base
        .values()
        .filter(|identities| identities.len() > 1)
        .count();
    let identity_ordinals: HashMap<PaddleFigureIdentity, usize> = identities_by_base
        .values()
        .flat_map(|identities| {
            identities
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, identity)| (identity, index + 1))
                .collect::<Vec<_>>()
        })
        .collect();

    let mut seen_within_identity: HashMap<PaddleFigureIdentity, usize> = HashMap::new();
    let mut used_names = HashSet::new();
    let artifacts = records
        .into_iter()
        .map(|(source_path, identity)| {
            let base = paddle_identity_base(&identity);
            let identities = identities_by_base
                .get(&base)
                .map(BTreeSet::len)
                .unwrap_or(1);
            let mut stem = base;
            if identities > 1 {
                stem.push_str(&format!("_page_{:04}", identity.page));
                let same_page = identities_by_base
                    .values()
                    .find(|values| values.contains(&identity))
                    .map(|values| {
                        values
                            .iter()
                            .filter(|candidate| candidate.page == identity.page)
                            .count()
                    })
                    .unwrap_or(1);
                if same_page > 1 {
                    stem.push_str(&format!(
                        "_{}",
                        identity_ordinals.get(&identity).copied().unwrap_or(1)
                    ));
                }
            }
            let occurrence = seen_within_identity.entry(identity.clone()).or_default();
            *occurrence += 1;
            if assets_by_identity.get(&identity).copied().unwrap_or(1) > 1 {
                stem.push_str(&format!("_{occurrence}"));
            }
            let extension = source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("png")
                .to_ascii_lowercase();
            let mut display_name = format!("{stem}.{extension}");
            let mut collision = 2usize;
            while !used_names.insert(display_name.clone()) {
                display_name = format!("{stem}_{collision}.{extension}");
                collision += 1;
            }
            PaddleFullImageArtifact {
                source_path,
                display_name,
            }
        })
        .collect();
    PaddleFullImageInventory {
        artifacts,
        disambiguated_labels,
    }
}

pub(crate) fn paddle_full_image_inventory(
    paper_hash: &str,
) -> Result<PaddleFullImageInventory, String> {
    let images = paddle_full_image_files(paper_hash)?;
    let Some(structure) = read_paddle_full_structure(paper_hash)? else {
        return Ok(paddle_full_image_inventory_from(
            &PaddleStructure {
                schema_version: PADDLE_STRUCTURE_SCHEMA,
                parser: "paddleocr-vl-full".to_string(),
                parser_version: String::new(),
                settings: serde_json::Value::Null,
                quality_notes: Vec::new(),
                pages: Vec::new(),
            },
            images,
        ));
    };
    Ok(paddle_full_image_inventory_from(&structure, images))
}

#[cfg(test)]
pub(super) fn marker_type_name(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value)
}

#[cfg(test)]
pub(super) fn marker_block_html(block: &MarkerJsonBlock) -> String {
    if block.children.is_empty() {
        return block.html.clone();
    }
    let mut html = block.html.clone();
    let mut matched_child = false;
    for child in &block.children {
        let child_html = marker_block_html(child);
        let pattern = format!(
            r#"(?is)<content-ref\b[^>]*\bsrc\s*=\s*["']{}["'][^>]*>(?:\s*</content-ref\s*>)?"#,
            regex::escape(&child.id)
        );
        if let Ok(reference) = Regex::new(&pattern) {
            if reference.is_match(&html) {
                matched_child = true;
                html = reference
                    .replace_all(&html, child_html.as_str())
                    .into_owned();
            }
        }
    }
    if html.trim().is_empty() || (!matched_child && html.contains("content-ref")) {
        html = block
            .children
            .iter()
            .map(marker_block_html)
            .collect::<Vec<_>>()
            .join("\n");
    } else if html.contains("content-ref") {
        let unresolved = Regex::new(r"(?is)</?content-ref\b[^>]*>").unwrap();
        html = unresolved.replace_all(&html, "").into_owned();
    }
    html
}

#[cfg(test)]
pub(super) fn marker_review_html(html: &str) -> String {
    let display_math =
        Regex::new(r#"(?is)<math\b[^>]*display\s*=\s*["']block["'][^>]*>(.*?)</math>"#).unwrap();
    let inline_math = Regex::new(r"(?is)<math\b[^>]*>(.*?)</math>").unwrap();
    let with_display = display_math.replace_all(html, |capture: &regex::Captures<'_>| {
        format!("\n$$\n{}\n$$\n", marker_plain_text(&capture[1]))
    });
    let with_math = inline_math.replace_all(&with_display, |capture: &regex::Captures<'_>| {
        format!("${}$", marker_plain_text(&capture[1]))
    });
    ammonia::clean(&with_math).trim().to_string()
}

#[cfg(test)]
pub(super) fn marker_plain_text(html: &str) -> String {
    let cleaned = ammonia::clean(html);
    let tags = Regex::new(r"(?is)<[^>]+>").unwrap();
    let without_tags = tags.replace_all(&cleaned, " ");
    let entities = Regex::new(r"&(#x[0-9A-Fa-f]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp);").unwrap();
    let decoded = entities.replace_all(&without_tags, |capture: &regex::Captures<'_>| {
        let entity = &capture[1];
        match entity {
            "amp" => "&".to_string(),
            "lt" => "<".to_string(),
            "gt" => ">".to_string(),
            "quot" => "\"".to_string(),
            "apos" => "'".to_string(),
            "nbsp" => " ".to_string(),
            _ => {
                let value = entity
                    .strip_prefix("#x")
                    .and_then(|value| u32::from_str_radix(value, 16).ok())
                    .or_else(|| {
                        entity
                            .strip_prefix('#')
                            .and_then(|value| value.parse::<u32>().ok())
                    });
                value
                    .and_then(char::from_u32)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| capture[0].to_string())
            }
        }
    });
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
pub(super) fn collect_marker_images<'a>(
    block: &'a MarkerJsonBlock,
    output: &mut Vec<(&'a str, &'a str)>,
) {
    output.extend(
        block
            .images
            .iter()
            .map(|(id, content)| (id.as_str(), content.as_str())),
    );
    for child in &block.children {
        collect_marker_images(child, output);
    }
}

#[cfg(test)]
pub(super) fn marker_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("webp")
    } else {
        None
    }
}

#[cfg(test)]
pub(super) fn materialize_marker_images(
    block: &MarkerJsonBlock,
    page: u32,
    out_dir: &Path,
    budget: &mut MarkerImageBudget,
    quality_notes: &mut Vec<String>,
) -> Vec<String> {
    let mut encoded = Vec::new();
    collect_marker_images(block, &mut encoded);
    let mut files = Vec::new();
    for (marker_id, content) in encoded {
        if let Some(existing) = budget.materialized.get(marker_id) {
            files.push(existing.clone());
            continue;
        }
        if budget.files >= MAX_MARKER_IMAGES {
            if budget.warned.insert("count".to_string()) {
                quality_notes.push(format!(
                    "Marker emitted more than {MAX_MARKER_IMAGES} extracted images; additional images were omitted. Consult the rendered PDF pages."
                ));
            }
            continue;
        }
        let max_encoded_bytes = MAX_MARKER_IMAGE_BYTES.div_ceil(3).saturating_mul(4);
        if content.len() > max_encoded_bytes {
            if budget.warned.insert("individual".to_string()) {
                quality_notes.push(format!(
                    "A Marker image exceeded the {} MB safety limit and was omitted. Consult the rendered PDF page.",
                    MAX_MARKER_IMAGE_BYTES / 1024 / 1024
                ));
            }
            continue;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(content.as_bytes()) else {
            if budget.warned.insert("decode".to_string()) {
                quality_notes.push(
                    "Marker emitted an invalid encoded image; it was omitted. Consult the rendered PDF page."
                        .to_string(),
                );
            }
            continue;
        };
        if bytes.len() > MAX_MARKER_IMAGE_BYTES
            || budget.bytes.saturating_add(bytes.len()) > MAX_MARKER_IMAGE_TOTAL_BYTES
        {
            if budget.warned.insert("bytes".to_string()) {
                quality_notes.push(format!(
                    "Marker extracted-image materialization reached its {} MB safety budget; additional images were omitted. Consult the rendered PDF pages.",
                    MAX_MARKER_IMAGE_TOTAL_BYTES / 1024 / 1024
                ));
            }
            continue;
        }
        let Some(extension) = marker_image_extension(&bytes) else {
            if budget.warned.insert("format".to_string()) {
                quality_notes.push(
                    "Marker emitted an extracted image in an unrecognized format; it was omitted. Consult the rendered PDF page."
                        .to_string(),
                );
            }
            continue;
        };
        let suffix = format!("{:x}", Sha256::digest(marker_id.as_bytes()));
        let filename = format!("marker-page-{page:04}-{}.{}", &suffix[..12], extension);
        if atomic_write_cache(&out_dir.join(&filename), &bytes).is_err() {
            if budget.warned.insert("write".to_string()) {
                quality_notes.push(
                    "Pipeline could not retain one or more Marker images. Consult the rendered PDF pages."
                        .to_string(),
                );
            }
            continue;
        }
        budget.files += 1;
        budget.bytes = budget.bytes.saturating_add(bytes.len());
        budget
            .materialized
            .insert(marker_id.to_string(), filename.clone());
        files.push(filename);
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
pub(super) fn leading_footnote_marker(text: &str) -> Option<String> {
    let marker = Regex::new(r"^\s*(\d{1,3}|[*†‡])(?:[.)])?\s+").unwrap();
    marker
        .captures(text)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_string())
}

#[cfg(test)]
pub(super) fn preceding_superscript_anchor(blocks: &[MarkerStructuredBlock], marker: &str) -> bool {
    let pattern = format!(r"(?is)<sup\b[^>]*>\s*{}\s*</sup>", regex::escape(marker));
    let Ok(anchor) = Regex::new(&pattern) else {
        return false;
    };
    blocks
        .iter()
        .filter(|block| block.role == "body")
        .any(|block| anchor.is_match(&block.html))
}

#[cfg(test)]
pub(super) fn bbox_vertical_extent(bbox: &[f64]) -> Option<(f64, f64)> {
    if bbox.len() < 4 || !bbox[1].is_finite() || !bbox[3].is_finite() {
        return None;
    }
    Some((bbox[1].min(bbox[3]), bbox[1].max(bbox[3])))
}

#[cfg(test)]
pub(super) fn page_vertical_extent(page: &MarkerJsonBlock) -> Option<(f64, f64)> {
    bbox_vertical_extent(&page.bbox).or_else(|| {
        let mut values = page
            .polygon
            .iter()
            .filter_map(|point| point.get(1).copied())
            .filter(|value| value.is_finite());
        let first = values.next()?;
        let (min, max) = values.fold((first, first), |(min, max), value| {
            (min.min(value), max.max(value))
        });
        Some((min, max))
    })
}

#[cfg(test)]
pub(super) fn possible_footnote(
    blocks: &[MarkerStructuredBlock],
    index: usize,
    page_extent: Option<(f64, f64)>,
) -> bool {
    let block = &blocks[index];
    if block.marker_type != "Text" || block.text.len() > 2_000 {
        return false;
    }
    let Some(marker) = leading_footnote_marker(&block.text) else {
        return false;
    };
    if !preceding_superscript_anchor(&blocks[..index], &marker) {
        return false;
    }
    let Some((page_top, page_bottom)) = page_extent else {
        return false;
    };
    let page_height = page_bottom - page_top;
    let Some((block_top, _)) = bbox_vertical_extent(&block.bbox) else {
        return false;
    };
    if page_height <= 0.0 || (block_top - page_top) / page_height < 0.75 {
        return false;
    }
    let previous_bottom = blocks[..index]
        .iter()
        .rev()
        .filter(|candidate| candidate.role == "body")
        .find_map(|candidate| bbox_vertical_extent(&candidate.bbox).map(|(_, bottom)| bottom));
    previous_bottom.is_some_and(|bottom| block_top - bottom >= page_height * 0.012)
}

#[cfg(test)]
pub(super) fn marker_role(marker_type: &str) -> &'static str {
    match marker_type {
        "Footnote" => "footnote",
        "PageHeader" => "page_header",
        "PageFooter" => "page_footer",
        _ => "body",
    }
}

#[cfg(test)]
pub(super) fn marker_block_placeholder(marker_type: &str, page: u32) -> String {
    match marker_type {
        "Figure" | "FigureGroup" | "Picture" | "PictureGroup" => format!(
            "<p><em>[Visual block on page {page}; inspect the corresponding page or extracted figure asset.]</em></p>"
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
pub(super) fn render_marker_page(page: &MarkerStructuredPage) -> String {
    let mut output = format!("<!-- PAGE {} -->\n\n", page.number);
    let mut footnotes = Vec::new();
    for block in &page.blocks {
        match block.role.as_str() {
            "page_header" | "page_footer" => {}
            "footnote" => footnotes.push(block),
            "possible_footnote" => {
                output.push_str(&format!(
                    "<!-- BEGIN POSSIBLE_FOOTNOTE page={} marker_id={} -->\n\
                     <aside data-document-role=\"possible-footnote\">\n{}\n</aside>\n\
                     <!-- END POSSIBLE_FOOTNOTE page={} -->\n\n",
                    page.number,
                    &format!("{:x}", Sha256::digest(block.marker_id.as_bytes()))[..12],
                    block.html,
                    page.number
                ));
            }
            _ => {
                output.push_str(&block.html);
                output.push_str("\n\n");
            }
        }
    }
    if !footnotes.is_empty() {
        output.push_str(&format!("<!-- BEGIN FOOTNOTES page={} -->\n", page.number));
        for block in footnotes {
            output.push_str("<aside data-document-role=\"footnote\">\n");
            output.push_str(&block.html);
            output.push_str("\n</aside>\n");
        }
        output.push_str(&format!("<!-- END FOOTNOTES page={} -->\n", page.number));
    }
    output.trim().to_string()
}

#[cfg(test)]
pub(super) fn normalize_marker_json(raw: &str, out_dir: &Path) -> Result<MarkerExtraction, String> {
    let parsed: MarkerJsonDocument = serde_json::from_str(raw)
        .map_err(|error| format!("Failed to parse Marker JSON output: {error}"))?;
    if !parsed.block_type.is_empty() && marker_type_name(&parsed.block_type) != "Document" {
        return Err(format!(
            "Marker JSON returned {} instead of a Document",
            parsed.block_type
        ));
    }
    let mut quality_notes = Vec::new();
    let mut pages = Vec::new();
    let mut image_budget = MarkerImageBudget::default();
    for (page_index, page) in parsed.children.iter().enumerate() {
        if marker_type_name(&page.block_type) != "Page" {
            continue;
        }
        let number = page_index as u32 + 1;
        let mut blocks = Vec::new();
        for source in &page.children {
            let marker_type = marker_type_name(&source.block_type).to_string();
            let resolved_html = marker_block_html(source);
            let mut html = marker_review_html(&resolved_html);
            if html.trim().is_empty() {
                html = marker_block_placeholder(&marker_type, number);
            }
            let text = marker_plain_text(&resolved_html);
            let image_files = materialize_marker_images(
                source,
                number,
                out_dir,
                &mut image_budget,
                &mut quality_notes,
            );
            blocks.push(MarkerStructuredBlock {
                marker_id: source.id.clone(),
                marker_type: marker_type.clone(),
                role: marker_role(&marker_type).to_string(),
                html,
                text,
                polygon: source.polygon.clone(),
                bbox: source.bbox.clone(),
                image_files,
            });
        }
        let page_extent = page_vertical_extent(page);
        let mut candidates = 0usize;
        for index in 0..blocks.len() {
            if possible_footnote(&blocks, index, page_extent) {
                blocks[index].role = "possible_footnote".to_string();
                candidates += 1;
            }
        }
        if candidates > 0 {
            quality_notes.push(format!(
                "Marker classified {candidates} bottom-of-page text block(s) on page {number} as possible footnotes using layout and matching superscript evidence. Verify the page image if a finding depends on whether this text is a footnote."
            ));
        }
        pages.push(MarkerStructuredPage {
            number,
            marker_id: page.id.clone(),
            polygon: page.polygon.clone(),
            bbox: page.bbox.clone(),
            blocks,
        });
    }
    if pages.is_empty() {
        return Err("Marker JSON output contained no page blocks".to_string());
    }
    let text = pages
        .iter()
        .map(render_marker_page)
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        return Err("Marker structured normalization returned empty output".to_string());
    }
    if text.len() > crate::pipeline::claude::MAX_STDOUT_BYTES {
        return Err("Normalized Marker document exceeded the 50 MB safety limit".to_string());
    }
    let structure = MarkerStructure {
        schema_version: MARKER_STRUCTURE_SCHEMA,
        quality_notes: quality_notes.clone(),
        pages,
    };
    let structure_json = serde_json::to_string_pretty(&structure)
        .map_err(|error| format!("Failed to serialize normalized Marker structure: {error}"))?;
    if structure_json.len() > crate::pipeline::claude::MAX_STDOUT_BYTES {
        return Err("Normalized Marker structure exceeded the 50 MB safety limit".to_string());
    }
    atomic_write_cache(
        &out_dir.join(MARKER_STRUCTURE_FILE),
        structure_json.as_bytes(),
    )?;
    atomic_write_cache(&out_dir.join(MARKER_DOCUMENT_FILE), text.as_bytes())?;
    Ok(MarkerExtraction {
        text,
        quality_notes,
    })
}

/// Image files marker emitted for a paper (figures/tables extracted from the
/// PDF), retained only for reading historical extraction caches.
pub fn marker_image_files(paper_hash: &str) -> Result<Vec<PathBuf>, String> {
    let Some(root) = marker_output_dir(paper_hash) else {
        return Ok(Vec::new());
    };
    let mut images = Vec::new();
    let mut stack = vec![root];
    let mut walk = crate::safety::WalkBudget::new("Marker image discovery");
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            walk.entry()?;
            let p = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Failed to inspect marker output: {error}"))?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                walk.directory()?;
                stack.push(p);
            } else if file_type.is_file()
                && ["png", "jpg", "jpeg", "gif", "webp"]
                    .iter()
                    .any(|ext| ext_eq(&p, ext))
            {
                images.push(p);
            }
        }
    }
    images.sort();
    Ok(images)
}

pub(super) const EXTRACTION_CACHE_SCHEMA: u32 = 3;

pub(super) fn cache_fingerprint(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))[..16].to_string()
}

pub(super) fn atomic_write_cache(path: &Path, content: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let parent = path
        .parent()
        .ok_or_else(|| format!("Cache path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create extraction cache: {error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Failed to create extraction cache file: {error}"))?;
    temporary
        .write_all(content)
        .map_err(|error| format!("Failed to write extraction cache: {error}"))?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("Failed to flush extraction cache: {error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("Failed to publish extraction cache: {}", error.error))?;
    Ok(())
}
