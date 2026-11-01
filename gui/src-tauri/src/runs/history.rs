//! History listing, latest-report lookup and user metadata.
use super::recovery::{
    manifest_is_foreign_but_valid, recover_broken_manifest, run_has_resume_files,
};
use super::{
    load_manifest, read_utf8_at_most, runs_dir, validate_run_id, write_manifest, RunManifest,
    RunSummary, MAX_MANIFEST_BYTES, MAX_REPORT_BYTES,
};
use std::fs;
use std::path::{Path, PathBuf};

/// List every run on disk as a summary row, newest first. Unreadable or
/// malformed manifests are skipped rather than failing the whole listing.
pub fn list_runs() -> Result<Vec<RunSummary>, String> {
    let dir = runs_dir()?;
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(summaries);
    };
    let mut walk = crate::safety::WalkBudget::new("Run history listing");
    for entry in entries.flatten() {
        walk.entry()?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest_path = entry.path().join("manifest.json");
        let manifest_too_large = fs::symlink_metadata(&manifest_path)
            .map(|metadata| metadata.len() > MAX_MANIFEST_BYTES as u64)
            .unwrap_or(false);
        let manifest = if manifest_too_large {
            // Never rename and replace an oversized manifest: doing so can
            // destroy a valid run index merely because an older writer
            // exceeded the newer reader's limit.
            None
        } else {
            let Some(content) = read_manifest_content_for_listing(&manifest_path) else {
                // The bytes could not be read at all: NotFound or a sharing
                // violation while a writer replaces the file on Windows, or a
                // directory without a manifest. Quarantining or synthesizing a
                // manifest without having read the bytes destructively rewrites
                // healthy runs; skip the directory for this listing pass.
                continue;
            };
            serde_json::from_str::<RunManifest>(&content)
                .ok()
                .or_else(|| {
                    // A manifest that is valid JSON but does not deserialize as
                    // RunManifest was almost certainly written by a different
                    // app version (e.g. a field type changed). Quarantining it
                    // would convert a healthy run into a synthetic "failed" row
                    // and hide the real data on a downgrade or side-by-side
                    // install. Only recover when the read bytes are not valid
                    // JSON at all.
                    if manifest_is_foreign_but_valid(Some(content.as_str()), true) {
                        None
                    } else {
                        recover_broken_manifest(&entry.path(), true)
                    }
                })
        };
        if let Some(manifest) = manifest {
            let mut summary = manifest.to_summary();
            summary.resumable = run_has_resume_files(&entry.path(), &manifest);
            summaries.push(summary);
        }
    }
    sort_newest_first(&mut summaries, |summary| &summary.created);
    Ok(summaries)
}

/// Order by the instant each run was created, newest first. `created` keeps
/// the local offset it was written with, so its text sorts correctly only
/// within one offset. A timestamp that cannot be read sorts last.
pub(super) fn sort_newest_first<T>(items: &mut [T], created: impl Fn(&T) -> &str) {
    items.sort_by_cached_key(|item| {
        let text = created(item);
        let instant = chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|time| time.with_timezone(&chrono::Utc));
        std::cmp::Reverse((instant, text.to_string()))
    });
}

/// The manifest bytes when they could actually be read, or `None` on any
/// open/read failure. Windows writers replace `manifest.json` in place, so
/// NotFound / permission / sharing-violation errors here are routinely
/// transient; only content that was fully read may be judged — and possibly
/// quarantined — by the caller. Invalid UTF-8 is read lossily so genuinely
/// corrupt bytes still fail JSON parsing rather than masquerading as an
/// unreadable file.
fn read_manifest_content_for_listing(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let file = crate::safety::open_regular_file(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Locate the newest completed report in the same revision lineage. Stable
/// input path is the primary lineage key; content hash also finds identical
/// copies moved back to the same logical input.
pub fn load_latest_report_for_input(
    input_path: &str,
    paper_hash: &str,
) -> Result<Option<crate::models::PipelineReport>, String> {
    let normalized_input = Path::new(input_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(input_path));
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    let mut walk = crate::safety::WalkBudget::new_cancellable("Prior report discovery");
    for entry in fs::read_dir(runs_dir()?).map_err(|e| format!("Failed to list runs: {e}"))? {
        walk.entry()?;
        let Ok(entry) = entry else { continue };
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let manifest = read_utf8_at_most(
            &entry.path().join("manifest.json"),
            MAX_MANIFEST_BYTES,
            "Run manifest",
        )
        .ok()
        .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok());
        let Some(manifest) = manifest else { continue };
        if manifest.status == "running"
            || manifest.status == "failed"
            || manifest.status == "cancelled"
            || manifest.status == "interrupted"
        {
            continue;
        }
        let same_path = Path::new(&manifest.input_path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&manifest.input_path))
            == normalized_input;
        let same_hash = manifest.run_id.starts_with(&format!("{paper_hash}_"));
        if same_path || same_hash {
            candidates.push((manifest.created, entry.path().join("report.json")));
        }
    }
    sort_newest_first(&mut candidates, |candidate| &candidate.0);
    for (_, path) in candidates {
        let Ok(json) = read_utf8_at_most(&path, MAX_REPORT_BYTES, "Run report") else {
            continue;
        };
        if let Ok(report) = serde_json::from_str(&json) {
            return Ok(Some(report));
        }
    }
    Ok(None)
}

/// Update a run's user-assigned title and tags in place. Tags are trimmed and
/// de-duplicated; empties are dropped.
pub fn update_run_meta(run_id: &str, title: &str, tags: &[String]) -> Result<(), String> {
    validate_run_id(run_id)?;
    let mut manifest = load_manifest(run_id)?;
    if manifest.status == "running" {
        return Err("A running job cannot be renamed or retagged".to_string());
    }
    let title = title.trim();
    if title.len() > 500 {
        return Err("Run title cannot exceed 500 bytes".to_string());
    }
    if tags.len() > 50 || tags.iter().any(|tag| tag.trim().len() > 100) {
        return Err("Runs support at most 50 tags of 100 bytes each".to_string());
    }
    manifest.title = title.to_string();
    let mut seen = std::collections::HashSet::new();
    manifest.tags = tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .collect();
    let dir = runs_dir()?.join(run_id);
    write_manifest(&dir, &manifest)
}
