//! Session-scoped reveal authority and temporary print-file lifecycle.
use super::print::build_print_report_html;
use std::io::Write as _;

static RECENT_EXPORT_PATHS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::VecDeque<std::path::PathBuf>>,
> = std::sync::OnceLock::new();

pub(in crate::commands) fn record_export_path(path: &std::path::Path) {
    let Ok(path) = path.canonicalize() else {
        return;
    };
    let paths = RECENT_EXPORT_PATHS
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    if let Ok(mut paths) = paths.lock() {
        paths.retain(|existing| existing != &path);
        paths.push_front(path);
        paths.truncate(16);
    }
}

pub(in crate::commands) fn validated_recent_export(
    path: &str,
) -> Result<std::path::PathBuf, String> {
    let canonical = std::path::Path::new(path)
        .canonicalize()
        .map_err(|error| format!("Could not resolve the exported package: {error}"))?;
    let paths = RECENT_EXPORT_PATHS
        .get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()));
    let paths = paths
        .lock()
        .map_err(|_| "Recent export registry is unavailable".to_string())?;
    if !paths.iter().any(|recent| recent == &canonical) {
        return Err("Pipeline can reveal only a package exported in this app session".to_string());
    }
    Ok(canonical)
}

pub fn reveal_export_in_folder(path: String) -> Result<(), String> {
    let path = validated_recent_export(&path)?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(format!("/select,{}", path.display()));
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(&path));
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Could not reveal the exported package: {error}"))
}

/// Stash the last export path so we can clean it up on the next export.
pub(in crate::commands) static LAST_EXPORT_PATH: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

pub(crate) fn cleanup_print_export() {
    if let Ok(mut previous) = LAST_EXPORT_PATH.lock() {
        if let Some(path) = previous.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn cleanup_stale_print_exports() {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(24 * 60 * 60))
        .unwrap_or(std::time::UNIX_EPOCH);
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten().take(1_000) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("pipeline_report_") || !name.ends_with(".html") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file()
            && metadata
                .modified()
                .map(|time| time < cutoff)
                .unwrap_or(false)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub async fn print_report_html(
    markdown: String,
    provenance_markdown: Option<String>,
) -> Result<(), String> {
    let html_doc = build_print_report_html(&markdown, provenance_markdown.as_deref())?;
    // Clean up previous export file
    cleanup_print_export();

    let mut tmp = tempfile::Builder::new()
        .prefix("pipeline_report_")
        .suffix(".html")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {e}"))?;
    tmp.write_all(html_doc.as_bytes())
        .map_err(|e| format!("Failed to write HTML: {e}"))?;
    tmp.flush().map_err(|e| format!("Failed to flush: {e}"))?;

    let path = tmp.into_temp_path();
    let path_buf = path
        .keep()
        .map_err(|e| format!("Failed to persist temp file: {e}"))?;

    if let Ok(mut prev) = LAST_EXPORT_PATH.lock() {
        *prev = Some(path_buf.clone());
    }

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .arg(&path_buf)
            .creation_flags(0x08000000)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg("--")
        .arg(&path_buf)
        .spawn()
        .map_err(|e| format!("Failed to open browser: {e}"))?;

    Ok(())
}
