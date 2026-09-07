use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentSelection {
    pub revision_id: String,
    pub revision_hash: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
    pub page: Option<u32>,
    pub region: Option<[f64; 4]>,
    pub quote: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub selection: DocumentSelection,
    pub body: String,
    pub prefix: String,
    pub suffix: String,
    pub origin: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    pub revision: research::PaperRevision,
    pub title: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub total_bytes: usize,
    pub page_count: Option<usize>,
    pub image_url: Option<String>,
    pub page: Option<u32>,
}
pub(super) fn revision(
    store: &Store,
    workspace_id: &str,
    revision_id: &str,
) -> WorkbenchResult<research::PaperRevision> {
    scope(store, workspace_id)?;
    valid_id(revision_id)?;
    store.connection()?.query_row("SELECT pr.id,pr.paper_id,pr.input_kind,pr.entrypoint,pr.dependency_manifest_json,pr.content_hash,pr.text_reference,pr.compiled_artifact_id,pr.extraction_json,pr.capture_complete,pr.captured_at FROM paper_revisions pr JOIN papers p ON p.id=pr.paper_id WHERE pr.id=?1 AND p.workspace_id=?2",params![revision_id,workspace_id],research::revision_from_row).optional().map_err(err)?.ok_or_else(||WorkbenchError::invalid("Document revision is not in this Workspace"))
}
fn full_text(store: &Store, revision: &research::PaperRevision) -> WorkbenchResult<String> {
    let Some(reference) = &revision.text_reference else {
        return Ok(String::new());
    };
    let p = Path::new(reference);
    if p.parent() != Some(store.root_path().join("blobs").as_path()) {
        return Err(WorkbenchError::invalid(
            "Document text is outside immutable storage",
        ));
    }
    let mut f = crate::safety::open_regular_file(p).map_err(err)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(WorkbenchError::invalid(
            "Document text exceeds reader limit",
        ));
    }
    String::from_utf8(bytes).map_err(err)
}
pub fn read_document(
    store: &Store,
    workspace_id: &str,
    revision_id: &str,
    start: usize,
    page: Option<u32>,
) -> WorkbenchResult<DocumentView> {
    let revision = revision(store, workspace_id, revision_id)?;
    let text = full_text(store, &revision)?;
    let title: String = store
        .connection()?
        .query_row(
            "SELECT title FROM papers WHERE id=?1",
            [&revision.paper_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let total_bytes = text.len();
    if start > total_bytes || !text.is_char_boundary(start) {
        return Err(WorkbenchError::invalid("Invalid document byte offset"));
    }
    let mut end = (start + 128 * 1024).min(total_bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let page_count = if revision.input_kind == "pdf" && !text.is_empty() {
        Some(text.matches('\u{000c}').count().max(1))
    } else {
        None
    };
    let image_url = if let Some(page) = page {
        if revision.input_kind != "pdf" {
            return Err(WorkbenchError::invalid(
                "Page rendering requires a PDF revision",
            ));
        }
        let image = research::paper_page_image(store, workspace_id, revision_id, page)?;
        image["_contentItems"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find_map(|i| i.get("imageUrl").and_then(Value::as_str))
            })
            .map(str::to_owned)
    } else {
        None
    };
    Ok(DocumentView {
        revision,
        title,
        text: text[start..end].into(),
        start,
        end,
        total_bytes,
        page_count,
        image_url,
        page,
    })
}
pub(super) fn annotate(
    store: &Store,
    workspace_id: &str,
    selection: DocumentSelection,
    body: String,
) -> WorkbenchResult<ProjectRecord> {
    bounded(&body, 16000)?;
    if selection.quote.len() > 65536 {
        return Err(WorkbenchError::invalid("Selection exceeds 64 KiB"));
    }
    let revision = revision(store, workspace_id, &selection.revision_id)?;
    if revision.content_hash != selection.revision_hash {
        return Err(WorkbenchError::conflict(
            "Selection revision hash does not match",
        ));
    }
    let text = full_text(store, &revision)?;
    let mut prefix = String::new();
    let mut suffix = String::new();
    match (selection.start, selection.end) {
        (Some(start), Some(end))
            if start < end
                && end <= text.len()
                && text.is_char_boundary(start)
                && text.is_char_boundary(end) =>
        {
            if text[start..end] != selection.quote {
                return Err(WorkbenchError::conflict(
                    "Selection text does not match the captured revision",
                ));
            }
            prefix = text[..start]
                .chars()
                .rev()
                .take(80)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            suffix = text[end..].chars().take(80).collect();
        }
        (None, None) if selection.page.is_some() && selection.region.is_some() => (),
        _ => {
            return Err(WorkbenchError::invalid(
                "Choose an exact text span or PDF page region",
            ))
        }
    }
    if let Some(region) = selection.region {
        if region
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 1.0)
            || region[2] <= 0.0
            || region[3] <= 0.0
            || region[0] + region[2] > 1.000001
            || region[1] + region[3] > 1.000001
            || selection.page.is_none()
        {
            return Err(WorkbenchError::invalid("Invalid normalized page region"));
        }
    }
    if let Some(page) = selection.page {
        if revision.input_kind != "pdf" {
            return Err(WorkbenchError::invalid("Page selections require a PDF"));
        }
        research::paper_page_image(store, workspace_id, &selection.revision_id, page)?;
    }
    put(
        store,
        workspace_id,
        &id("anchor")?,
        "anchor",
        0,
        &Annotation {
            selection,
            body,
            prefix,
            suffix,
            origin: "user".into(),
        },
    )
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorMapping {
    pub status: String,
    pub original: DocumentSelection,
    pub target_revision_id: String,
    pub candidates: Vec<[usize; 2]>,
    pub explanation: String,
}
pub fn map_anchor(
    store: &Store,
    workspace_id: &str,
    anchor_id: &str,
    target: &str,
) -> WorkbenchResult<AnchorMapping> {
    let annotation: Annotation = decode(&record(store, workspace_id, anchor_id, "anchor")?)?;
    let revision = revision(store, workspace_id, target)?;
    let (status, candidates, explanation) = if revision.content_hash
        == annotation.selection.revision_hash
    {
        (
            "exact",
            annotation
                .selection
                .start
                .zip(annotation.selection.end)
                .map(|(a, b)| vec![[a, b]])
                .unwrap_or_default(),
            "The immutable content hash is unchanged.",
        )
    } else if annotation.selection.quote.is_empty() {
        (
            "missing",
            Vec::new(),
            "Page geometry cannot be transferred to different PDF bytes automatically.",
        )
    } else {
        let text = full_text(store, &revision)?;
        let hits = text
            .match_indices(&annotation.selection.quote)
            .take(21)
            .map(|(offset, q)| [offset, offset + q.len()])
            .collect::<Vec<_>>();
        match hits.len(){0=>("missing",hits,"The original passage was not found. Open the original revision."),1=>("candidate",hits,"One matching passage was found in a different revision. Review before creating a new annotation."),_=>("ambiguous",hits,"Repeated passages match. No automatic mapping was applied.")}
    };
    Ok(AnchorMapping {
        status: status.into(),
        original: annotation.selection,
        target_revision_id: target.into(),
        candidates,
        explanation: explanation.into(),
    })
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPreview {
    pub text: Option<String>,
    pub image_url: Option<String>,
    pub hash: String,
    pub size: usize,
}
pub fn preview_snapshot(
    store: &Store,
    workspace_id: &str,
    digest: &str,
) -> WorkbenchResult<SnapshotPreview> {
    scope(store, workspace_id)?;
    let bytes = blob_read(store, workspace_id, digest)?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[255, 216, 255]) {
        Some("image/jpeg")
    } else {
        None
    };
    let image_url = mime.map(|mime| {
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        )
    });
    let text = if bytes.len() <= 512 * 1024 {
        String::from_utf8(bytes.clone())
            .ok()
            .filter(|s| !s.contains('\0'))
    } else {
        None
    };
    Ok(SnapshotPreview {
        text,
        image_url,
        hash: digest.into(),
        size: bytes.len(),
    })
}

/// Explicit, bounded capture for inspecting a working file. The returned digest
/// identifies immutable bytes even if an external editor changes the source.
pub fn preview_working_file(
    store: &Store,
    ws: &str,
    path: &str,
) -> WorkbenchResult<SnapshotPreview> {
    let root = root(store, ws)?;
    let bytes = files::SafeRoot::open(&root)?.read(path)?;
    let digest = blob(store, ws, &bytes, "working_file_preview")?;
    preview_snapshot(store, ws, &digest)
}
