//! Records kept by project views that were retired in the projects overhaul.
//!
//! Theory notes, experiments, literature notes, claims, decisions and the other
//! structured records no longer have a view. Their tables stay, so backups and
//! project packages keep restoring, and nothing here changes or deletes a row.
//! `export` writes them as Markdown into a new folder, and
//! `convert_abandoned_approaches` copies abandoned theory approaches into notes
//! so the assistant keeps seeing them.
use super::{decode, err, records_up_to, scope, RECORD_READ_LIMIT};
use crate::workbench::research;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq)]
enum Source {
    /// `project_records`, one row per record, revised in place.
    Project,
    /// `desk_records`, append-only: a newer row supersedes an older one.
    Desk,
    /// `claims` with their current version and evidence links.
    Claims,
}

/// Every retired record kind, in the order the export lists them.
const KINDS: &[(Source, &str, &str)] = &[
    (Source::Project, "theory", "Theory notes"),
    (Source::Project, "check", "Theory checks"),
    (Source::Project, "direction", "Research directions"),
    (Source::Desk, "symbol", "Symbols"),
    (Source::Desk, "assumption_branch", "Assumption branches"),
    (Source::Desk, "decision", "Decisions"),
    (Source::Desk, "relation", "Dependencies"),
    (Source::Claims, "claim", "Claims and evidence"),
    (Source::Project, "literature", "Literature notes"),
    (Source::Project, "bibliography", "Bibliography entries"),
    (Source::Desk, "collection", "Reading collections"),
    (Source::Desk, "acquisition", "Reading inbox"),
    (Source::Project, "experiment", "Experiments"),
    (Source::Project, "specification", "Specifications"),
    (Source::Project, "series", "Result series"),
    (Source::Project, "binding", "Result links"),
    (Source::Desk, "experiment_plan", "Specification grids"),
    (Source::Desk, "execution_plan", "Saved analysis plans"),
    (Source::Desk, "dataset", "Datasets"),
    (Source::Desk, "sample", "Samples"),
    (Source::Desk, "publication_asset", "Tables and figures"),
    (
        Source::Desk,
        "asset_inclusion",
        "Table and figure placements",
    ),
    (Source::Desk, "figure_recipe", "Figure recipes"),
    (Source::Desk, "deliverable", "Drafts"),
    (Source::Desk, "deliverable_role", "Document roles"),
    (Source::Desk, "kit_install", "Starter kits"),
    (Source::Desk, "revision_campaign", "Revision automations"),
    (Source::Desk, "monitor", "Scheduled checks"),
    (Source::Desk, "coauthor_review", "Coauthor reviews"),
    (
        Source::Desk,
        "capsule_import",
        "Imported replication packages",
    ),
];

