use super::*;

fn resolve_artifact_path(run_id: &str, rel_path: &str) -> Result<PathBuf, String> {
    validate_run_id(run_id)?;
    if rel_path.is_empty()
        || Path::new(rel_path).is_absolute()
        || rel_path.split(['/', '\\']).any(|part| part == "..")
    {
        return Err("Invalid artifact path".into());
    }
    let run_dir = runs_dir()?
        .join(run_id)
        .canonicalize()
        .map_err(|_| "Run not found".to_string())?;
    let path = run_dir
        .join(rel_path)
        .canonicalize()
        .map_err(|_| "Artifact not found".to_string())?;
    if !path.starts_with(&run_dir) {
        return Err("Invalid artifact path".into());
    }
    Ok(path)
}

/// Forward-slash a canonical path and strip Windows verbatim prefixes
/// (`\\?\C:\…` → `C:/…`, `\\?\UNC\server\share` → `//server/share`), mirroring
/// `pipeline::claude::normalize_canonical_cli_path`. "Open externally" hands
/// this to ShellExecute, which rejects verbatim paths.
pub(super) fn normalize_external_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        rest.to_string()
    } else {
        normalized
    }
}

/// Read an artifact for display. The single choke point for file bytes
/// reaching the webview: validates the path stays inside the run dir,
/// applies size caps, and never returns raw binary.
pub fn read_artifact(run_id: &str, rel_path: &str) -> Result<ArtifactContent, String> {
    let path = resolve_artifact_path(run_id, rel_path)?;

    let abs_path = normalize_external_path(&path.to_string_lossy());

    // Sniff a small head for kind detection of extensionless files.
    let (head, size) = {
        use std::io::Read as _;
        let mut buf = vec![0u8; 512];
        let mut f = crate::safety::open_regular_file(&path)
            .map_err(|e| format!("Cannot open artifact: {e}"))?;
        let size = f
            .metadata()
            .map_err(|e| format!("Cannot stat artifact: {e}"))?
            .len();
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("Cannot read artifact: {e}"))?;
        buf.truncate(n);
        (buf, size)
    };
    let kind = detect_kind(rel_path, &head);

    match kind {
        "image" => {
            if size > MAX_IMAGE_BYTES {
                return Ok(ArtifactContent {
                    kind: kind.into(),
                    bytes: size,
                    text: None,
                    base64: None,
                    truncated: false,
                    abs_path,
                });
            }
            let (bytes, grew_too_large) = read_at_most(&path, MAX_IMAGE_BYTES as usize)?;
            if grew_too_large {
                return Ok(ArtifactContent {
                    kind: kind.into(),
                    bytes: size,
                    text: None,
                    base64: None,
                    truncated: false,
                    abs_path,
                });
            }
            use base64::Engine as _;
            Ok(ArtifactContent {
                kind: kind.into(),
                bytes: size,
                text: None,
                base64: Some(base64::engine::general_purpose::STANDARD.encode(&bytes)),
                truncated: false,
                abs_path,
            })
        }
        "binary" | "pdf" => Ok(ArtifactContent {
            kind: kind.into(),
            bytes: size,
            text: None,
            base64: None,
            truncated: false,
            abs_path,
        }),
        _ => {
            let limit = if rel_path == "context/document_bundle.json" {
                MAX_DOCUMENT_BUNDLE_BYTES
            } else {
                MAX_TEXT_BYTES
            };
            let (bytes, truncated) = read_at_most(&path, limit)?;
            Ok(ArtifactContent {
                kind: kind.into(),
                bytes: size,
                text: Some(String::from_utf8_lossy(&bytes).into_owned()),
                base64: None,
                truncated,
                abs_path,
            })
        }
    }
}

/// Render one page of a PDF artifact through the same bundled Poppler path
/// used for document-page previews. The source path remains run-scoped and
/// only the bounded JPEG result crosses the IPC boundary.
pub fn read_pdf_artifact_page(
    run_id: &str,
    rel_path: &str,
    page: u32,
) -> Result<PdfArtifactPage, String> {
    let path = resolve_artifact_path(run_id, rel_path)?;
    if detect_kind(rel_path, &[]) != "pdf" {
        return Err("This artifact is not a PDF".to_string());
    }
    // Reopen with the shared no-follow guard before handing the canonical path
    // to Poppler. This also rejects directories and other special files.
    crate::safety::open_regular_file(&path)
        .map_err(|error| format!("Cannot open PDF artifact: {error}"))?;
    let temp = tempfile::Builder::new()
        .prefix("pipeline-pdf-preview-")
        .tempdir()
        .map_err(|error| format!("Could not create PDF preview directory: {error}"))?;
    let rendered = crate::pipeline::extract::render_pdf_page_preview(&path, temp.path(), page)?;
    use base64::Engine as _;
    let encode_page = |name: &str| -> Result<String, String> {
        let rendered_path = temp.path().join(name);
        let (bytes, grew_too_large) = read_at_most(&rendered_path, MAX_IMAGE_BYTES as usize)?;
        if grew_too_large {
            return Err(format!(
                "Rendered PDF page exceeds the {} MB inline preview limit",
                MAX_IMAGE_BYTES / 1_000_000
            ));
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    };
    let base64 = encode_page(&rendered.name)?;
    // A look-ahead failure must not make the requested page unreadable. The
    // frontend simply falls back to a normal request for the next page.
    let prefetched_next = rendered.next_name.as_deref().and_then(|name| {
        encode_page(name)
            .ok()
            .zip(page.checked_add(1))
            .map(|(base64, next_page)| PrefetchedPdfArtifactPage {
                page: next_page,
                has_previous: true,
                has_next: rendered.next_has_next,
                base64,
            })
    });
    Ok(PdfArtifactPage {
        page,
        has_previous: page > 1,
        has_next: rendered.has_next,
        base64,
        prefetched_next,
    })
}

/// Read one image from a completed run's compact page index. Only the selected
/// page reaches the webview; the other page files are never opened.
pub fn read_page_artifact(run_id: &str, page: u32) -> Result<ArtifactContent, String> {
    let manifest = load_manifest(run_id)?;
    let index = manifest
        .page_artifacts
        .ok_or_else(|| "This run does not have a compact page index".to_string())?;
    let rel_path = index.rel_path(page)?;
    read_artifact(run_id, &rel_path)
}
