//! Durable, report-type-neutral issue tracking for projects.
//!
//! A ledger occurrence is provenance from one immutable run. Project-level
//! status and notes live separately from run annotations so a decision can
//! survive subsequent document, source-tree, or no-input reviews.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

const LEDGER_SCHEMA_VERSION: u32 = 1;
const MAX_LEDGER_BYTES: u64 = 32 * 1024 * 1024;
const MAX_LEDGER_ISSUES: usize = 10_000;
const MAX_LEDGER_OCCURRENCES: usize = 20_000;
const MAX_SCANNED_RUNS: usize = 500;
const MAX_ISSUES_PER_RUN: usize = 1_000;
const MAX_EVIDENCE_PER_ISSUE: usize = 50;
const MAX_TITLE_BYTES: usize = 1_000;
const MAX_SECTION_BYTES: usize = 1_000;
const MAX_BODY_BYTES: usize = 20_000;
const MAX_NOTE_BYTES: usize = 20_000;
const MAX_REFERENCE_BYTES: usize = 1_000;
const MAX_SOURCE_LINE: u32 = 10_000_000;
const MAX_WARNING_BYTES: usize = 2_000;
const MAX_WARNINGS: usize = 100;
const MAX_REPORT_BYTES: u64 = 64 * 1024 * 1024;