/// Fields too large or too mechanical to be worth reading in Markdown.
const OMITTED_FIELDS: &[&str] = &["rows", "previewRows", "promotions"];
/// A desk row that no newer row supersedes.
const DESK_CURRENT: &str =
    "NOT EXISTS (SELECT 1 FROM desk_records n WHERE n.supersedes=d.id AND n.workspace_id=d.workspace_id)";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EarlierRecordKind {
    pub kind: String,
    pub label: String,
    pub count: usize,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EarlierRecords {
    /// Kinds with at least one record, in export order.
    pub kinds: Vec<EarlierRecordKind>,
    pub total: usize,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EarlierRecordsExport {
    /// The new folder the files were written into.
    pub directory: String,
    pub files: Vec<String>,
    pub records: usize,
    /// Abandoned approaches newly copied into notes by this export.
    pub notes_created: usize,
}

/// How many records of each retired kind the project holds.
pub fn earlier_records(store: &Store, ws: &str) -> WorkbenchResult<EarlierRecords> {
    scope(store, ws)?;
    let conn = store.connection()?;
    let mut kinds = Vec::new();
    for &(source, kind, label) in KINDS {
        let count: i64 = match source {
            Source::Project => conn.query_row(
                "SELECT COUNT(*) FROM project_records WHERE workspace_id=?1 AND kind=?2",
                params![ws, kind],
                |r| r.get(0),
            ),
            Source::Desk => conn.query_row(
                &format!("SELECT COUNT(*) FROM desk_records d WHERE workspace_id=?1 AND kind=?2 AND {DESK_CURRENT}"),
                params![ws, kind],
                |r| r.get(0),
            ),
            Source::Claims => conn.query_row(
                "SELECT COUNT(*) FROM claims WHERE workspace_id=?1",
                [ws],
                |r| r.get(0),
            ),
        }
        .map_err(err)?;
        if count > 0 {
            kinds.push(EarlierRecordKind {
                kind: kind.into(),
                label: label.into(),
                count: count as usize,
            });
        }
    }
    let total = kinds.iter().map(|k| k.count).sum();
    Ok(EarlierRecords { kinds, total })
}

struct Entry {
    id: String,
    title: Option<String>,
    date: String,
    body: Value,
}

fn entries(store: &Store, ws: &str, source: Source, kind: &str) -> WorkbenchResult<Vec<Entry>> {
    let conn = store.connection()?;
    let limit = RECORD_READ_LIMIT as i64;
    let parse = |text: String| serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
    match source {
        Source::Project => {
            let mut q = conn
                .prepare("SELECT id,body_json,updated_at FROM project_records WHERE workspace_id=?1 AND kind=?2 ORDER BY updated_at DESC,id LIMIT ?3")
                .map_err(err)?;
            let rows = q
                .query_map(params![ws, kind, limit], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            Ok(rows
                .into_iter()
                .map(|(id, body, date)| Entry {
                    id,
                    title: None,
                    date,
                    body: parse(body),
                })
                .collect())
        }
        Source::Desk => {
            let mut q = conn
                .prepare(&format!("SELECT id,title,body_json,created_at FROM desk_records d WHERE workspace_id=?1 AND kind=?2 AND {DESK_CURRENT} ORDER BY created_at DESC,id LIMIT ?3"))
                .map_err(err)?;
            let rows = q
                .query_map(params![ws, kind, limit], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get(3)?,
                    ))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            Ok(rows
                .into_iter()
                .map(|(id, title, body, date)| Entry {
                    id,
                    title: Some(title).filter(|t| !t.trim().is_empty()),
                    date,
                    body: parse(body),
                })
                .collect())
        }
        Source::Claims => {
            let mut q = conn
                .prepare("SELECT c.id,c.workflow_state,c.updated_at,v.id,v.claim_text,v.kind,v.origin,v.version FROM claims c LEFT JOIN claim_versions v ON v.id=c.current_version_id WHERE c.workspace_id=?1 ORDER BY c.updated_at DESC,c.id LIMIT ?2")
                .map_err(err)?;
            let claims = q
                .query_map(params![ws, limit], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<String>>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, Option<i64>>(7)?,
                    ))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let mut links = conn
                .prepare("SELECT relation,target_type,target_id,assessment,assessor,freshness,stale_reason FROM evidence_links WHERE claim_version_id=?1 ORDER BY created_at,id")
                .map_err(err)?;
            let mut out = Vec::with_capacity(claims.len());
            for (id, state, date, version_id, text, kind, origin, version) in claims {
                let evidence = match &version_id {
                    Some(v) => links
                        .query_map([v], |r| {
                            Ok(json!({
                                "relation": r.get::<_, String>(0)?,
                                "target": format!("{} {}", r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                                "assessment": r.get::<_, String>(3)?,
                                "assessor": r.get::<_, String>(4)?,
                                "freshness": r.get::<_, String>(5)?,
                                "staleReason": r.get::<_, Option<String>>(6)?,
                            }))
                        })
                        .map_err(err)?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(err)?,
                    None => Vec::new(),
                };
                out.push(Entry {
                    id,
                    title: None,
                    date,
                    body: json!({
                        "statement": text,
                        "kind": kind,
                        "state": state,
                        "origin": origin,
                        "version": version,
                        "evidence": evidence,
                    }),
                });
            }
            Ok(out)
        }
    }
}

/// Writes every retired record as Markdown into a new folder inside
/// `destination`, one file per kind and a README. Existing files are never
/// replaced. Also copies abandoned approaches into notes.
pub fn export(
    store: &Store,
    ws: &str,
    destination: &Path,
) -> WorkbenchResult<EarlierRecordsExport> {
    let summary = earlier_records(store, ws)?;
    if summary.total == 0 {
        return Err(WorkbenchError::invalid(
            "This project has no earlier research records to export",
        ));
    }
    if !destination.is_absolute() || !destination.is_dir() {
        return Err(WorkbenchError::invalid("Choose an existing folder"));
    }
    let project = store.workspace(ws)?.name;
    let notes_created = convert_abandoned_approaches(store, ws)?;
    let directory = new_folder(destination)?;
    let today = chrono::Local::now().format("%B %-d, %Y").to_string();
    let mut files = Vec::new();
    let mut index = String::new();
    for kind in &summary.kinds {
        let &(source, _, label) = KINDS
            .iter()
            .find(|(_, k, _)| *k == kind.kind)
            .expect("summary kinds come from KINDS");
        let records = entries(store, ws, source, &kind.kind)?;
        let name = format!("{}.md", file_stem(label));
        write_new(
            &directory.join(&name),
            &render_kind(label, &project, &today, &records),
        )?;
        index.push_str(&format!(
            "- [{label}]({}) · {}\n",
            name.replace(' ', "%20"),
            count(records.len())
        ));
        files.push(name);
    }
    let readme = format!(
        "# Earlier research records\n\nExported from Pipeline on {today} for the project \"{project}\".\n\n\
         Pipeline no longer has views for these records. These files are copies: the records themselves stay in Pipeline's \
         database, so backups and project packages still include them. Exporting again writes a new folder.\n\n{index}"
    );
    write_new(&directory.join("README.md"), &readme)?;
    files.insert(0, "README.md".into());
    Ok(EarlierRecordsExport {
        directory: directory.to_string_lossy().into_owned(),
        files,
        records: summary.total,
        notes_created,
    })
}

fn new_folder(destination: &Path) -> WorkbenchResult<PathBuf> {
    for n in 1..=100 {
        let name = if n == 1 {
            "Earlier research records".to_string()
        } else {
            format!("Earlier research records {n}")
        };
        let path = destination.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(err(e)),
        }
    }
    Err(WorkbenchError::conflict(
        "This folder already holds 100 earlier-records exports; choose another folder",
    ))
}

fn write_new(path: &Path, text: &str) -> WorkbenchResult<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(err)?;
    file.write_all(text.as_bytes()).map_err(err)?;
    file.sync_all().map_err(err)
}

