use crate::models::{PipelineReport, ReportSummary};
use std::fs;
use std::path::PathBuf;

const MAX_LEGACY_REPORT_BYTES: usize = 64 * 1024 * 1024;

fn read_legacy_report(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path)
        .map_err(|e| format!("Failed to read report '{}': {e}", path.display()))?;
    let mut bytes = Vec::with_capacity(64 * 1024);
    file.take(MAX_LEGACY_REPORT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read report '{}': {e}", path.display()))?;
    if bytes.len() > MAX_LEGACY_REPORT_BYTES {
        return Err(format!(
            "Report '{}' exceeds the {} MB safety limit",
            path.display(),
            MAX_LEGACY_REPORT_BYTES / 1024 / 1024
        ));
    }
    String::from_utf8(bytes)
        .map_err(|e| format!("Report '{}' is not valid UTF-8: {e}", path.display()))
}

/// Get the history directory (~/.pipeline/history/), creating it if needed.
/// Reports contain the full paper text and reviewer feedback — restrict to owner-only on Unix.
fn history_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let dir = home.join(".pipeline").join("history");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create history dir: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)) {
            eprintln!(
                "WARNING: could not tighten permissions on {}: {e}",
                dir.display()
            );
        }
    }
    Ok(dir)
}

/// Result of listing reports, including any warnings about skipped files.
pub struct ListReportsResult {
    pub summaries: Vec<ReportSummary>,
    pub warnings: Vec<String>,
}

/// List all saved reports. Returns warnings for any files that couldn't be read or parsed.
pub fn list_reports() -> Result<ListReportsResult, String> {
    let dir = history_dir()?;
    let mut summaries = Vec::new();
    let mut warnings = Vec::new();

    let entries = fs::read_dir(&dir).map_err(|e| format!("Failed to read history dir: {e}"))?;
    let mut walk = crate::safety::WalkBudget::new("Legacy history listing");

    for entry_result in entries {
        walk.entry()?;
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                // A read_dir iterator error (e.g. permission denied on a single
                // entry) would otherwise make that file invisible in the UI.
                warnings.push(format!("Could not read a history entry: {e}"));
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        match read_legacy_report(&path) {
            Ok(content) => match serde_json::from_str::<PipelineReport>(&content) {
                Ok(report) => {
                    summaries.push(ReportSummary {
                        paper_hash: report.paper_hash.clone(),
                        title: crate::models::paper_view(&report.orientation)
                            .map(|v| v.metadata.title)
                            .unwrap_or_default(),
                        report_date: report.report_date,
                        file_path: path.to_string_lossy().to_string(),
                    });
                }
                Err(e) => {
                    warnings.push(format!("Skipped corrupted report {filename}: {e}"));
                }
            },
            Err(e) => {
                warnings.push(format!("Could not read report {filename}: {e}"));
            }
        }
    }

    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.report_date));
    Ok(ListReportsResult {
        summaries,
        warnings,
    })
}
