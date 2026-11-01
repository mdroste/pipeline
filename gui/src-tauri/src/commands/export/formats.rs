//! Review report formats: one HTML file, a Word report, Word comments on the
//! reviewed `.docx`, and highlights on the reviewed PDF.
//!
//! Every export reads one finished run's saved products and writes a new file
//! the researcher chooses. Nothing is written into run storage, and the
//! reviewed manuscript is read, verified against the run's recorded content
//! hash, and never overwritten.

use super::desktop::record_export_path;
use crate::models::PipelineReport;
use crate::output::formats::{self, ReportExport, ReportMetadata};
use crate::runs::RunManifest;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const MAX_REPORT_JSON_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REPORT_MARKDOWN_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ORIGINAL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Html,
    Docx,
    DocxComments,
    AnnotatedPdf,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportFormatExport {
    pub path: String,
    pub findings: usize,
    /// Findings placed on a quoted passage (comments or highlights).
    pub anchored: usize,
    /// Findings listed in a summary comment or page note instead.
    pub unanchored: usize,
}

struct Generated {
    bytes: Vec<u8>,
    suggested_name: String,
    filter: &'static str,
    extension: &'static str,
    findings: usize,
    anchored: usize,
    unanchored: usize,
    /// Paths a save must never replace: the reviewed manuscript.
    protected: Vec<PathBuf>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct BundleExtraction {
    #[serde(default)]
    source_path: String,
    #[serde(default)]
    paper_hash: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct BundleOrigin {
    #[serde(default)]
    role: String,
    #[serde(default)]
    sha256: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct BundleHead {
    #[serde(default)]
    extraction: BundleExtraction,
    #[serde(default)]
    origins: Vec<BundleOrigin>,
}

struct LoadedRun {
    dir: PathBuf,
    manifest: RunManifest,
    report: PipelineReport,
    bundle: BundleHead,
    export: ReportExport,
}

fn read_bounded(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let file = crate::safety::open_regular_file(path)
        .map_err(|error| format!("Could not open the {label}: {error}"))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the {label}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("The {label} is too large to export"));
    }
    Ok(bytes)
}

fn short_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))[..16].to_string()
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn load_run(dir: &Path, manifest: RunManifest) -> Result<LoadedRun, String> {
    if manifest.status == "running" {
        return Err("Wait for this Review to finish before exporting it".into());
    }
    let report: PipelineReport = serde_json::from_slice(&read_bounded(
        &dir.join("report.json"),
        MAX_REPORT_JSON_BYTES,
        "saved report data",
    )?)
    .map_err(|error| format!("The saved report data could not be read: {error}"))?;
    let markdown = read_bounded(&dir.join("report.md"), MAX_REPORT_MARKDOWN_BYTES, "report")
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    let bundle = read_bounded(
        &dir.join("context/document_bundle.json"),
        MAX_BUNDLE_BYTES,
        "document record",
    )
    .ok()
    .and_then(|bytes| serde_json::from_slice::<BundleHead>(&bytes).ok())
    .unwrap_or_default();
    let source = if bundle.extraction.source_path.is_empty() {
        manifest.input_path.clone()
    } else {
        bundle.extraction.source_path.clone()
    };
    let paper_title = crate::models::paper_view(&report.orientation)
        .map(|view| view.metadata.title.trim().to_string())
        .unwrap_or_default();
    let input_name = file_name(&source);
    let title = [
        manifest.title.trim(),
        paper_title.as_str(),
        input_name.as_str(),
    ]
    .into_iter()
    .find(|value| !value.is_empty())
    .unwrap_or("Review report")
    .to_string();
    let status = if report.quality.status.is_empty() {
        manifest.status.clone()
    } else {
        report.quality.status.clone()
    };
    let meta = ReportMetadata {
        title,
        input_name,
        workflow: manifest.profile_name.clone(),
        date: report.report_date.to_string(),
        status: crate::output::status_label(&report.quality, &status),
    };
    let export = ReportExport::from_report(&report, &markdown, meta);
    Ok(LoadedRun {
        dir: dir.to_path_buf(),
        manifest,
        report,
        bundle,
        export,
    })
}

/// Whether a Word file is the version the run read. Its `paper_hash` covers
/// extracted text, which moves when the extractor improves, so byte-level
/// records decide first: the captured copy's fingerprint, then the hash a
/// newer run keeps of the file. Only an older run with neither falls back to
/// comparing text.
fn is_reviewed_docx(
    run: &LoadedRun,
    path: &Path,
    captured: bool,
    bytes: &[u8],
    expected: &str,
) -> Result<bool, String> {
    if captured {
        let recorded = crate::commands::reuse_artifacts::retained_copy_matches(&run.dir, path);
        if let Some(matches) = recorded {
            return Ok(matches);
        }
    }
    let primary = run
        .bundle
        .origins
        .iter()
        .find(|origin| origin.role == "primary" && !origin.sha256.is_empty());
    if let Some(origin) = primary {
        return Ok(short_hash(bytes) == origin.sha256);
    }
    Ok(expected.is_empty() || short_hash(docx_text(bytes)?.as_bytes()) == expected)
}