fn file_stem(label: &str) -> String {
    label.to_lowercase().replace(' ', "-")
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 record".into()
    } else {
        format!("{n} records")
    }
}

fn render_kind(label: &str, project: &str, today: &str, records: &[Entry]) -> String {
    let mut out = format!(
        "# {label}\n\nFrom the project \"{project}\", exported {today}. {}, newest first.\n",
        count(records.len())
    );
    if records.len() == RECORD_READ_LIMIT {
        out.push_str(&format!(
            "\nOnly the newest {RECORD_READ_LIMIT} records are included.\n"
        ));
    }
    for entry in records {
        out.push('\n');
        render_entry(&mut out, entry);
    }
    out
}

/// The fields that can name a record, in the order they are tried.
const TITLE_FIELDS: &[&str] = &[
    "title",
    "name",
    "label",
    "statement",
    "objective",
    "question",
    "key",
];

fn title_of(entry: &Entry) -> (String, Option<&'static str>) {
    if let Some(title) = &entry.title {
        return (one_line(title), None);
    }
    if let Value::Object(map) = &entry.body {
        for &field in TITLE_FIELDS {
            if let Some(Value::String(s)) = map.get(field) {
                if !s.trim().is_empty() {
                    // A long statement stays in the body; the heading shows its start.
                    let keep = s.chars().count() > 120 || s.contains('\n');
                    return (one_line(s), (!keep).then_some(field));
                }
            }
        }
        // Bibliography entries keep their fields one level down.
        if let Some(Value::Object(e)) = map.get("entry") {
            let key = e.get("key").and_then(Value::as_str).unwrap_or("");
            let title = e
                .get("fields")
                .and_then(|f| f.get("title"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let text = [key, title]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(": ");
            if !text.is_empty() {
                return (one_line(&text), None);
            }
        }
    }
    (entry.id.clone(), None)
}

fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 120 {
        format!("{}…", flat.chars().take(119).collect::<String>())
    } else {
        flat
    }
}

fn render_entry(out: &mut String, entry: &Entry) {
    let (title, used) = title_of(entry);
    out.push_str(&format!("## {title}\n\n"));
    let day = entry.date.get(..10).unwrap_or(&entry.date);
    let mut list = vec![format!("- Record `{}`, last changed {day}", entry.id)];
    let mut blocks = Vec::new();
    match &entry.body {
        Value::Object(map) => {
            for (field, value) in map {
                if Some(field.as_str()) == used || OMITTED_FIELDS.contains(&field.as_str()) {
                    continue;
                }
                field_markdown(&humanize(field), value, &mut list, &mut blocks);
            }
        }
        other => field_markdown("Content", other, &mut list, &mut blocks),
    }
    out.push_str(&list.join("\n"));
    out.push('\n');
    for block in blocks {
        out.push('\n');
        out.push_str(&block);
    }
}

fn empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if s.chars().count() <= 160 && !s.contains('\n') => {
            Some(s.trim().to_string())
        }
        Value::Bool(b) => Some(if *b { "yes" } else { "no" }.into()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn field_markdown(label: &str, value: &Value, list: &mut Vec<String>, blocks: &mut Vec<String>) {
    if empty(value) {
        return;
    }
    if let Some(text) = scalar(value) {
        list.push(format!("- **{label}:** {text}"));
        return;
    }
    match value {
        Value::String(s) => blocks.push(format!("**{label}**\n\n{}\n", s.trim())),
        Value::Array(items) if items.iter().all(|i| scalar(i).is_some()) => {
            list.push(format!("- **{label}:**"));
            for item in items {
                list.push(format!("  - {}", scalar(item).unwrap_or_default()));
            }
        }
        Value::Object(map) if map.values().all(|v| empty(v) || scalar(v).is_some()) => {
            list.push(format!("- **{label}:**"));
            for (k, v) in map {
                if let Some(text) = scalar(v).filter(|t| !t.is_empty()) {
                    list.push(format!("  - {}: {text}", humanize(k)));
                }
            }
        }
        other => {
            let pretty = serde_json::to_string_pretty(other).unwrap_or_default();
            let fence = "`".repeat(longest_backtick_run(&pretty).max(2) + 1);
            blocks.push(format!("**{label}**\n\n{fence}json\n{pretty}\n{fence}\n"));
        }
    }
}

fn longest_backtick_run(text: &str) -> usize {
    let (mut best, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// `rejectionReason` → "Rejection reason", `sample_id` → "Sample id".
fn humanize(field: &str) -> String {
    let mut words = String::new();
    for (i, c) in field.chars().enumerate() {
        if c == '_' || c == '-' {
            words.push(' ');
        } else if c.is_uppercase() && i > 0 {
            words.push(' ');
            words.extend(c.to_lowercase());
        } else if i == 0 {
            words.extend(c.to_uppercase());
        } else {
            words.push(c);
        }
    }
    words
}

/// The origin that marks a note copied from a theory record.
fn theory_origin(record_id: &str) -> String {
    format!("theory:{record_id}")
}

/// Copies each abandoned theory approach into an accepted, pinned decision
/// note, so the assistant still reads why it was dropped. A record already
/// copied is skipped, even if its note was later edited or retired.
pub fn convert_abandoned_approaches(store: &Store, ws: &str) -> WorkbenchResult<usize> {
    let mut created = 0;
    for record in records_up_to(store, ws, "theory", RECORD_READ_LIMIT)? {
        let Ok(note) = decode::<super::TheoryNote>(&record) else {
            continue;
        };
        if note.status != "abandoned" {
            continue;
        }
        let origin = theory_origin(&record.id);
        let copied = store
            .connection()?
            .query_row(
                "SELECT 1 FROM research_notes WHERE workspace_id=?1 AND origin=?2",
                params![ws, origin],
                |_| Ok(()),
            )
            .optional()
            .map_err(err)?
            .is_some();
        if copied {
            continue;
        }
        research::create_note(
            store,
            research::CreateNoteRequest {
                workspace_id: ws.into(),
                paper_id: None,
                kind: "decision".into(),
                body: abandoned_note_body(&note),
                state: Some("accepted".into()),
                origin,
                pinned: true,
                operation_id: format!("theory-note-{}", record.id),
            },
            false,
        )?;
        created += 1;
    }
    Ok(created)
}

fn abandoned_note_body(note: &super::TheoryNote) -> String {
    let mut body = format!("Abandoned approach: {}", note.title.trim());
    if !note.statement.trim().is_empty() {
        body.push_str(&format!("\n\n{}", note.statement.trim()));
    }
    let reason = note.rejection_reason.trim();
    body.push_str(&format!(
        "\n\nWhy it was dropped: {}",
        if reason.is_empty() {
            "not recorded"
        } else {
            reason
        }
    ));
    if !note.assumptions.is_empty() {
        body.push_str(&format!("\nAssumptions: {}", note.assumptions.join("; ")));
    }
    // Notes hold at most 64 KiB.
    const LIMIT: usize = 60 * 1024;
    if body.len() > LIMIT {
        let mut end = LIMIT;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push('…');
    }
    body
}

/// Runs the conversion for every open project. Called once at startup; a
/// project that fails is reported and skipped.
pub fn convert_all_abandoned_approaches(store: &Store) {
    let Ok(list) = store.list_workspaces(false) else {
        return;
    };
    for workspace in list.workspaces {
        if let Err(e) = convert_abandoned_approaches(store, &workspace.id) {
            eprintln!(
                "Abandoned approaches in a project could not be copied to notes: {}",
                e.message
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use super::*;
    use crate::workbench::desk;

    #[test]
    fn export_writes_current_records_as_markdown_without_replacing_files() {
        let f = fixture();
        super::super::put(
            &f.store,
            &f.ws,
            "theory_one",
            "theory",
            0,
            &json!({
                "kind": "rejected_approach",
                "title": "Contraction argument",
                "statement": "The Bellman operator is a contraction.",
                "status": "abandoned",
                "rejectionReason": "The modulus exceeds one.",
                "assumptions": ["Bounded rewards"],
            }),
        )
        .unwrap();
        let first = desk::insert(
            &f.store,
            &f.ws,
            "decision",
            "Cluster by state",
            json!({"statement": "Cluster by county", "state": "proposed"}),
            None,
            "decide-1",
        )
        .unwrap();
        desk::insert(
            &f.store,
            &f.ws,
            "decision",
            "Cluster by state",
            json!({"statement": "Cluster by state", "rationale": "Policy varies by state.\nCounties share it.", "state": "accepted"}),
            Some(&first.id),
            "decide-2",
        )
        .unwrap();

        let summary = earlier_records(&f.store, &f.ws).unwrap();
        let counts: Vec<_> = summary
            .kinds
            .iter()
            .map(|k| (k.kind.as_str(), k.count))
            .collect();
        // The superseded decision is not counted.
        assert_eq!(counts, [("theory", 1), ("decision", 1)]);

        let out = tempfile::tempdir().unwrap();
        let written = export(&f.store, &f.ws, out.path()).unwrap();
        assert_eq!(written.records, 2);
        assert_eq!(written.notes_created, 1);
        assert_eq!(
            written.files,
            ["README.md", "theory-notes.md", "decisions.md"]
        );
        let dir = PathBuf::from(&written.directory);
        assert_eq!(dir, out.path().join("Earlier research records"));
        let theory = fs::read_to_string(dir.join("theory-notes.md")).unwrap();
        assert!(theory.contains("## Contraction argument"));
        assert!(theory.contains("- **Rejection reason:** The modulus exceeds one."));
        assert!(theory.contains("  - Bounded rewards"));
        let decisions = fs::read_to_string(dir.join("decisions.md")).unwrap();
        assert!(decisions.contains("## Cluster by state"));
        assert!(decisions.contains("**Rationale**\n\nPolicy varies by state.\nCounties share it."));
        assert!(!decisions.contains("Cluster by county"));
        assert!(fs::read_to_string(dir.join("README.md"))
            .unwrap()
            .contains("[Theory notes](theory-notes.md) · 1 record"));

        // A second export goes to a new folder and copies no note twice.
        let again = export(&f.store, &f.ws, out.path()).unwrap();
        assert!(again.directory.ends_with("Earlier research records 2"));
        assert_eq!(again.notes_created, 0);
        assert_eq!(
            theory,
            fs::read_to_string(dir.join("theory-notes.md")).unwrap()
        );
    }

    #[test]
    fn export_refuses_a_project_with_nothing_to_export() {
        let f = fixture();
        assert_eq!(earlier_records(&f.store, &f.ws).unwrap().total, 0);
        let out = tempfile::tempdir().unwrap();
        assert!(export(&f.store, &f.ws, out.path()).is_err());
        assert_eq!(fs::read_dir(out.path()).unwrap().count(), 0);
    }

    #[test]
    fn humanize_splits_camel_and_snake_case() {
        assert_eq!(humanize("rejectionReason"), "Rejection reason");
        assert_eq!(humanize("sample_id"), "Sample id");
    }
}
