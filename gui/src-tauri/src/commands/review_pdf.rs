//! A one-page PDF reading check using the same settings as a prepared review.
use super::run_context::{bind_model_overrides, load_run_snapshot, RunModelOverrides};
use base64::Engine;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOptions {
    method: String,
    provider: String,
    snapshot_id: String,
}

fn effective_method(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
) -> String {
    let method = config.extraction.method.trim();
    crate::settings::resolve_pdf_extractor(if method.is_empty() || method == "auto" {
        &settings.pdf_extractor
    } else {
        method
    })
    .to_string()
}

#[tauri::command]
pub async fn review_pdf_options(
    run_parallel_overrides: Option<RunModelOverrides>,
) -> Result<PdfOptions, String> {
    let snapshot = bind_model_overrides(load_run_snapshot()?, run_parallel_overrides.as_ref())?;
    Ok(PdfOptions {
        method: effective_method(&snapshot.config, &snapshot.settings),
        provider: snapshot.settings.preferred_provider,
        snapshot_id: snapshot.fingerprint,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfSample {
    text: String,
    image: String,
    method: String,
    page_count: usize,
    notes: Vec<String>,
}

/// Copy the selected page into a private PDF. Drop document-level attachments,
/// actions, metadata and outlines, then remove unreachable page objects.
fn sample_page(
    path: &std::path::Path,
    page: u32,
    output: &std::path::Path,
) -> Result<usize, String> {
    if page == 0 {
        return Err("Choose a page starting at 1".into());
    }
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Err("Choose a PDF file".into());
    }
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > 100_000_000 {
        return Err(
            "The sample supports PDFs up to 100 MB. You can still review the full file.".into(),
        );
    }
    let mut doc =
        lopdf::Document::load(path).map_err(|e| format!("Couldn't read this PDF: {e}"))?;
    let pages = doc.get_pages();
    if !pages.contains_key(&page) {
        return Err(format!("This PDF has {} pages", pages.len()));
    }
    let count = pages.len();
    doc.delete_pages(
        &pages
            .keys()
            .copied()
            .filter(|p| *p != page)
            .collect::<Vec<_>>(),
    );
    // Page annotations can point back to other pages or embedded files.
    // The sample needs only the selected page's visible content.
    for page_id in doc.get_pages().values() {
        if let Ok(page) = doc
            .get_object_mut(*page_id)
            .and_then(|object| object.as_dict_mut())
        {
            for key in [b"Annots".as_slice(), b"AA", b"Metadata", b"B", b"Thumb"] {
                page.remove(key);
            }
        }
    }
    if let Ok(catalog) = doc.catalog_mut() {
        for key in [
            b"Names".as_slice(),
            b"OpenAction",
            b"AA",
            b"Metadata",
            b"Outlines",
            b"AcroForm",
            b"StructTreeRoot",
        ] {
            catalog.remove(key);
        }
    }
    doc.trailer.remove(b"Info");
    doc.prune_objects();
    doc.save(output)
        .map_err(|e| format!("Couldn't prepare the sample: {e}"))?;
    Ok(count)
}

#[tauri::command]
pub async fn preview_review_pdf(
    paper_path: String,
    page: u32,
    method: String,
    allow_model_call: bool,
    expected_snapshot_id: String,
    run_parallel_overrides: Option<RunModelOverrides>,
) -> Result<PdfSample, String> {
    let mut snapshot = bind_model_overrides(load_run_snapshot()?, run_parallel_overrides.as_ref())?;
    if snapshot.fingerprint != expected_snapshot_id {
        return Err("The review settings changed. Reload PDF options and preview again.".into());
    }
    let resolved = effective_method(&snapshot.config, &snapshot.settings);
    if resolved != method {
        return Err("The PDF reading method changed. Check it and preview again.".into());
    }
    if resolved == "llm" && !allow_model_call {
        return Err(
            "This sample needs a model call. Choose Preview page with AI to continue.".into(),
        );
    }
    snapshot.config.extraction.method = resolved.clone();
    let temp = tempfile::Builder::new()
        .prefix("pipeline-review-sample-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let sample = temp.path().join("sample.pdf");
    let sample_copy = sample.clone();
    let render_dir = temp.path().join("pages");
    let (page_count, image) = tokio::task::spawn_blocking(move || {
        let count = sample_page(std::path::Path::new(&paper_path), page, &sample_copy)?;
        let preview =
            crate::pipeline::extract::render_pdf_page_preview(&sample_copy, &render_dir, 1)?;
        let bytes = std::fs::read(render_dir.join(preview.name)).map_err(|e| e.to_string())?;
        Ok::<_, String>((
            count,
            format!(
                "data:image/jpeg;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ),
        ))
    })
    .await
    .map_err(|e| e.to_string())??;
    let active =
        crate::run_scope::ActiveScope::begin(format!("pdf-sample-{}", temp.path().display()));
    let scope = active.scope().clone();
    let result = crate::run_scope::scoped(scope, async {
        let _settings = crate::settings::freeze_for_run(snapshot.settings);
        let events: crate::emit::EventBus = std::sync::Arc::new(crate::emit::NullEvents);
        crate::pipeline::extract::extract(
            &events,
            &sample.to_string_lossy(),
            &snapshot.config.extraction,
        )
        .await
    })
    .await?;
    let mut notes = result.quality_notes;
    let mut text = result.text;
    if text.len() > 200_000 {
        let mut end = 200_000;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        notes.push(
            "This page's sample stops at 200,000 bytes. The review reads the full page.".into(),
        );
    }
    Ok(PdfSample {
        text,
        image,
        method: resolved,
        page_count,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_contains_only_selected_page_and_keeps_input_unchanged() {
        use lopdf::{dictionary, Document, Object, Stream};
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input.pdf");
        let output = temp.path().join("sample.pdf");
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let first_content =
            doc.add_object(Stream::new(dictionary! {}, b"FIRST_PAGE_CONTENT".to_vec()));
        let second_content =
            doc.add_object(Stream::new(dictionary! {}, b"SECOND_PAGE_CONTENT".to_vec()));
        let first = doc.add_object(
            dictionary! {"Type" => "Page", "Parent" => pages_id, "Contents" => first_content},
        );
        let annotation = doc.add_object(dictionary! {"Type" => "Annot", "Subtype" => "Link", "Dest" => Object::Array(vec![first.into(), Object::Name(b"Fit".to_vec())])});
        let second = doc.add_object(dictionary! {"Type" => "Page", "Parent" => pages_id, "Contents" => second_content, "Annots" => Object::Array(vec![annotation.into()])});
        doc.objects.insert(pages_id, dictionary! {"Type" => "Pages", "Kids" => Object::Array(vec![first.into(), second.into()]), "Count" => 2, "MediaBox" => Object::Array(vec![0.into(), 0.into(), 600.into(), 800.into()])}.into());
        let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
        doc.trailer.set("Root", catalog);
        doc.save(&input).unwrap();
        let original = std::fs::read(&input).unwrap();
        assert_eq!(sample_page(&input, 2, &output).unwrap(), 2);
        let sample = Document::load(&output).unwrap();
        assert_eq!(sample.get_pages().len(), 1);
        assert!(!sample.objects.contains_key(&first));
        assert!(!sample.objects.contains_key(&first_content));
        assert!(!sample.objects.contains_key(&annotation));
        assert!(sample.objects.contains_key(&second_content));
        assert_eq!(std::fs::read(&input).unwrap(), original);
        assert!(sample_page(&input, 3, &output)
            .unwrap_err()
            .contains("2 pages"));
    }
    #[test]
    fn rejects_non_pdf_and_zero_page_before_parsing() {
        let temp = tempfile::tempdir().unwrap();
        assert!(sample_page(
            std::path::Path::new("input.pdf"),
            0,
            &temp.path().join("out.pdf")
        )
        .unwrap_err()
        .contains("starting at 1"));
        assert!(sample_page(
            std::path::Path::new("input.txt"),
            1,
            &temp.path().join("out.pdf")
        )
        .unwrap_err()
        .contains("PDF"));
    }
}