/// The reviewed manuscript, verified to be the exact version the run read.
/// A captured copy inside the run is preferred to the original location.
fn original_document(run: &LoadedRun, extension: &str) -> Result<(PathBuf, Vec<u8>), String> {
    let kind = if extension == "pdf" {
        "PDF"
    } else {
        "Word (.docx) file"
    };
    let source = if run.bundle.extraction.source_path.is_empty() {
        run.manifest.input_path.as_str()
    } else {
        run.bundle.extraction.source_path.as_str()
    };
    let original = PathBuf::from(source);
    let matches_kind = original
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension));
    if source.is_empty() || !matches_kind {
        return Err(format!(
            "This Review's input was not a {kind}, so there is no original to annotate. Export the report as Word or HTML instead."
        ));
    }
    let captured = run.dir.join("source-snapshot").join(file_name(source));
    let from_snapshot = crate::safety::open_regular_file(&captured).is_ok();
    let path = if from_snapshot {
        captured
    } else {
        original.clone()
    };
    let bytes = read_bounded(&path, MAX_ORIGINAL_BYTES, "reviewed file").map_err(|error| {
        format!("{error}. Pipeline needs the reviewed {kind} at its original location: {source}")
    })?;
    let expected = if run.report.paper_hash.is_empty() {
        run.bundle.extraction.paper_hash.as_str()
    } else {
        run.report.paper_hash.as_str()
    };
    let unchanged = if extension == "pdf" {
        expected.is_empty() || short_hash(&bytes) == expected
    } else {
        is_reviewed_docx(run, &path, from_snapshot, &bytes, expected)?
    };
    if !unchanged {
        return Err(format!(
            "The {kind} at {source} has changed since this Review ran, so its quotations may no longer line up. Pipeline annotates only the exact version that was reviewed."
        ));
    }
    Ok((original, bytes))
}

/// Text as the existing Review DOCX reader extracts it.
fn docx_text(bytes: &[u8]) -> Result<String, String> {
    let directory = tempfile::Builder::new()
        .prefix("pipeline_docx_check_")
        .tempdir()
        .map_err(|error| format!("Could not create a private working folder: {error}"))?;
    let path = directory.path().join("document.docx");
    std::fs::write(&path, bytes)
        .map_err(|error| format!("Could not prepare the Word file for checking: {error}"))?;
    crate::document_bundle::extract_docx_text(&path)
}

