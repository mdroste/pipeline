use crate::models::{PipelineReport, ReportSummary};
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static HISTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn history_filename(paper_hash: &str) -> String {
    let sequence = HISTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}_{}-{sequence:016x}.json",
        paper_hash,
        Local::now().format("%Y-%m-%d_%H%M%S-%9f")
    )
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

/// Save a report to ~/.pipeline/history/<hash>_<date>.json.
pub fn save_report(report: &PipelineReport) -> Result<String, String> {
    use std::io::Write as _;
    let dir = history_dir()?;
    let filename = history_filename(&report.paper_hash);
    let path = dir.join(&filename);

    let json = serde_json::to_string_pretty(report)
        .map_err(|e| format!("Failed to serialize report: {e}"))?;
    // Atomic write with a unique temp name so concurrent saves for the same
    // hash+second can't clobber each other's .tmp file.
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)
        .map_err(|e| format!("Failed to create temp file in {}: {e}", dir.display()))?;
    tmp.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write report: {e}"))?;
    tmp.persist_noclobber(&path)
        .map_err(|e| format!("Failed to save report: {}", e.error))?;

    Ok(path.to_string_lossy().to_string())
}

/// Load the most recent report for a given paper hash.
pub fn load_latest_report(paper_hash: &str) -> Result<Option<PipelineReport>, String> {
    // Validate hash format: hex characters only, expected length
    if paper_hash.is_empty()
        || !paper_hash.chars().all(|c| c.is_ascii_alphanumeric())
        || paper_hash.len() > 64
    {
        return Err("Invalid paper hash format".into());
    }
    let dir = history_dir()?;
    let prefix = format!("{}_", paper_hash);

    let mut matching: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read history dir: {e}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&prefix) && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();

    matching.sort();

    if let Some(latest) = matching.last() {
        let content =
            fs::read_to_string(latest).map_err(|e| format!("Failed to read report: {e}"))?;
        let report: PipelineReport =
            serde_json::from_str(&content).map_err(|e| format!("Failed to parse report: {e}"))?;
        Ok(Some(report))
    } else {
        Ok(None)
    }
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

    for entry_result in entries {
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

        match fs::read_to_string(&path) {
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

    summaries.sort_by(|a, b| b.report_date.cmp(&a.report_date));
    Ok(ListReportsResult {
        summaries,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_filenames_are_unique_within_the_same_clock_tick() {
        assert_ne!(history_filename("abc"), history_filename("abc"));
    }
}