fn default_schema_version() -> u32 {
    LEDGER_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIssueEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub node_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIssueOccurrence {
    pub key: String,
    pub run_id: String,
    pub issue_id: String,
    pub observed_at: String,
    pub profile_id: String,
    pub profile_name: String,
    pub input_name: String,
    pub input_mode: String,
    pub input_interpretation: String,
    pub step_id: String,
    pub step_label: String,
    pub title: String,
    pub severity: String,
    pub section: String,
    pub body: String,
    #[serde(default)]
    pub evidence: Vec<ProjectIssueEvidence>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub annotation_status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub annotation_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIssue {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub section: String,
    /// "open" | "addressed" | "dismissed" | "regressed".
    pub status: String,
    #[serde(default)]
    pub note: String,
    pub created: String,
    pub updated: String,
    /// Empty means the status follows run-level annotations. A timestamp means
    /// the user made an explicit project-level decision.
    #[serde(default)]
    pub decision_updated: String,
    /// True when the issue was not observed in any readable run during the
    /// last refresh (its runs were purged, unreadable, or removed from the
    /// project). The last-known occurrences are retained as history so the
    /// user's decision and notes survive run retention.
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub occurrences: Vec<ProjectIssueOccurrence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIssueLedger {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub project_id: String,
    pub updated: String,
    #[serde(default)]
    pub issues: Vec<ProjectIssue>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn empty_ledger(project_id: &str) -> ProjectIssueLedger {
    ProjectIssueLedger {
        schema_version: LEDGER_SCHEMA_VERSION,
        project_id: project_id.to_string(),
        updated: now(),
        issues: Vec::new(),
        warnings: Vec::new(),
    }
}

fn ledger_dir(projects_dir: &Path) -> PathBuf {
    projects_dir.join("ledgers")
}

fn ensure_ledger_dir(projects_dir: &Path) -> Result<PathBuf, String> {
    let dir = ledger_dir(projects_dir);
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create project-ledger directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Failed to secure project-ledger directory: {error}"))?;
    }
    Ok(dir)
}

fn ledger_path(projects_dir: &Path, project_id: &str) -> Result<PathBuf, String> {
    super::validate_project_id(project_id)?;
    Ok(ledger_dir(projects_dir).join(format!("{project_id}.json")))
}

pub(super) fn delete_ledger_from(projects_dir: &Path, project_id: &str) -> Result<(), String> {
    let path = ledger_path(projects_dir, project_id)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to delete project issue ledger: {error}")),
    }
}

fn load_ledger_from(projects_dir: &Path, project_id: &str) -> Result<ProjectIssueLedger, String> {
    let path = ledger_path(projects_dir, project_id)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(empty_ledger(project_id));
        }
        Err(error) => return Err(format!("Failed to inspect project issue ledger: {error}")),
    }
    let file = crate::safety::open_regular_file(&path)
        .map_err(|error| format!("Failed to open project issue ledger: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Failed to inspect project issue ledger: {error}"))?
        .len();
    if size > MAX_LEDGER_BYTES {
        return Err("Project issue ledger exceeds the storage limit".to_string());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_LEDGER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read project issue ledger: {error}"))?;
    if bytes.len() as u64 > MAX_LEDGER_BYTES {
        return Err("Project issue ledger exceeds the storage limit".to_string());
    }
    let ledger: ProjectIssueLedger = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Project issue ledger is invalid: {error}"))?;
    validate_ledger(&ledger, project_id)?;
    Ok(ledger)
}

fn write_ledger_to(projects_dir: &Path, ledger: &ProjectIssueLedger) -> Result<(), String> {
    validate_ledger(ledger, &ledger.project_id)?;
    let dir = ensure_ledger_dir(projects_dir)?;
    let destination = ledger_path(projects_dir, &ledger.project_id)?;
    let bytes = serde_json::to_vec_pretty(ledger)
        .map_err(|error| format!("Failed to serialize project issue ledger: {error}"))?;
    if bytes.len() as u64 > MAX_LEDGER_BYTES {
        return Err("Project issue ledger exceeds the storage limit".to_string());
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir)
        .map_err(|error| format!("Failed to create project-ledger staging file: {error}"))?;
    temp.write_all(&bytes)
        .map_err(|error| format!("Failed to stage project issue ledger: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("Failed to flush project issue ledger: {error}"))?;
    temp.persist(destination)
        .map_err(|error| format!("Failed to save project issue ledger: {}", error.error))?;
    Ok(())
}

fn validate_ledger(ledger: &ProjectIssueLedger, project_id: &str) -> Result<(), String> {
    if ledger.project_id != project_id {
        return Err("Project issue ledger has mismatched metadata".to_string());
    }
    if ledger.schema_version > LEDGER_SCHEMA_VERSION {
        return Err("Project issue ledger was created by a newer Pipeline version".to_string());
    }
    if ledger.issues.len() > MAX_LEDGER_ISSUES {
        return Err("Project issue ledger contains too many issues".to_string());
    }
    if ledger.warnings.len() > MAX_WARNINGS
        || ledger
            .warnings
            .iter()
            .any(|warning| warning.len() > MAX_WARNING_BYTES)
    {
        return Err("Project issue ledger warnings exceed their limit".to_string());
    }
    let mut issue_ids = HashSet::new();
    let mut occurrence_keys = HashSet::new();
    let mut occurrence_count = 0usize;
    for issue in &ledger.issues {
        if !valid_internal_id(&issue.id, "issue-") || !issue_ids.insert(&issue.id) {
            return Err("Project issue ledger contains an invalid issue id".to_string());
        }
        if !valid_status(&issue.status)
            || issue.title.len() > MAX_TITLE_BYTES
            || issue.section.len() > MAX_SECTION_BYTES
            || issue.note.len() > MAX_NOTE_BYTES
        {
            return Err("Project issue ledger contains invalid issue metadata".to_string());
        }
        occurrence_count = occurrence_count.saturating_add(issue.occurrences.len());
        for occurrence in &issue.occurrences {
            if !valid_internal_id(&occurrence.key, "occ-")
                || !occurrence_keys.insert(&occurrence.key)
            {
                return Err("Project issue ledger contains an invalid occurrence key".to_string());
            }
            crate::runs::validate_run_id(&occurrence.run_id)?;
            if occurrence.title.len() > MAX_TITLE_BYTES
                || occurrence.section.len() > MAX_SECTION_BYTES
                || occurrence.body.len() > MAX_BODY_BYTES
                || occurrence.annotation_note.len() > MAX_NOTE_BYTES
                || occurrence.evidence.len() > MAX_EVIDENCE_PER_ISSUE
            {
                return Err("Project issue occurrence exceeds its limit".to_string());
            }
            for evidence in &occurrence.evidence {
                validate_evidence(evidence)?;
            }
        }
    }
    if occurrence_count > MAX_LEDGER_OCCURRENCES {
        return Err("Project issue ledger contains too many occurrences".to_string());
    }
    Ok(())
}

fn valid_internal_id(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn valid_status(value: &str) -> bool {
    matches!(value, "open" | "addressed" | "dismissed" | "regressed")
}

fn validate_evidence(evidence: &ProjectIssueEvidence) -> Result<(), String> {
    if evidence.page.is_some_and(|page| page == 0 || page > 5_000)
        || evidence
            .line_start
            .is_some_and(|line| line == 0 || line > MAX_SOURCE_LINE)
        || evidence
            .line_end
            .is_some_and(|line| line == 0 || line > MAX_SOURCE_LINE)
        || (evidence.line_end.is_some() && evidence.line_start.is_none())
        || matches!((evidence.line_start, evidence.line_end), (Some(start), Some(end)) if end < start)
        || evidence.node_id.len() > MAX_REFERENCE_BYTES
        || evidence.asset_id.len() > MAX_REFERENCE_BYTES
        || evidence.artifact_path.len() > MAX_REFERENCE_BYTES
        || evidence.description.len() > MAX_BODY_BYTES
        || evidence.quote.len() > MAX_BODY_BYTES
        || (!evidence.artifact_path.is_empty() && !safe_artifact_path(&evidence.artifact_path))
    {
        return Err("Project issue evidence is invalid".to_string());
    }
    Ok(())
}

fn read_run_report(run_id: &str) -> Result<crate::models::PipelineReport, String> {
    crate::runs::validate_run_id(run_id)?;
    let path = crate::runs::runs_dir()?.join(run_id).join("report.json");
    let file = crate::safety::open_regular_file(&path)
        .map_err(|error| format!("Cannot open report.json: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Cannot inspect report.json: {error}"))?
        .len();
    if size > MAX_REPORT_BYTES {
        return Err("report.json exceeds the ledger scan limit".to_string());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read report.json: {error}"))?;
    if bytes.len() as u64 > MAX_REPORT_BYTES {
        return Err("report.json exceeds the ledger scan limit".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid report.json: {error}"))
}

fn scan_project(
    project: &super::Project,
) -> (Vec<ProjectIssueOccurrence>, Vec<String>, HashSet<String>) {
    let mut warnings = Vec::new();
    // Runs that were not actually re-scanned (purged, unreadable, or beyond
    // the scan caps). Their prior ledger occurrences are retained rather than
    // treated as no-longer-observed.
    let mut skipped_runs = HashSet::new();
    let mut manifests = Vec::new();
    for run_id in &project.run_ids {
        match crate::runs::load_manifest(run_id) {
            Ok(manifest) => manifests.push(manifest),
            Err(error) => {
                skipped_runs.insert(run_id.clone());
                push_warning(
                    &mut warnings,
                    format!("Run {run_id} was skipped while refreshing the ledger: {error}"),
                );
            }
        }
    }
    manifests.sort_by(|left, right| left.created.cmp(&right.created));
    if manifests.len() > MAX_SCANNED_RUNS {
        let skipped = manifests.len() - MAX_SCANNED_RUNS;
        skipped_runs.extend(manifests.drain(..skipped).map(|manifest| manifest.run_id));
        push_warning(
            &mut warnings,
            format!(
                "Only the {MAX_SCANNED_RUNS} most recent runs were scanned; {skipped} older runs remain in the project."
            ),
        );
    }

    let mut occurrences = Vec::new();
    let mut manifests = std::collections::VecDeque::from(manifests);
    while let Some(manifest) = manifests.pop_front() {
        if occurrences.len() >= MAX_LEDGER_OCCURRENCES {
            push_warning(
                &mut warnings,
                format!("Ledger refresh stopped at {MAX_LEDGER_OCCURRENCES} issue occurrences."),
            );
            skipped_runs.insert(manifest.run_id);
            skipped_runs.extend(manifests.drain(..).map(|manifest| manifest.run_id));
            break;
        }
        let report = match read_run_report(&manifest.run_id) {
            Ok(report) => report,
            Err(error) => {
                push_warning(
                    &mut warnings,
                    format!(
                        "Run {} has no readable structured report: {error}",
                        manifest.run_id
                    ),
                );
                skipped_runs.insert(manifest.run_id.clone());
                continue;
            }
        };
        let annotations = read_annotation_map(&manifest.run_id);
        let remaining = MAX_LEDGER_OCCURRENCES - occurrences.len();
        occurrences.extend(
            extract_occurrences(&report, &manifest, &annotations)
                .into_iter()
                .take(remaining),
        );
    }
    (occurrences, warnings, skipped_runs)
}

type AnnotationMap = HashMap<String, (String, String)>;

fn read_annotation_map(run_id: &str) -> AnnotationMap {
    let Ok(content) = crate::runs::read_annotations(run_id) else {
        return HashMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return HashMap::new();
    };
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(id, annotation)| {
                    let status = bounded_text(
                        annotation
                            .get("status")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(""),
                        20,
                    );
                    let note = bounded_text(
                        annotation
                            .get("note")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(""),
                        MAX_NOTE_BYTES,
                    );
                    (id.clone(), (status, note))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_occurrences(
    report: &crate::models::PipelineReport,
    manifest: &crate::runs::RunManifest,
    annotations: &AnnotationMap,
) -> Vec<ProjectIssueOccurrence> {
    let artifact_paths = manifest
        .artifacts
        .iter()
        .map(|artifact| artifact.rel_path.as_str())
        .collect::<HashSet<_>>();
    for output in report.all_outputs().iter().rev() {
        if output.skipped {
            continue;
        }
        let Some(value) = crate::pipeline::structured::extract_json(&output.raw_text) else {
            continue;
        };
        let Some(items) = value
            .as_array()
            .or_else(|| value.get("issues").and_then(serde_json::Value::as_array))
        else {
            continue;
        };
        if items.is_empty() || items.len() > MAX_ISSUES_PER_RUN {
            continue;
        }
        let mut occurrences = Vec::new();
        let mut used_issue_ids = HashMap::<String, usize>::new();
        for (index, item) in items.iter().enumerate() {
            let Some(object) = item.as_object() else {
                continue;
            };
            let title = bounded_value(
                first_value(object, &["title", "summary", "message", "name"]),
                MAX_TITLE_BYTES,
            );
            let body = bounded_value(
                first_value(
                    object,
                    &[
                        "body",
                        "description",
                        "detail",
                        "explanation",
                        "rationale",
                        "recommendation",
                    ],
                ),
                MAX_BODY_BYTES,
            );
            if title.is_empty() && body.is_empty() {
                continue;
            }
            let issue_id = bounded_value(
                first_value(
                    object,
                    &[
                        "id",
                        "issue_id",
                        "issueId",
                        "rule_id",
                        "ruleId",
                        "fingerprint",
                        "key",
                    ],
                ),
                MAX_REFERENCE_BYTES,
            );
            let base_issue_id = if issue_id.is_empty() {
                (index + 1).to_string()
            } else {
                issue_id
            };
            let count = used_issue_ids.entry(base_issue_id.clone()).or_default();
            *count += 1;
            let issue_id = if *count == 1 {
                base_issue_id
            } else {
                bounded_text(&format!("{base_issue_id}#{count}"), MAX_REFERENCE_BYTES)
            };
            let section = bounded_value(
                first_value(
                    object,
                    &[
                        "section",
                        "location",
                        "file",
                        "file_path",
                        "filePath",
                        "path",
                        "criterion",
                        "category",
                    ],
                ),
                MAX_SECTION_BYTES,
            );
            let severity = normalize_severity(&bounded_value(
                first_value(object, &["severity", "priority", "level"]),
                100,
            ));
            let mut evidence = match object.get("evidence") {
                Some(serde_json::Value::Array(values)) => values
                    .iter()
                    .take(MAX_EVIDENCE_PER_ISSUE)
                    .filter_map(|value| parse_evidence(value, &artifact_paths))
                    .collect(),
                Some(value @ serde_json::Value::Object(_)) => {
                    parse_evidence(value, &artifact_paths).into_iter().collect()
                }
                _ => Vec::new(),
            };
            if evidence.len() < MAX_EVIDENCE_PER_ISSUE {
                if let Some(inline) = parse_inline_evidence(object, &artifact_paths) {
                    if !evidence.contains(&inline) {
                        evidence.push(inline);
                    }
                }
            }
            let (annotation_status, annotation_note) =
                annotations.get(&issue_id).cloned().unwrap_or_default();
            occurrences.push(ProjectIssueOccurrence {
                key: occurrence_key(&manifest.run_id, &output.step_id, &issue_id, index),
                run_id: manifest.run_id.clone(),
                issue_id,
                observed_at: manifest.created.clone(),
                profile_id: bounded_text(&manifest.profile_id, MAX_REFERENCE_BYTES),
                profile_name: bounded_text(&manifest.profile_name, MAX_TITLE_BYTES),
                input_name: input_name(&manifest.input_path),
                input_mode: bounded_text(&manifest.input_mode, 100),
                input_interpretation: bounded_text(&manifest.input_interpretation, 100),
                step_id: bounded_text(&output.step_id, MAX_REFERENCE_BYTES),
                step_label: bounded_text(&output.step_label, MAX_TITLE_BYTES),
                title: if title.is_empty() {
                    bounded_text(&body, MAX_TITLE_BYTES)
                } else {
                    title
                },
                severity,
                section,
                body,
                evidence,
                annotation_status,
                annotation_note,
            });
        }
        if !occurrences.is_empty() {
            return occurrences;
        }
    }
    Vec::new()
}

fn parse_evidence(
    value: &serde_json::Value,
    artifact_paths: &HashSet<&str>,
) -> Option<ProjectIssueEvidence> {
    let object = value.as_object()?;
    parse_evidence_object(object, artifact_paths, true)
}

fn parse_inline_evidence(
    object: &serde_json::Map<String, serde_json::Value>,
    artifact_paths: &HashSet<&str>,
) -> Option<ProjectIssueEvidence> {
    const LOCATION_KEYS: &[&str] = &[
        "page",
        "page_number",
        "pageNumber",
        "line",
        "line_start",
        "lineStart",
        "line_end",
        "lineEnd",
        "node_id",
        "nodeId",
        "asset_id",
        "assetId",
        "artifact_path",
        "artifactPath",
        "rel_path",
        "file",
        "file_path",
        "filePath",
        "path",
        "source_path",
        "sourcePath",
    ];
    if !LOCATION_KEYS.iter().any(|key| object.contains_key(*key)) {
        return None;
    }
    parse_evidence_object(object, artifact_paths, false)
}

fn parse_evidence_object(
    object: &serde_json::Map<String, serde_json::Value>,
    artifact_paths: &HashSet<&str>,
    include_description: bool,
) -> Option<ProjectIssueEvidence> {
    let page = bounded_u32(
        first_value(object, &["page", "page_number", "pageNumber"]),
        5_000,
    );
    let line_start = bounded_u32(
        first_value(object, &["line_start", "lineStart", "line"]),
        MAX_SOURCE_LINE,
    );
    let line_end = bounded_u32(
        first_value(object, &["line_end", "lineEnd"]),
        MAX_SOURCE_LINE,
    )
    .filter(|end| line_start.is_some_and(|start| *end >= start));
    let node_id = bounded_value(
        first_value(object, &["node_id", "nodeId"]),
        MAX_REFERENCE_BYTES,
    );
    let asset_id = bounded_value(
        first_value(object, &["asset_id", "assetId"]),
        MAX_REFERENCE_BYTES,
    );
    let raw_artifact_path = bounded_value(
        first_value(
            object,
            &[
                "artifact_path",
                "artifactPath",
                "rel_path",
                "file",
                "file_path",
                "filePath",
                "path",
                "source_path",
                "sourcePath",
            ],
        ),
        MAX_REFERENCE_BYTES,
    );
    let artifact_path = resolve_artifact_path(&raw_artifact_path, artifact_paths);
    let mut description = if include_description {
        bounded_value(
            first_value(object, &["description", "label", "location"]),
            MAX_BODY_BYTES,
        )
    } else {
        bounded_value(object.get("location"), MAX_BODY_BYTES)
    };
    if artifact_path.is_empty() && !raw_artifact_path.is_empty() {
        description = if description.is_empty() || description == raw_artifact_path {
            raw_artifact_path
        } else {
            bounded_text(
                &format!("{raw_artifact_path} — {description}"),
                MAX_BODY_BYTES,
            )
        };
    }
    let quote = bounded_value(object.get("quote"), MAX_BODY_BYTES);
    if page.is_none()
        && line_start.is_none()
        && line_end.is_none()
        && node_id.is_empty()
        && asset_id.is_empty()
        && artifact_path.is_empty()
        && description.is_empty()
        && quote.is_empty()
    {
        return None;
    }
    Some(ProjectIssueEvidence {
        page,
        line_start,
        line_end,
        node_id,
        asset_id,
        artifact_path,
        description,
        quote,
    })
}

fn first_value<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    keys.iter().find_map(|key| object.get(*key))
}

fn bounded_u32(value: Option<&serde_json::Value>, maximum: u32) -> Option<u32> {
    value
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0 && *value <= maximum)
}

fn resolve_artifact_path(value: &str, artifact_paths: &HashSet<&str>) -> String {
    if !safe_artifact_path(value) {
        return String::new();
    }
    if artifact_paths.contains(value) {
        return value.to_string();
    }
    let suffix = format!("/{value}");
    let mut matches = artifact_paths
        .iter()
        .copied()
        .filter(|candidate| candidate.ends_with(&suffix));
    let first = matches.next();
    match (first, matches.next()) {
        (Some(candidate), None) => candidate.to_string(),
        _ => String::new(),
    }
}

fn safe_artifact_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn bounded_value(value: Option<&serde_json::Value>, max_bytes: usize) -> String {
    match value {
        Some(serde_json::Value::String(value)) => bounded_text(value, max_bytes),
        Some(serde_json::Value::Number(value)) => bounded_text(&value.to_string(), max_bytes),
        _ => String::new(),
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    let value = value.trim();
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn normalize_severity(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" | "major" | "severe" | "high" => "high".to_string(),
        "moderate" | "medium" | "warning" => "medium".to_string(),
        "minor" | "low" | "suggestion" | "info" | "informational" => "low".to_string(),
        other => bounded_text(other, 100),
    }
}

fn input_name(input_path: &str) -> String {
    Path::new(input_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| bounded_text(value, MAX_TITLE_BYTES))
        .unwrap_or_else(|| "(no input)".to_string())
}

fn occurrence_key(run_id: &str, step_id: &str, issue_id: &str, index: usize) -> String {
    let digest = Sha256::digest(format!("{run_id}\0{step_id}\0{issue_id}\0{index}").as_bytes());
    format!("occ-{}", hex_prefix(&digest, 24))
}

fn issue_id(project_id: &str, occurrence_key: &str, used: &HashSet<String>) -> String {
    let digest = Sha256::digest(format!("{project_id}\0{occurrence_key}").as_bytes());
    let base = format!("issue-{}", hex_prefix(&digest, 20));
    if !used.contains(&base) {
        return base;
    }
    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    format!(
        "issue-{}",
        hex_prefix(&Sha256::digest(now().as_bytes()), 32)
    )
}

fn hex_prefix(bytes: &[u8], characters: usize) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output.truncate(characters.min(output.len()));
    output
}

fn normalize_identity(value: &str) -> String {
    let mut output = String::new();
    let mut spacing = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if spacing && !output.is_empty() {
                output.push(' ');
            }
            output.push(character);
            spacing = false;
        } else {
            spacing = true;
        }
    }
    output
}

fn semantic_key(occurrence: &ProjectIssueOccurrence) -> Option<String> {
    let title = normalize_identity(&occurrence.title);
    let section = normalize_identity(&occurrence.section);
    let words = title.split_whitespace().count();
    if title.len() < 8 || (section.is_empty() && words < 4) {
        return None;
    }
    Some(format!("{title}\0{section}"))
}

fn stable_source_key(occurrence: &ProjectIssueOccurrence) -> Option<String> {
    let issue_id = normalize_identity(&occurrence.issue_id);
    if issue_id.len() < 3
        || issue_id.chars().all(|character| character.is_ascii_digit())
        || generic_ordinal_id(&issue_id)
    {
        return None;
    }
    Some(format!(
        "{}\0{}\0{}",
        normalize_identity(&occurrence.profile_id),
        normalize_identity(&occurrence.step_id),
        issue_id
    ))
}

fn generic_ordinal_id(value: &str) -> bool {
    let mut words = value.split_whitespace();
    let Some(prefix) = words.next() else {
        return true;
    };
    matches!(
        prefix,
        "issue" | "finding" | "item" | "problem" | "comment" | "warning" | "defect" | "todo"
    ) && words.all(|word| word.chars().all(|character| character.is_ascii_digit()))
}

fn matching_issue_index(
    issues: &[ProjectIssue],
    occurrence: &ProjectIssueOccurrence,
) -> Option<usize> {
    if let Some(source_key) = stable_source_key(occurrence) {
        let matches = issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| {
                issue
                    .occurrences
                    .iter()
                    .filter_map(stable_source_key)
                    .any(|candidate| candidate == source_key)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return matches.first().copied();
        }
    }
    let key = semantic_key(occurrence)?;
    let matches = issues
        .iter()
        .enumerate()
        .filter(|(_, issue)| {
            issue
                .occurrences
                .iter()
                .filter_map(semantic_key)
                .any(|candidate| candidate == key)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0])
}

fn derived_status(occurrences: &[ProjectIssueOccurrence]) -> String {
    let latest = occurrences.last();
    match latest.map(|occurrence| occurrence.annotation_status.as_str()) {
        Some("reject") => "dismissed".to_string(),
        Some("done") => "addressed".to_string(),
        _ if occurrences
            .iter()
            .take(occurrences.len().saturating_sub(1))
            .any(|occurrence| occurrence.annotation_status == "done") =>
        {
            "regressed".to_string()
        }
        _ => "open".to_string(),
    }
}

fn refresh_issue(issue: &mut ProjectIssue, changed: bool) {
    issue.occurrences.sort_by(|left, right| {
        left.observed_at
            .cmp(&right.observed_at)
            .then_with(|| left.key.cmp(&right.key))
    });
    if let Some(latest) = issue.occurrences.last() {
        issue.title = latest.title.clone();
        issue.severity = latest.severity.clone();
        issue.section = latest.section.clone();
    }
    if issue.decision_updated.is_empty() {
        issue.status = derived_status(&issue.occurrences);
    }
    if changed {
        issue.updated = now();
    }
}

fn sync_ledger(
    mut ledger: ProjectIssueLedger,
    project_id: &str,
    occurrences: Vec<ProjectIssueOccurrence>,
    warnings: Vec<String>,
    skipped_runs: &HashSet<String>,
) -> ProjectIssueLedger {
    let mut available = occurrences
        .into_iter()
        .map(|occurrence| (occurrence.key.clone(), occurrence))
        .collect::<HashMap<_, _>>();
    let mut issues = Vec::new();
    for mut issue in ledger.issues.drain(..) {
        let previous = issue.occurrences.clone();
        let mut observed = false;
        issue.occurrences = previous
            .iter()
            .filter_map(|occurrence| match available.remove(&occurrence.key) {
                Some(fresh) => {
                    observed = true;
                    Some(fresh)
                }
                // A run that was not actually re-scanned keeps its last-known
                // occurrences: absence of evidence from an unread run is not a
                // re-scan result.
                None if skipped_runs.contains(&occurrence.run_id) => Some(occurrence.clone()),
                None => None,
            })
            .collect();
        if issue.occurrences.is_empty() {
            // Every occurrence run is gone (purged or removed from the
            // project). Retain the last-known history instead of silently
            // dropping the user's status, decision, and note.
            issue.occurrences = previous.clone();
        }
        let archived = !observed;
        let changed = issue.occurrences != previous || issue.archived != archived;
        issue.archived = archived;
        refresh_issue(&mut issue, changed);
        issues.push(issue);
    }

    let mut pending = available.into_values().collect::<Vec<_>>();
    pending.sort_by(|left, right| {
        left.observed_at
            .cmp(&right.observed_at)
            .then_with(|| left.key.cmp(&right.key))
    });
    let mut used = issues
        .iter()
        .map(|issue| issue.id.clone())
        .collect::<HashSet<_>>();
    for occurrence in pending {
        if let Some(index) = matching_issue_index(&issues, &occurrence) {
            issues[index].occurrences.push(occurrence);
            // A fresh observation reactivates an archived issue.
            issues[index].archived = false;
            refresh_issue(&mut issues[index], true);
            continue;
        }
        let id = issue_id(project_id, &occurrence.key, &used);
        used.insert(id.clone());
        let created = occurrence.observed_at.clone();
        let mut issue = ProjectIssue {
            id,
            title: occurrence.title.clone(),
            severity: occurrence.severity.clone(),
            section: occurrence.section.clone(),
            status: "open".to_string(),
            note: String::new(),
            created: created.clone(),
            updated: created,
            decision_updated: String::new(),
            archived: false,
            occurrences: vec![occurrence],
        };
        refresh_issue(&mut issue, false);
        issues.push(issue);
    }
    issues.sort_by(|left, right| {
        right
            .occurrences
            .last()
            .map(|occurrence| &occurrence.observed_at)
            .cmp(
                &left
                    .occurrences
                    .last()
                    .map(|occurrence| &occurrence.observed_at),
            )
            .then_with(|| left.title.cmp(&right.title))
    });
    ledger.project_id = project_id.to_string();
    ledger.schema_version = LEDGER_SCHEMA_VERSION;
    ledger.updated = now();
    ledger.issues = issues;
    ledger.warnings = warnings;
    ledger
}

fn push_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_WARNINGS {
        warnings.push(bounded_text(&warning, MAX_WARNING_BYTES));
    }
}

fn sync_project_issue_ledger_in(
    projects_dir: &Path,
    project_id: &str,
) -> Result<ProjectIssueLedger, String> {
    let snapshot = super::load_project_from(projects_dir, project_id)?;
    let (occurrences, warnings, skipped_runs) = scan_project(&snapshot);
    let _lock = super::lock_projects(projects_dir)?;
    let current = super::load_project_from(projects_dir, project_id)?;
    if current.run_ids != snapshot.run_ids {
        return Err(
            "Project runs changed while the issue ledger was refreshing; retry.".to_string(),
        );
    }
    let ledger = sync_ledger(
        load_ledger_from(projects_dir, project_id)?,
        project_id,
        occurrences,
        warnings,
        &skipped_runs,
    );
    write_ledger_to(projects_dir, &ledger)?;
    Ok(ledger)
}

#[tauri::command]
pub async fn sync_project_issue_ledger(project_id: String) -> Result<ProjectIssueLedger, String> {
    tokio::task::spawn_blocking(move || {
        let dir = super::projects_dir()?;
        sync_project_issue_ledger_in(&dir, &project_id)
    })
    .await
    .map_err(|error| format!("Project issue-ledger task failed: {error}"))?
}

#[tauri::command]
pub fn update_project_issue(
    project_id: String,
    issue_id: String,
    status: String,
    note: String,
) -> Result<ProjectIssueLedger, String> {
    let dir = super::projects_dir()?;
    let _lock = super::lock_projects(&dir)?;
    let _ = super::load_project_from(&dir, &project_id)?;
    if note.len() > MAX_NOTE_BYTES {
        return Err(format!(
            "Project issue note cannot exceed {MAX_NOTE_BYTES} bytes"
        ));
    }
    let mut ledger = load_ledger_from(&dir, &project_id)?;
    let issue = ledger
        .issues
        .iter_mut()
        .find(|issue| issue.id == issue_id)
        .ok_or("Project issue no longer exists")?;
    issue.note = note.trim().to_string();
    if status == "automatic" {
        issue.decision_updated.clear();
        issue.status = derived_status(&issue.occurrences);
    } else if valid_status(&status) {
        issue.status = status;
        issue.decision_updated = now();
    } else {
        return Err("Invalid project issue status".to_string());
    }
    issue.updated = now();
    ledger.updated = issue.updated.clone();
    write_ledger_to(&dir, &ledger)?;
    Ok(ledger)
}

#[tauri::command]
pub fn merge_project_issues(
    project_id: String,
    source_issue_id: String,
    target_issue_id: String,
) -> Result<ProjectIssueLedger, String> {
    if source_issue_id == target_issue_id {
        return Err("Choose two different project issues to combine".to_string());
    }
    let dir = super::projects_dir()?;
    let _lock = super::lock_projects(&dir)?;
    let _ = super::load_project_from(&dir, &project_id)?;
    let mut ledger = load_ledger_from(&dir, &project_id)?;
    let source_index = ledger
        .issues
        .iter()
        .position(|issue| issue.id == source_issue_id)
        .ok_or("Source project issue no longer exists")?;
    let source = ledger.issues.remove(source_index);
    let target = ledger
        .issues
        .iter_mut()
        .find(|issue| issue.id == target_issue_id)
        .ok_or("Target project issue no longer exists")?;
    let mut keys = target
        .occurrences
        .iter()
        .map(|occurrence| occurrence.key.clone())
        .collect::<HashSet<_>>();
    target.occurrences.extend(
        source
            .occurrences
            .into_iter()
            .filter(|occurrence| keys.insert(occurrence.key.clone())),
    );
    if target.note.is_empty() {
        target.note = source.note;
    } else if !source.note.is_empty() && target.note != source.note {
        target.note = bounded_text(
            &format!("{}\n\nMerged note: {}", target.note, source.note),
            MAX_NOTE_BYTES,
        );
    }
    if target.decision_updated.is_empty() && !source.decision_updated.is_empty() {
        target.status = source.status;
        target.decision_updated = source.decision_updated;
    }
    refresh_issue(target, true);
    ledger.updated = now();
    write_ledger_to(&dir, &ledger)?;
    Ok(ledger)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occurrence(
        key: &str,
        run_id: &str,
        title: &str,
        section: &str,
        input_mode: &str,
    ) -> ProjectIssueOccurrence {
        ProjectIssueOccurrence {
            key: key.to_string(),
            run_id: run_id.to_string(),
            issue_id: "stable-finding".to_string(),
            observed_at: format!("2026-08-0{}T00:00:00Z", run_id.len()),
            profile_id: "review".to_string(),
            profile_name: "Review".to_string(),
            input_name: "input".to_string(),
            input_mode: input_mode.to_string(),
            input_interpretation: input_mode.to_string(),
            step_id: "synthesis".to_string(),
            step_label: "Synthesis".to_string(),
            title: title.to_string(),
            severity: "high".to_string(),
            section: section.to_string(),
            body: "Details".to_string(),
            evidence: Vec::new(),
            annotation_status: String::new(),
            annotation_note: String::new(),
        }
    }

    #[test]
    fn conservative_matching_is_report_type_neutral_and_preserves_decisions() {
        let first = occurrence(
            "occ-aaaaaaaa",
            "run_1",
            "Unbounded retry loop",
            "worker.rs",
            "folder",
        );
        let second = occurrence(
            "occ-bbbbbbbb",
            "run_22",
            "Unbounded retry loop",
            "worker.rs",
            "document",
        );
        let mut ledger = sync_ledger(
            empty_ledger("project"),
            "project",
            vec![first, second],
            Vec::new(),
            &HashSet::new(),
        );
        assert_eq!(ledger.issues.len(), 1);
        assert_eq!(ledger.issues[0].occurrences.len(), 2);
        ledger.issues[0].status = "dismissed".to_string();
        ledger.issues[0].decision_updated = "2026-08-10T00:00:00Z".to_string();

        let refreshed = sync_ledger(
            ledger,
            "project",
            vec![occurrence(
                "occ-bbbbbbbb",
                "run_22",
                "Unbounded retry loop",
                "worker.rs",
                "folder",
            )],
            Vec::new(),
            &HashSet::new(),
        );
        assert_eq!(refreshed.issues.len(), 1);
        assert_eq!(refreshed.issues[0].status, "dismissed");
        assert_eq!(refreshed.issues[0].occurrences.len(), 1);
    }

    #[test]
    fn decisions_survive_purged_and_unreadable_runs() {
        let first = occurrence(
            "occ-aaaaaaaa",
            "run_1",
            "Unbounded retry loop",
            "worker.rs",
            "document",
        );
        let mut ledger = sync_ledger(
            empty_ledger("project"),
            "project",
            vec![first.clone()],
            Vec::new(),
            &HashSet::new(),
        );
        ledger.issues[0].status = "dismissed".to_string();
        ledger.issues[0].decision_updated = "2026-08-10T00:00:00Z".to_string();
        ledger.issues[0].note = "Intentional design".to_string();

        // run_1 could not be re-scanned (unreadable report): retained, not dropped.
        let warned = sync_ledger(
            ledger,
            "project",
            Vec::new(),
            Vec::new(),
            &HashSet::from(["run_1".to_string()]),
        );
        assert_eq!(warned.issues.len(), 1);
        assert!(warned.issues[0].archived);
        assert_eq!(warned.issues[0].occurrences.len(), 1);

        // run_1 was purged from the project entirely: the decision, note, and
        // last-known occurrences survive as an archived issue.
        let purged = sync_ledger(warned, "project", Vec::new(), Vec::new(), &HashSet::new());
        assert_eq!(purged.issues.len(), 1);
        assert!(purged.issues[0].archived);
        assert_eq!(purged.issues[0].status, "dismissed");
        assert_eq!(purged.issues[0].note, "Intentional design");
        assert_eq!(purged.issues[0].occurrences.len(), 1);

        // A fresh observation of the same occurrence reactivates the issue.
        let reobserved = sync_ledger(purged, "project", vec![first], Vec::new(), &HashSet::new());
        assert!(!reobserved.issues[0].archived);
        assert_eq!(reobserved.issues[0].status, "dismissed");
    }

    #[test]
    fn short_generic_titles_do_not_merge_without_a_stable_source_key() {
        let mut first = occurrence("occ-aaaaaaaa", "run_1", "Missing test", "", "folder");
        let mut second = occurrence("occ-bbbbbbbb", "run_22", "Missing test", "", "folder");
        first.issue_id = "1".to_string();
        second.issue_id = "2".to_string();
        let ledger = sync_ledger(
            empty_ledger("project"),
            "project",
            vec![first, second],
            Vec::new(),
            &HashSet::new(),
        );
        assert_eq!(ledger.issues.len(), 2);
    }

    #[test]
    fn generated_ordinal_ids_do_not_override_conservative_matching() {
        let mut first = occurrence(
            "occ-aaaaaaaa",
            "run_1",
            "Missing authorization check",
            "src/api.rs",
            "folder",
        );
        let mut second = occurrence(
            "occ-bbbbbbbb",
            "run_22",
            "Unbounded retry policy",
            "src/worker.rs",
            "folder",
        );
        first.issue_id = "issue-1".to_string();
        second.issue_id = "issue-1".to_string();
        let ledger = sync_ledger(
            empty_ledger("project"),
            "project",
            vec![first, second],
            Vec::new(),
            &HashSet::new(),
        );
        assert_eq!(ledger.issues.len(), 2);
    }

    #[test]
    fn run_annotations_can_mark_an_automatic_regression() {
        let mut first = occurrence(
            "occ-aaaaaaaa",
            "run_1",
            "Identification assumption is unstated",
            "Model",
            "document",
        );
        first.annotation_status = "done".to_string();
        let second = occurrence(
            "occ-bbbbbbbb",
            "run_22",
            "Identification assumption is unstated",
            "Model",
            "document",
        );
        let ledger = sync_ledger(
            empty_ledger("project"),
            "project",
            vec![first, second],
            Vec::new(),
            &HashSet::new(),
        );
        assert_eq!(ledger.issues[0].status, "regressed");
    }

    #[test]
    fn unsafe_evidence_paths_are_not_retained() {
        let value = serde_json::json!({
            "artifact_path": "../outside.txt",
            "page": 4,
            "description": "Relevant source"
        });
        let evidence = parse_evidence(&value, &HashSet::new()).unwrap();
        assert_eq!(evidence.page, Some(4));
        assert!(evidence.artifact_path.is_empty());
    }

    #[test]
    fn source_tree_evidence_resolves_saved_paths_and_retains_lines() {
        let value = serde_json::json!({
            "file": "src/worker.rs",
            "line": 41,
            "line_end": 45,
            "label": "Retry branch"
        });
        let paths = HashSet::from(["artifacts/source/src/worker.rs"]);
        let evidence = parse_evidence(&value, &paths).unwrap();
        assert_eq!(evidence.artifact_path, "artifacts/source/src/worker.rs");
        assert_eq!(evidence.line_start, Some(41));
        assert_eq!(evidence.line_end, Some(45));
        assert_eq!(evidence.description, "Retry branch");
    }

    #[test]
    fn generic_issue_aliases_and_duplicate_ids_match_run_annotations() {
        let report: crate::models::PipelineReport = serde_json::from_value(serde_json::json!({
            "step_outputs": [{
                "step_id": "audit",
                "step_label": "Source audit",
                "raw_text": r#"{"issues":[
                    {"issue_id":"issue-1","message":"Retry loop has no bound","priority":"critical","file":"src/worker.rs","explanation":"Terminal failures retry forever.","line":41},
                    {"issue_id":"issue-1","summary":"Missing authorization check","level":"medium","path":"src/api.rs","detail":"The handler trusts the caller."}
                ]}"#
            }]
        }))
        .unwrap();
        let manifest: crate::runs::RunManifest = serde_json::from_value(serde_json::json!({
            "run_id": "run_source",
            "created": "2026-08-10T00:00:00Z",
            "input_path": "/work/repository",
            "input_mode": "folder",
            "input_interpretation": "source_tree",
            "profile_id": "code-review",
            "profile_name": "Code Review",
            "provider": "codex",
            "artifacts": [{
                "rel_path": "artifacts/source/src/worker.rs",
                "label": "worker.rs",
                "kind": "code",
                "bytes": 100,
                "sha256": "abc",
                "group": "source"
            }]
        }))
        .unwrap();
        let annotations = HashMap::from([(
            "issue-1#2".to_string(),
            ("done".to_string(), "Fixed in the next revision".to_string()),
        )]);

        let occurrences = extract_occurrences(&report, &manifest, &annotations);
        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[0].title, "Retry loop has no bound");
        assert_eq!(occurrences[0].severity, "high");
        assert_eq!(occurrences[0].section, "src/worker.rs");
        assert_eq!(
            occurrences[0].evidence[0].artifact_path,
            "artifacts/source/src/worker.rs"
        );
        assert_eq!(occurrences[0].evidence[0].line_start, Some(41));
        assert_eq!(occurrences[1].issue_id, "issue-1#2");
        assert_eq!(occurrences[1].annotation_status, "done");
    }
}
