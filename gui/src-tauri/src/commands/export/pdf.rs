//! Direct Review PDF export through the OS webview in an isolated executable
//! mode. Owns its offline subprocess, temporary files, deadline and save; no
//! Workspace runtime, browser installation, or conversation state is involved.
//! The OS save dialog is the only source of destination authority. Rendering
//! finishes in private staging before an atomic replacement of the chosen file.
use super::{desktop::record_export_path, formats, print::build_pdf_report_html};
use std::{io::Read, path::Path, path::PathBuf, time::Duration};
use tauri::{ipc::Channel, AppHandle};
use tauri_plugin_dialog::DialogExt;

const MAX_MARKDOWN: usize = 16 * 1024 * 1024;
const MAX_PROVENANCE: usize = 1024 * 1024;
const MAX_PDF: u64 = 64 * 1024 * 1024;
static EXPORT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
mod native;
mod pagination;
pub use native::run_pdf_helper_if_requested;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportPdfRequest {
    pub markdown: String,
    pub provenance_markdown: Option<String>,
    pub run_id: Option<String>,
    /// Only restricts the destination; it grants no read or write authority.
    pub input_path: Option<String>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfProgress {
    pub message: String,
    pub setting_up: bool,
}

fn progress(channel: &Channel<PdfProgress>, message: &str) {
    let _ = channel.send(PdfProgress {
        message: message.into(),
        // Retained on the wire for compatibility; native export needs no setup.
        setting_up: false,
    });
}

fn prepare(request: ReportPdfRequest) -> Result<(String, Vec<PathBuf>), String> {
    if request.markdown.len() > MAX_MARKDOWN
        || request
            .provenance_markdown
            .as_ref()
            .is_some_and(|text| text.len() > MAX_PROVENANCE)
    {
        return Err("This report is too large to export as PDF. Save Markdown instead.".into());
    }
    let mut protected: Vec<PathBuf> = request.input_path.into_iter().map(PathBuf::from).collect();
    if let Some(run_id) = request.run_id {
        let (_, manifest) = formats::resolve_run(&run_id)?;
        protected.push(PathBuf::from(manifest.input_path));
    }
    Ok((
        native::document(&build_pdf_report_html(
            &request.markdown,
            request.provenance_markdown.as_deref(),
        )?),
        protected,
    ))
}

fn read_pdf(path: &Path) -> Result<Vec<u8>, String> {
    let file = crate::safety::open_regular_file(path)
        .map_err(|error| format!("Could not read the generated PDF: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_PDF + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the generated PDF: {error}"))?;
    if bytes.len() as u64 > MAX_PDF {
        return Err("The PDF exceeds the 64 MB export limit. Save Markdown instead.".into());
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("The native renderer did not produce a valid PDF. Retry the export.".into());
    }
    Ok(bytes)
}

fn render(html: &str, executable: &Path) -> Result<Vec<u8>, String> {
    let stage = tempfile::Builder::new()
        .prefix("pipeline-pdf-")
        .tempdir()
        .map_err(|error| format!("Could not prepare PDF export: {error}"))?;
    let input = stage.path().join("report.html");
    let output = stage.path().join("report.pdf");
    std::fs::write(&input, html)
        .map_err(|error| format!("Could not prepare PDF export: {error}"))?;
    let mut command = std::process::Command::new(executable);
    command
        .arg(native::ARGUMENT)
        .arg(&input)
        .arg(&output)
        .current_dir(stage.path());
    let result =
        crate::process::run_bounded_independent(&mut command, Duration::from_secs(120), 8192)
            .map_err(|error| format!("PDF export failed: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "Could not create the PDF: {}",
            String::from_utf8_lossy(&result.stderr)
                .chars()
                .take(2000)
                .collect::<String>()
        ));
    }
    pagination::number_pages(&read_pdf(&output)?)
}

fn render_and_save(
    html: &str,
    executable: &Path,
    target: &Path,
    protected: &[PathBuf],
) -> Result<String, String> {
    let bytes = render(html, executable)?;
    // Recheck after rendering before replacing the user-selected destination.
    let target = formats::check_destination(target, protected)?;
    formats::write_atomically(&target, &bytes)?;
    record_export_path(&target);
    Ok(target.to_string_lossy().into_owned())
}

pub(super) async fn save(
    app: AppHandle,
    request: ReportPdfRequest,
    channel: Channel<PdfProgress>,
) -> Result<Option<String>, String> {
    let permit = EXPORT
        .try_lock()
        .map_err(|_| "A PDF export is already in progress.")?;
    let (html, protected) = tokio::task::spawn_blocking(move || prepare(request))
        .await
        .map_err(|error| format!("Could not prepare PDF export: {error}"))??;
    let chosen = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name("Pipeline review.pdf")
            .add_filter("PDF document", &["pdf"])
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
    let target = formats::check_destination(&chosen, &protected)?;
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate PDF renderer: {error}"))?;
    progress(&channel, "Creating PDF…");
    tokio::task::spawn_blocking(move || {
        // The worker retains admission even if its async caller goes away.
        let _permit = permit;
        render_and_save(&html, &executable, &target, &protected).map(Some)
    })
    .await
    .map_err(|error| format!("PDF export failed: {error}"))?
}

#[cfg(test)]
mod tests;
