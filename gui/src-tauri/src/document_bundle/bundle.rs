use super::*;

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
    if matches!(
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
    // Paddle already provides block-level semantics. Re-inferring the same
    // headings, equations, captions, and tables from its Markdown
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

    /// Model-facing projection backed by the exact verified extraction text.
    /// Native Paddle bundles intentionally do not duplicate that text into
    /// page nodes. Durable runs keep the exact text in `document.md`; this
    /// enriched projection is used only for transient model context.
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
