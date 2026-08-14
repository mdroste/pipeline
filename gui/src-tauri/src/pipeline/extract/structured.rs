use super::*;

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
