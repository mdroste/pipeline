//! Stable Tauri commands for Review export and printing.
use super::save_via_dialog;
use crate::output;
use tauri::AppHandle;

mod desktop;
mod files;
mod formats;
mod manifest;
mod package;
mod print;
mod types;

pub(crate) use desktop::{cleanup_print_export, cleanup_stale_print_exports};
pub use formats::ReportFormatExport;
use formats::{save_report_format, ReportFormat};
pub use types::{ExportMode, ExportPackageResult, ExportRunArtifactsResult, ExportSelection};

#[cfg(test)]
pub(super) use desktop::{record_export_path, validated_recent_export};
#[cfg(test)]
pub(super) use files::{copy_export_tree, create_export_stage, finish_export_stage};
#[cfg(test)]
pub(super) use manifest::{enumerate_export_files, finalize_export_metadata};
#[cfg(test)]
pub(super) use package::{write_custom_payload, write_shareable_package};
#[cfg(test)]
pub(super) use print::build_print_report_html;

#[tauri::command]
pub async fn save_report_md(
    app: AppHandle,
    markdown: String,
    suggested_name: Option<String>,
) -> Result<Option<String>, String> {
    let clean = output::normalize_math_delimiters(&output::clean_export_markdown(&markdown));
    let name = suggested_name.as_deref().unwrap_or("report.md");
    save_via_dialog(&app, name, "Markdown", "md", clean.into_bytes()).await
}

/// Save a saved run's report as one self-contained, offline HTML file.
#[tauri::command]
pub async fn save_report_html(
    app: AppHandle,
    run_id: String,
) -> Result<Option<ReportFormatExport>, String> {
    save_report_format(app, run_id, ReportFormat::Html).await
}

/// Save a saved run's report and findings as a new Word document.
#[tauri::command]
pub async fn save_report_docx(
    app: AppHandle,
    run_id: String,
) -> Result<Option<ReportFormatExport>, String> {
    save_report_format(app, run_id, ReportFormat::Docx).await
}

/// Save a copy of the reviewed `.docx` with one Word comment per finding.
#[tauri::command]
pub async fn save_report_docx_comments(
    app: AppHandle,
    run_id: String,
) -> Result<Option<ReportFormatExport>, String> {
    save_report_format(app, run_id, ReportFormat::DocxComments).await
}

/// Save a copy of the reviewed PDF with highlights and notes per finding.
#[tauri::command]
pub async fn save_report_annotated_pdf(
    app: AppHandle,
    run_id: String,
) -> Result<Option<ReportFormatExport>, String> {
    save_report_format(app, run_id, ReportFormat::AnnotatedPdf).await
}

/// Export one durable run as a complete, self-contained directory. All files
/// are copied into a hidden staging directory first, then atomically renamed
/// to a new unique child of the user-selected destination.
#[tauri::command]
pub async fn export_run_artifacts(
    run_id: String,
    destination: String,
) -> Result<ExportRunArtifactsResult, String> {
    let result = export_run_package(
        run_id,
        destination,
        ExportMode::Forensic,
        ExportSelection::default(),
    )
    .await?;
    Ok(ExportRunArtifactsResult {
        exported_path: result.exported_path,
        file_count: result.file_count,
        bytes: result.bytes,
    })
}

#[tauri::command]
pub async fn export_run_package(
    run_id: String,
    destination: String,
    mode: ExportMode,
    selection: ExportSelection,
) -> Result<ExportPackageResult, String> {
    package::export_run_package(run_id, destination, mode, selection).await
}

#[tauri::command(async)]
pub fn reveal_export_in_folder(path: String) -> Result<(), String> {
    desktop::reveal_export_in_folder(path)
}

#[tauri::command]
pub async fn save_all_artifacts(
    dir: String,
    markdown: String,
    extracted_text: String,
    report: crate::models::PipelineReport,
) -> Result<(), String> {
    package::save_all_artifacts(dir, markdown, extracted_text, report).await
}

#[tauri::command]
pub async fn print_report_html(
    markdown: String,
    provenance_markdown: Option<String>,
) -> Result<(), String> {
    desktop::print_report_html(markdown, provenance_markdown).await
}