fn stem(run: &LoadedRun) -> String {
    let name = Path::new(&run.export.meta.input_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let clean: String = name
        .chars()
        .filter(|character| !character.is_control() && !"/\\:*?\"<>|".contains(*character))
        .take(80)
        .collect();
    if clean.trim().is_empty() {
        "Pipeline".to_string()
    } else {
        clean.trim().to_string()
    }
}

fn generate(run: &LoadedRun, format: ReportFormat) -> Result<Generated, String> {
    let date = &run.export.meta.date;
    let stem = stem(run);
    let file = |bytes, suggested_name, filter, extension| Generated {
        bytes,
        suggested_name,
        filter,
        extension,
        findings: run.export.findings.len(),
        anchored: 0,
        unanchored: 0,
        protected: Vec::new(),
    };
    Ok(match format {
        ReportFormat::Html => file(
            formats::render_html_report(&run.export).into_bytes(),
            format!("{stem} review.html"),
            "Web page",
            "html",
        ),
        ReportFormat::Docx => file(
            formats::render_docx_report(&run.export)?,
            format!("{stem} review.docx"),
            "Word document",
            "docx",
        ),
        ReportFormat::DocxComments => {
            let (path, original) = original_document(run, "docx")?;
            let annotated =
                formats::annotate_docx(&original, &run.export, &format!("{date}T00:00:00Z"))?;
            if docx_text(&annotated.bytes)? != docx_text(&original)? {
                return Err(
                    "Adding comments would change the manuscript's text; no file was written"
                        .into(),
                );
            }
            Generated {
                anchored: annotated.anchored,
                unanchored: annotated.unanchored,
                protected: vec![path],
                ..file(
                    annotated.bytes,
                    format!("{stem} with review comments.docx"),
                    "Word document",
                    "docx",
                )
            }
        }
        ReportFormat::AnnotatedPdf => {
            let (path, original) = original_document(run, "pdf")?;
            let pages = formats::pdftotext_bbox_layout(&original)?;
            let annotated = formats::annotate_pdf(&original, &pages, &run.export, date)?;
            Generated {
                anchored: annotated.anchored,
                unanchored: annotated.unanchored,
                protected: vec![path],
                ..file(
                    annotated.bytes,
                    format!("{stem} with review highlights.pdf"),
                    "PDF document",
                    "pdf",
                )
            }
        }
    })
}

fn resolve_run(run_id: &str) -> Result<(PathBuf, RunManifest), String> {
    crate::runs::validate_run_id(run_id)?;
    let manifest = crate::runs::load_manifest(run_id)?;
    let runs_root = crate::runs::runs_dir()?
        .canonicalize()
        .map_err(|error| format!("Could not find Review storage: {error}"))?;
    let dir = runs_root
        .join(run_id)
        .canonicalize()
        .map_err(|error| format!("Could not find this Review: {error}"))?;
    if !dir.starts_with(&runs_root) || !dir.is_dir() {
        return Err("This Review's saved files are not in Review storage".into());
    }
    Ok((dir, manifest))
}

/// Reject a destination that is the reviewed manuscript or inside Review
/// storage. The comparison uses canonical paths so links cannot bypass it.
fn check_destination(destination: &Path, protected: &[PathBuf]) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("Choose a folder for the exported file")?
        .canonicalize()
        .map_err(|error| format!("Could not open the chosen folder: {error}"))?;
    let name = destination
        .file_name()
        .ok_or("Choose a file name for the export")?;
    let target = parent.join(name);
    let resolved = target.canonicalize().unwrap_or_else(|_| target.clone());
    if let Ok(runs_root) = crate::runs::runs_dir().and_then(|root| {
        root.canonicalize()
            .map_err(|error| format!("Could not find Review storage: {error}"))
    }) {
        if resolved.starts_with(&runs_root) {
            return Err("Choose a location outside Pipeline's Review storage".into());
        }
    }
    for path in protected {
        if path.canonicalize().is_ok_and(|path| path == resolved) {
            return Err("Choose a different file name. Pipeline never replaces the reviewed manuscript; the export is a separate copy.".into());
        }
    }
    Ok(target)
}

fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or("Choose a folder for the exported file")?;
    let mut builder = tempfile::Builder::new();
    builder.prefix(".pipeline-export-");
    // A new export gets a saved file's usual permissions: the default is private,
    // which would shut colleagues out of a shared folder.
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o666));
    let mut temporary = builder
        .tempfile_in(parent)
        .map_err(|error| format!("Could not write to the chosen folder: {error}"))?;
    // A file being replaced keeps the permissions it had.
    #[cfg(unix)]
    if let Ok(existing) = std::fs::metadata(target) {
        temporary
            .as_file()
            .set_permissions(existing.permissions())
            .map_err(|error| format!("Could not write the exported file: {error}"))?;
    }
    temporary
        .write_all(bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("Could not write the exported file: {error}"))?;
    temporary
        .persist(target)
        .map_err(|error| format!("Could not save the exported file: {}", error.error))?;
    Ok(())
}

/// Build the requested format, then ask where to save it.
pub(super) async fn save_report_format(
    app: AppHandle,
    run_id: String,
    format: ReportFormat,
) -> Result<Option<ReportFormatExport>, String> {
    let generated = tokio::task::spawn_blocking(move || {
        let (dir, manifest) = resolve_run(&run_id)?;
        generate(&load_run(&dir, manifest)?, format)
    })
    .await
    .map_err(|error| format!("Export failed: {error}"))??;

    use tauri_plugin_dialog::DialogExt;
    let dialog_app = app.clone();
    let suggested = generated.suggested_name.clone();
    let (filter, extension) = (generated.filter, generated.extension);
    let chosen = tokio::task::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_file_name(suggested)
            .add_filter(filter, &[extension])
            .blocking_save_file()
    })
    .await
    .map_err(|error| format!("Save dialog failed: {error}"))?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let chosen = chosen
        .into_path()
        .map_err(|error| format!("Invalid save location: {error}"))?;
    let target = check_destination(&chosen, &generated.protected)?;
    write_atomically(&target, &generated.bytes)?;
    record_export_path(&target);
    Ok(Some(ReportFormatExport {
        path: target.to_string_lossy().into_owned(),
        findings: generated.findings,
        anchored: generated.anchored,
        unanchored: generated.unanchored,
    }))
}

#[cfg(test)]
mod tests;
