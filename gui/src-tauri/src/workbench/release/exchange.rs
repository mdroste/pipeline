//! Selective project exchange: a `.pwex` package of one Workspace's selected
//! research objects with a previewed dependency closure, importable into a
//! nonempty store under a source namespace with ID remapping, duplicate
//! detection, and reviewable conflicts.
//!
//! The package never contains conversations, credentials, native runtime state,
//! execution profiles or authorizations, or declared raw execution inputs. It
//! stays readable without Pipeline: `README.md` summarizes every object and
//! `objects/<kind>/<id>.json` holds the structured records.
use super::archive::safe_archive_name;
use super::now;
use crate::workbench::project;
use crate::workbench::research;
use crate::workbench::store::{
    CreateWorkspaceRequest, Store, WorkbenchError, WorkbenchResult, CURRENT_SCHEMA_VERSION,
};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

pub const EXCHANGE_FORMAT: &str = "pipeline-project-exchange";
pub const EXCHANGE_FORMAT_VERSION: u32 = 1;
const MAX_OBJECTS: usize = 5000;
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BLOB_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const CAPTURED_OUTPUT_LIMIT: usize = 64 * 1024;
pub const RECORD_KINDS: [&str; 15] = [
    "task",
    "anchor",
    "checkpoint",
    "application",
    "build",
    "response",
    "experiment",
    "specification",
    "series",
    "binding",
    "bibliography",
    "literature",
    "theory",
    "check",
    "direction",
];

fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::storage("Project exchange failed", e)
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> WorkbenchResult<(String, u64)> {
    let mut file = File::open(path).map_err(err)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(err)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), total))
}
fn random_id(prefix: &str) -> WorkbenchResult<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(format!(
        "{prefix}_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
fn sort_json(value: &mut Value) {
    match value {
        Value::Object(object) => {
            let mut entries = std::mem::take(object).into_iter().collect::<Vec<_>>();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (key, mut child) in entries {
                sort_json(&mut child);
                object.insert(key, child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sort_json),
        _ => {}
    }
}
/// Content fingerprint that ignores volatile bookkeeping so identical research
/// content compares equal across stores.
pub fn fingerprint(value: &Value) -> String {
    let mut v = value.clone();
    fn strip(v: &mut Value) {
        match v {
            Value::Object(o) => {
                for key in [
                    "workspaceId",
                    "updatedAt",
                    "createdAt",
                    "revision",
                    "textReference",
                    "storageReference",
                    "cwd",
                    "sessionId",
                    "profileId",
                    "artifactId",
                    "compiledArtifactId",
                    "entrypoint",
                    "paperId",
                ] {
                    o.remove(key);
                }
                o.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut v);
    sort_json(&mut v);
    hex(v.to_string().as_bytes())
}
/// A stable per-store namespace so two machines never share an ID namespace.
pub fn instance_id(store: &Store) -> WorkbenchResult<String> {
    let path = store.root_path().join("instance.id");
    if let Ok(existing) = fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if trimmed.len() == 32 && trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(trimmed.to_string());
        }
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(err)?;
    let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    fs::write(&path, &id).map_err(err)?;
    Ok(id)
}
fn validate_path(path: &str, extension: &str) -> WorkbenchResult<PathBuf> {
    let p = Path::new(path);
    if !p.is_absolute() || p.extension().and_then(|e| e.to_str()) != Some(extension) {
        return Err(WorkbenchError::invalid(format!(
            "Choose an absolute .{extension} path"
        )));
    }
    Ok(p.to_path_buf())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExchangeSelection {
    #[serde(default)]
    pub include_notes: bool,
    #[serde(default)]
    pub record_kinds: Vec<String>,
    #[serde(default)]
    pub paper_ids: Vec<String>,
    #[serde(default)]
    pub execution_ids: Vec<String>,
    #[serde(default)]
    pub include_sources: bool,
    #[serde(default)]
    pub include_ledger: bool,
    #[serde(default)]
    pub include_compiled_pdfs: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReference {
    pub kind: String,
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectEntry {
    pub kind: String,
    pub id: String,
    pub fingerprint: String,
    pub title: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobEntry {
    pub archive_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub role: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeManifest {
    pub format: String,
    pub format_version: u32,
    pub store_schema_version: u32,
    pub created_at: String,
    pub source_namespace: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_root: Option<String>,
    pub selection: ExchangeSelection,
    pub objects: Vec<ObjectEntry>,
    pub blobs: Vec<BlobEntry>,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
    pub reproducibility: String,
}

struct Package {
    objects: BTreeMap<(String, String), Value>,
    titles: BTreeMap<(String, String), String>,
    blobs: BTreeMap<String, (PathBuf, String, u64, String)>,
    external: Vec<ExternalReference>,
    limitations: BTreeSet<String>,
}

struct Index {
    share_data: bool,
    records: HashMap<String, String>,
    notes: HashSet<String>,
    executions: HashSet<String>,
    revisions: HashMap<String, String>,
    papers: HashSet<String>,
    source_versions: HashMap<String, String>,
    sources: HashSet<String>,
    artifacts: HashMap<String, (String, String, u64, String)>,
    claims: HashSet<String>,
    claim_versions: HashMap<String, String>,
    evidence: HashSet<String>,
}
fn index(store: &Store, ws: &str) -> WorkbenchResult<Index> {
    let conn = store.connection()?;
    let mut records = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT id,kind FROM project_records WHERE workspace_id=?1")
        .map_err(err)?;
    for row in stmt
        .query_map([ws], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(err)?
    {
        let (id, kind) = row.map_err(err)?;
        records.insert(id, kind);
    }
    let list = |sql: &str| -> WorkbenchResult<Vec<(String, String)>> {
        let mut stmt = conn.prepare(sql).map_err(err)?;
        let rows = stmt
            .query_map([ws], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(rows)
    };
    let notes = list("SELECT id,id FROM research_notes WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let executions = list("SELECT id,id FROM research_executions WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let revisions = list("SELECT pr.id,pr.paper_id FROM paper_revisions pr JOIN papers p ON p.id=pr.paper_id WHERE p.workspace_id=?1")?.into_iter().collect();
    let papers = list("SELECT id,id FROM papers WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let source_versions = list("SELECT v.id,v.source_id FROM source_versions v JOIN sources s ON s.id=v.source_id WHERE s.workspace_id=?1")?.into_iter().collect();
    let sources = list("SELECT id,id FROM sources WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let mut artifacts = HashMap::new();
    let mut stmt = conn.prepare("SELECT id,storage_reference,content_hash,size_bytes,origin FROM artifacts WHERE workspace_id=?1").map_err(err)?;
    for row in stmt
        .query_map([ws], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(err)?
    {
        let (id, reference, hash, size, origin) = row.map_err(err)?;
        artifacts.insert(id, (reference, hash, size.max(0) as u64, origin));
    }
    let claims = list("SELECT id,id FROM claims WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let claim_versions = list("SELECT cv.id,cv.claim_id FROM claim_versions cv JOIN claims c ON c.id=cv.claim_id WHERE c.workspace_id=?1")?.into_iter().collect();
    let evidence = list("SELECT id,id FROM evidence_links WHERE workspace_id=?1")?
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    Ok(Index {
        share_data: crate::workbench::data::policy(store, ws)?.package_data,
        records,
        notes,
        executions,
        revisions,
        papers,
        source_versions,
        sources,
        artifacts,
        claims,
        claim_versions,
        evidence,
    })
}

fn walk_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>, depth: usize) {
    if depth > 24 {
        return;
    }
    match value {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|v| walk_strings(v, out, depth + 1)),
        Value::Object(o) => o.values().for_each(|v| walk_strings(v, out, depth + 1)),
        _ => {}
    }
}
fn truncate(text: Option<String>, note: &mut BTreeSet<String>, label: &str) -> Option<String> {
    text.map(|t| {
        if t.len() > CAPTURED_OUTPUT_LIMIT {
            note.insert(format!(
                "Captured {label} was truncated to 64 KiB in the package; the exporting store keeps the full text."
            ));
            let mut end = CAPTURED_OUTPUT_LIMIT;
            while !t.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}\n[truncated]", &t[..end])
        } else {
            t
        }
    })
}

fn load_object(
    store: &Store,
    ws: &str,
    ix: &Index,
    kind: &str,
    id: &str,
    pkg: &mut Package,
) -> WorkbenchResult<Option<(Value, String)>> {
    let conn = store.connection()?;
    Ok(match kind {
        "note" => {
            let notes = research::list_notes(store, ws, true)?;
            notes.into_iter().find(|n| n.id == id).map(|n| {
                let title = n.body.chars().take(80).collect::<String>();
                (serde_json::to_value(n).unwrap_or(Value::Null), title)
            })
        }
        "paper" => {
            let papers = research::list_papers(store, ws)?;
            let Some(p) = papers.into_iter().find(|p| p.paper.id == id) else {
                return Ok(None);
            };
            let mut stmt = conn.prepare("SELECT id,paper_id,input_kind,entrypoint,dependency_manifest_json,content_hash,text_reference,compiled_artifact_id,extraction_json,capture_complete,captured_at FROM paper_revisions WHERE paper_id=?1 ORDER BY captured_at").map_err(err)?;
            let revisions = stmt
                .query_map([id], research::revision_from_row)
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let mut encoded = Vec::new();
            for r in revisions {
                let mut v = serde_json::to_value(&r).map_err(err)?;
                if let Some(reference) = &r.text_reference {
                    let path = PathBuf::from(reference);
                    if path.is_file() {
                        let (hash, size) = hash_file(&path)?;
                        let name = format!("blobs/{hash}.txt");
                        pkg.blobs
                            .insert(name.clone(), (path, hash, size, "paper_text".into()));
                        v["textReference"] = Value::String(name);
                    } else {
                        v["textReference"] = Value::Null;
                        pkg.limitations.insert(format!(
                            "Extracted text for paper revision {} was unavailable at export.",
                            r.id
                        ));
                    }
                }
                if let Some(aid) = &r.compiled_artifact_id {
                    if let Some((_, hash, _, _)) = ix.artifacts.get(aid) {
                        v["compiledSha256"] = Value::String(hash.clone());
                    }
                }
                v["entrypoint"] = Value::String(
                    Path::new(&r.entrypoint)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                encoded.push(v);
            }
            let title = p.paper.title.clone();
            let mut v = serde_json::to_value(&p.paper).map_err(err)?;
            v["revisions"] = Value::Array(encoded);
            Some((v, title))
        }
        "execution" => {
            let e = research::get_execution(store, id)?;
            if e.workspace_id != ws {
                return Ok(None);
            }
            let mut v = serde_json::to_value(&e).map_err(err)?;
            v["stdout"] = json!(truncate(e.stdout.clone(), &mut pkg.limitations, "stdout"));
            v["stderr"] = json!(truncate(e.stderr.clone(), &mut pkg.limitations, "stderr"));
            v["profileId"] = Value::Null;
            v["sessionId"] = Value::Null;
            let mut stmt = conn.prepare("SELECT result_id,body_json FROM structured_results WHERE execution_id=?1 ORDER BY result_id").map_err(err)?;
            let results = stmt
                .query_map([id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let mut encoded = Vec::new();
            for (result_id, body) in results {
                encoded.push(json!({"resultId":result_id,"body":serde_json::from_str::<Value>(&body).map_err(err)?}));
            }
            v["structuredResults"] = Value::Array(encoded);
            if let Some(inputs) = e.input_manifest.get("inputs").and_then(Value::as_array) {
                for input in inputs {
                    let path = input
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or("declared input");
                    pkg.external.push(ExternalReference {
                        kind: "execution_input".into(),
                        id: format!("{id}:{path}"),
                        reason: "Declared execution input (possibly raw data) is not packaged; its hash remains in the receipt".into(),
                    });
                }
            }
            for a in e.output_manifest["artifacts"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(aid) = a["artifactId"].as_str() {
                    include_artifact(ix, aid, "execution_output", pkg);
                }
            }
            let title = format!("{} · {}", e.adapter, e.outcome);
            Some((v, title))
        }
        "source" => {
            let sources = research::list_sources(store, ws)?;
            let Some(s) = sources.into_iter().find(|s| s.id == id) else {
                return Ok(None);
            };
            let mut stmt = conn.prepare("SELECT id,version_label,locator,access_state,acquired_via,accessed_at,content_hash,text_reference,created_at FROM source_versions WHERE source_id=?1 ORDER BY created_at").map_err(err)?;
            let versions = stmt
                .query_map([id], |r| {
                    Ok(json!({"id":r.get::<_,String>(0)?,"versionLabel":r.get::<_,Option<String>>(1)?,"locator":r.get::<_,Option<String>>(2)?,"accessState":r.get::<_,String>(3)?,"acquiredVia":r.get::<_,String>(4)?,"accessedAt":r.get::<_,Option<String>>(5)?,"contentHash":r.get::<_,Option<String>>(6)?,"textReference":r.get::<_,Option<String>>(7)?,"createdAt":r.get::<_,String>(8)?}))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let mut encoded = Vec::new();
            for mut v in versions {
                if let Some(reference) = v["textReference"].as_str().map(str::to_string) {
                    let path = PathBuf::from(&reference);
                    if path.is_file() {
                        let (hash, size) = hash_file(&path)?;
                        let name = format!("blobs/{hash}.txt");
                        pkg.blobs
                            .insert(name.clone(), (path, hash, size, "source_text".into()));
                        v["textReference"] = Value::String(name);
                    } else {
                        v["textReference"] = Value::Null;
                    }
                }
                encoded.push(v);
            }
            let title = s.title.clone();
            let mut v = json!({"id":s.id,"title":s.title,"citationKey":s.citation_key,"identifiers":s.identifiers,"createdAt":s.created_at,"updatedAt":s.updated_at});
            v["versions"] = Value::Array(encoded);
            Some((v, title))
        }
        "claim" => {
            let ledger = research::research_ledger(store, ws)?;
            let Some(c) = ledger.claims.into_iter().find(|c| c.id == id) else {
                return Ok(None);
            };
            let mut stmt = conn.prepare("SELECT id,version,claim_text,kind,origin,paper_locator_json,dependency_hash,created_at FROM claim_versions WHERE claim_id=?1 ORDER BY version").map_err(err)?;
            let versions = stmt
                .query_map([id], |r| {
                    let locator: Option<String> = r.get(5)?;
                    Ok(json!({"id":r.get::<_,String>(0)?,"version":r.get::<_,i64>(1)?,"claim":r.get::<_,String>(2)?,"kind":r.get::<_,String>(3)?,"origin":r.get::<_,String>(4)?,"paperLocator":locator.and_then(|l| serde_json::from_str::<Value>(&l).ok()),"dependencyHash":r.get::<_,Option<String>>(6)?,"createdAt":r.get::<_,String>(7)?}))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let title = c.claim.chars().take(80).collect::<String>();
            let mut v = serde_json::to_value(&c).map_err(err)?;
            v["versions"] = Value::Array(versions);
            Some((v, title))
        }
        "evidence" => {
            let ledger = research::research_ledger(store, ws)?;
            let Some(e) = ledger.evidence.into_iter().find(|e| e.id == id) else {
                return Ok(None);
            };
            let mut stmt = conn.prepare("SELECT id,method,checker_identity,input_hashes_json,observed_result_json,limitations,passed,created_at FROM verification_records WHERE evidence_link_id=?1 ORDER BY created_at").map_err(err)?;
            let verifications = stmt
                .query_map([id], |r| {
                    let hashes: String = r.get(3)?;
                    let observed: String = r.get(4)?;
                    Ok(json!({"id":r.get::<_,String>(0)?,"method":r.get::<_,String>(1)?,"checkerIdentity":r.get::<_,String>(2)?,"inputHashes":serde_json::from_str::<Value>(&hashes).unwrap_or(Value::Null),"observedResult":serde_json::from_str::<Value>(&observed).unwrap_or(Value::Null),"limitations":r.get::<_,String>(5)?,"passed":r.get::<_,bool>(6)?,"createdAt":r.get::<_,String>(7)?}))
                })
                .map_err(err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?;
            let title = format!("{} {} ({})", e.relation, e.target_type, e.assessment);
            let mut v = serde_json::to_value(&e).map_err(err)?;
            if e.target_type == "artifact" {
                include_artifact(ix, &e.target_id, "evidence_target", pkg);
                if let Some((_, hash, _, _)) = ix.artifacts.get(&e.target_id) {
                    v["targetSha256"] = Value::String(hash.clone());
                }
            }
            v["verifications"] = Value::Array(verifications);
            Some((v, title))
        }
        record_kind if RECORD_KINDS.contains(&record_kind) => {
            let r = match project::record(store, ws, id, record_kind) {
                Ok(r) => r,
                Err(_) => return Ok(None),
            };
            let title = record_title(record_kind, &r.body);
            Some((serde_json::to_value(&r).map_err(err)?, title))
        }
        _ => None,
    })
}
fn include_artifact(ix: &Index, artifact_id: &str, role: &str, pkg: &mut Package) {
    let Some((reference, hash, size, origin)) = ix.artifacts.get(artifact_id) else {
        pkg.external.push(ExternalReference {
            kind: "artifact".into(),
            id: artifact_id.into(),
            reason: "Referenced artifact is no longer in this Workspace".into(),
        });
        return;
    };
    if !ix.share_data && ["dataset", "execution_plan"].contains(&origin.as_str()) {
        pkg.external.push(ExternalReference {
            kind: "artifact".into(),
            id: artifact_id.into(),
            reason: "Project data-sharing policy excludes dataset and captured-input bytes".into(),
        });
        return;
    }
    let path = PathBuf::from(reference);
    if !path.is_file() {
        pkg.external.push(ExternalReference {
            kind: "artifact".into(),
            id: artifact_id.into(),
            reason: "Artifact bytes are unavailable in private storage".into(),
        });
        return;
    }
    if *size > MAX_BLOB_BYTES {
        pkg.external.push(ExternalReference {
            kind: "artifact".into(),
            id: artifact_id.into(),
            reason: format!("Artifact of {size} bytes exceeds the 64 MiB per-blob package limit"),
        });
        pkg.limitations.insert(
            "At least one large artifact was left out; the package does not claim full reproducibility.".into(),
        );
        return;
    }
    let name = format!(
        "blobs/{}",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| hash.clone())
    );
    pkg.blobs
        .entry(name)
        .or_insert((path, hash.clone(), *size, role.into()));
}
fn record_title(kind: &str, body: &Value) -> String {
    let pick = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| body.get(*k).and_then(Value::as_str))
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>()
    };
    match kind {
        "task" => pick(&["objective"]),
        "anchor" => body["selection"]["quote"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect(),
        "response" => format!(
            "Comment {} · {}",
            body["decision"]["number"].as_str().unwrap_or("?"),
            body["decision"]["disposition"].as_str().unwrap_or("open")
        ),
        "experiment" => pick(&["question"]),
        "theory" | "build" => pick(&["title", "name"]),
        "check" => body["check"]["method"].as_str().unwrap_or("").to_string(),
        "direction" => pick(&["question"]),
        "literature" => body["note"]["statement"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect(),
        "binding" => pick(&["printed"]),
        _ => kind.to_string(),
    }
}

fn collect(store: &Store, ws: &str, selection: &ExchangeSelection) -> WorkbenchResult<Package> {
    for kind in &selection.record_kinds {
        if !RECORD_KINDS.contains(&kind.as_str()) {
            return Err(WorkbenchError::invalid(format!(
                "Unknown exchange record kind {kind}"
            )));
        }
    }
    let ix = index(store, ws)?;
    let mut pkg = Package {
        objects: BTreeMap::new(),
        titles: BTreeMap::new(),
        blobs: BTreeMap::new(),
        external: Vec::new(),
        limitations: BTreeSet::new(),
    };
    let mut queue: VecDeque<(String, String)> = VecDeque::new();
    for (id, kind) in &ix.records {
        if selection.record_kinds.contains(kind) {
            queue.push_back((kind.clone(), id.clone()));
        }
    }
    if selection.include_notes {
        for id in &ix.notes {
            queue.push_back(("note".into(), id.clone()));
        }
    }
    for id in &selection.paper_ids {
        if !ix.papers.contains(id) {
            return Err(WorkbenchError::invalid(
                "Selected paper is not in this Workspace",
            ));
        }
        queue.push_back(("paper".into(), id.clone()));
    }
    for id in &selection.execution_ids {
        if !ix.executions.contains(id) {
            return Err(WorkbenchError::invalid(
                "Selected execution is not in this Workspace",
            ));
        }
        queue.push_back(("execution".into(), id.clone()));
    }
    if selection.include_sources {
        for id in &ix.sources {
            queue.push_back(("source".into(), id.clone()));
        }
    }
    if selection.include_ledger {
        for id in &ix.claims {
            queue.push_back(("claim".into(), id.clone()));
        }
        for id in &ix.evidence {
            queue.push_back(("evidence".into(), id.clone()));
        }
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    while let Some((kind, id)) = queue.pop_front() {
        if !seen.insert((kind.clone(), id.clone())) {
            continue;
        }
        if pkg.objects.len() >= MAX_OBJECTS {
            return Err(WorkbenchError::invalid(
                "The selection and its dependencies exceed 5,000 objects; narrow the selection",
            ));
        }
        let Some((value, title)) = load_object(store, ws, &ix, &kind, &id, &mut pkg)? else {
            pkg.external.push(ExternalReference {
                kind: kind.clone(),
                id: id.clone(),
                reason: "Referenced object is no longer available in this Workspace".into(),
            });
            continue;
        };
        let mut strings = Vec::new();
        let scan = if kind == "execution" {
            let mut v = value.clone();
            if let Some(o) = v.as_object_mut() {
                o.remove("inputManifest");
            }
            v
        } else {
            value.clone()
        };
        walk_strings(&scan, &mut strings, 0);
        for s in strings {
            if s == id {
                continue;
            }
            if let Some(k) = ix.records.get(s) {
                queue.push_back((k.clone(), s.to_string()));
            } else if ix.notes.contains(s) {
                queue.push_back(("note".into(), s.to_string()));
            } else if ix.executions.contains(s) {
                queue.push_back(("execution".into(), s.to_string()));
            } else if let Some(paper) = ix.revisions.get(s) {
                queue.push_back(("paper".into(), paper.clone()));
            } else if ix.papers.contains(s) {
                queue.push_back(("paper".into(), s.to_string()));
            } else if let Some(source) = ix.source_versions.get(s) {
                queue.push_back(("source".into(), source.clone()));
            } else if ix.sources.contains(s) {
                queue.push_back(("source".into(), s.to_string()));
            } else if ix.claims.contains(s) {
                queue.push_back(("claim".into(), s.to_string()));
            } else if let Some(claim) = ix.claim_versions.get(s) {
                queue.push_back(("claim".into(), claim.clone()));
            } else if ix.evidence.contains(s) {
                queue.push_back(("evidence".into(), s.to_string()));
            } else if ix.artifacts.contains_key(s) && kind != "execution" {
                include_artifact(&ix, s, "referenced_artifact", &mut pkg);
            }
        }
        pkg.titles.insert((kind.clone(), id.clone()), title);
        pkg.objects.insert((kind, id), value);
    }
    if selection.include_compiled_pdfs {
        let keys: Vec<_> = pkg
            .objects
            .iter()
            .filter(|((k, _), _)| k == "paper")
            .map(|(_, v)| v.clone())
            .collect();
        for paper in keys {
            for r in paper["revisions"].as_array().into_iter().flatten() {
                if let Some(aid) = r["compiledArtifactId"].as_str() {
                    include_artifact(&ix, aid, "compiled_pdf", &mut pkg);
                }
            }
        }
    }
    pkg.limitations.insert("Execution receipts are historical evidence from the exporting machine; their profiles, commands, working directories and declared inputs do not run or transfer.".into());
    if pkg
        .objects
        .keys()
        .any(|(k, _)| k == "checkpoint" || k == "application")
    {
        pkg.limitations.insert("Task copies and acceptance journals reference the exporting machine's folders; they are inert history after import.".into());
    }
    Ok(pkg)
}

fn readme(manifest: &ExchangeManifest, pkg: &Package) -> String {
    let mut md = format!(
        "# Research exchange: {}\n\nExported {} from Workspace `{}` (namespace `{}`). Format {} v{}.\n\n",
        manifest.workspace_name,
        manifest.created_at,
        manifest.workspace_id,
        &manifest.source_namespace[..12.min(manifest.source_namespace.len())],
        manifest.format,
        manifest.format_version
    );
    md.push_str("This package is readable without Pipeline: every object is a JSON file under `objects/<kind>/`, immutable text and artifacts are under `blobs/`, and this file lists them. Statuses and dispositions are the exporting researcher's; model assessments are labeled as such and are not verification.\n\n");
    let mut by_kind: BTreeMap<&str, Vec<&ObjectEntry>> = BTreeMap::new();
    for o in &manifest.objects {
        by_kind.entry(o.kind.as_str()).or_default().push(o);
    }
    md.push_str("## Contents\n\n| Kind | Count |\n|---|---|\n");
    for (kind, items) in &by_kind {
        md.push_str(&format!("| {kind} | {} |\n", items.len()));
    }
    md.push_str(&format!(
        "| immutable blobs | {} |\n\n",
        manifest.blobs.len()
    ));
    for (kind, items) in &by_kind {
        md.push_str(&format!("## {}\n\n", kind.replace('_', " ")));
        if *kind == "response" {
            md.push_str("| Comment | Disposition | Category | Intended response | Links |\n|---|---|---|---|---|\n");
            for o in items {
                let v = &pkg.objects[&(o.kind.clone(), o.id.clone())];
                let d = &v["body"]["decision"];
                let links = [
                    "taskId",
                    "manuscriptRevisionId",
                    "applicationId",
                    "executionId",
                ]
                .iter()
                .filter_map(|k| d[*k].as_str().map(|s| format!("{k}={s}")))
                .collect::<Vec<_>>()
                .join(", ");
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    d["number"].as_str().unwrap_or("?"),
                    d["disposition"].as_str().unwrap_or(""),
                    d["category"].as_str().unwrap_or(""),
                    d["intendedResponse"]
                        .as_str()
                        .unwrap_or("")
                        .replace('|', "\\|")
                        .replace('\n', " "),
                    links
                ));
            }
            md.push('\n');
            continue;
        }
        for o in items {
            let v = &pkg.objects[&(o.kind.clone(), o.id.clone())];
            let status = ["status", "state", "outcome", "disposition"]
                .iter()
                .find_map(|k| {
                    v.get(*k)
                        .or_else(|| v["body"].get(*k))
                        .and_then(Value::as_str)
                })
                .unwrap_or("");
            md.push_str(&format!(
                "- `{}` {}{}\n",
                o.id,
                o.title.replace('\n', " "),
                if status.is_empty() {
                    String::new()
                } else {
                    format!(" — {status}")
                }
            ));
        }
        md.push('\n');
    }
    md.push_str("## Excluded by default\n\n");
    for e in &manifest.exclusions {
        md.push_str(&format!("- {e}\n"));
    }
    if !manifest.external_references.is_empty() {
        md.push_str("\n## External references not packaged\n\n");
        for e in manifest.external_references.iter().take(500) {
            md.push_str(&format!("- {} `{}`: {}\n", e.kind, e.id, e.reason));
        }
        if manifest.external_references.len() > 500 {
            md.push_str("- … further references are listed in manifest.json\n");
        }
    }
    md.push_str("\n## Limitations\n\n");
    for l in &manifest.limitations {
        md.push_str(&format!("- {l}\n"));
    }
    md.push_str(&format!("\n{}\n", manifest.reproducibility));
    md
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportExchangeRequest {
    pub workspace_id: String,
    pub path: String,
    pub selection: ExchangeSelection,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangePreview {
    pub objects: Vec<ObjectEntry>,
    pub blob_count: usize,
    pub blob_bytes: u64,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeReport {
    pub path: String,
    pub bytes: u64,
    pub package_hash: String,
    pub object_count: usize,
    pub blob_count: usize,
    pub exclusions: Vec<String>,
    pub external_references: Vec<ExternalReference>,
    pub limitations: Vec<String>,
}
fn exclusions() -> Vec<String> {
    vec![
        "Conversations, transcripts, drafts and native thread bindings".into(),
        "Managed sign-in, Codex home and any credential".into(),
        "Execution profiles, host authorizations and other machine-specific executable settings"
            .into(),
        "Harness presets, Workspace settings and recipe runs".into(),
        "Declared execution inputs and unselected or raw datasets (hashes remain in receipts)"
            .into(),
        "Performance samples, evaluations and Workflow runs".into(),
    ]
}
fn manifest_for(
    store: &Store,
    ws: &str,
    selection: &ExchangeSelection,
    pkg: &Package,
) -> WorkbenchResult<ExchangeManifest> {
    let workspace = store.workspace(ws)?;
    let objects = pkg
        .objects
        .iter()
        .map(|((kind, id), value)| ObjectEntry {
            kind: kind.clone(),
            id: id.clone(),
            fingerprint: fingerprint(value),
            title: pkg
                .titles
                .get(&(kind.clone(), id.clone()))
                .cloned()
                .unwrap_or_default(),
        })
        .collect();
    let blobs = pkg
        .blobs
        .iter()
        .map(|(name, (_, hash, size, role))| BlobEntry {
            archive_path: name.clone(),
            sha256: hash.clone(),
            size_bytes: *size,
            role: role.clone(),
        })
        .collect();
    let mut limitations: Vec<String> = pkg.limitations.iter().cloned().collect();
    if pkg.external.iter().any(|e| e.kind == "execution_input") {
        limitations.push("Declared execution inputs (possibly large or restricted datasets) are not included; results are not reproducible from this package alone.".into());
    }
    Ok(ExchangeManifest {
        format: EXCHANGE_FORMAT.into(),
        format_version: EXCHANGE_FORMAT_VERSION,
        store_schema_version: CURRENT_SCHEMA_VERSION,
        created_at: now(),
        source_namespace: format!("{}:{}", instance_id(store)?, ws),
        workspace_id: ws.into(),
        workspace_name: workspace.name,
        workspace_root: workspace.root,
        selection: selection.clone(),
        objects,
        blobs,
        exclusions: exclusions(),
        external_references: pkg.external.clone(),
        limitations,
        reproducibility: "Reproducibility statement: receipts, hashes and outputs are evidence of what ran on the exporting machine. Nothing in this package reruns automatically, and missing inputs or excluded artifacts are listed above rather than assumed present.".into(),
    })
}
pub fn preview_exchange(
    store: &Store,
    ws: &str,
    selection: &ExchangeSelection,
) -> WorkbenchResult<ExchangePreview> {
    store.workspace(ws)?;
    let pkg = collect(store, ws, selection)?;
    let manifest = manifest_for(store, ws, selection, &pkg)?;
    Ok(ExchangePreview {
        objects: manifest.objects,
        blob_count: manifest.blobs.len(),
        blob_bytes: manifest.blobs.iter().map(|b| b.size_bytes).sum(),
        exclusions: manifest.exclusions,
        external_references: manifest.external_references,
        limitations: manifest.limitations,
    })
}
pub fn export_exchange(
    store: &Store,
    request: ExportExchangeRequest,
) -> WorkbenchResult<ExchangeReport> {
    let target = validate_path(&request.path, "pwex")?;
    if target.starts_with(store.root_path()) {
        return Err(WorkbenchError::invalid(
            "Choose a package destination outside Workspace private storage",
        ));
    }
    let ws = &request.workspace_id;
    store.workspace(ws)?;
    let pkg = collect(store, ws, &request.selection)?;
    if pkg.objects.is_empty() {
        return Err(WorkbenchError::invalid("The selection contains no objects"));
    }
    let manifest = manifest_for(store, ws, &request.selection, &pkg)?;
    let parent = target
        .parent()
        .ok_or_else(|| WorkbenchError::invalid("Package destination has no parent directory"))?;
    fs::create_dir_all(parent).map_err(err)?;
    let temporary = tempfile::Builder::new()
        .prefix(".project-exchange-")
        .suffix(".pwex.partial")
        .tempfile_in(parent)
        .map_err(err)?;
    let file = temporary.reopen().map_err(err)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(err)?;
    zip.start_file("manifest.json", options).map_err(err)?;
    zip.write_all(manifest_json.as_bytes()).map_err(err)?;
    zip.start_file("README.md", options).map_err(err)?;
    zip.write_all(readme(&manifest, &pkg).as_bytes())
        .map_err(err)?;
    let mut expanded = manifest_json.len() as u64;
    for ((kind, id), value) in &pkg.objects {
        if !safe_archive_name(id) {
            return Err(WorkbenchError::invalid("Object ID is not a safe file name"));
        }
        let encoded = serde_json::to_string_pretty(value).map_err(err)?;
        expanded = expanded.saturating_add(encoded.len() as u64);
        zip.start_file(format!("objects/{kind}/{id}.json"), options)
            .map_err(err)?;
        zip.write_all(encoded.as_bytes()).map_err(err)?;
    }
    for (name, (path, hash, size, _)) in &pkg.blobs {
        expanded = expanded.saturating_add(*size);
        if expanded > MAX_PACKAGE_BYTES {
            return Err(WorkbenchError::invalid("Expanded package exceeds 2 GiB"));
        }
        let (actual, _) = hash_file(path)?;
        if &actual != hash {
            return Err(WorkbenchError::invalid(
                "An immutable blob changed during export; retry",
            ));
        }
        zip.start_file(name, options).map_err(err)?;
        std::io::copy(&mut File::open(path).map_err(err)?, &mut zip).map_err(err)?;
    }
    let file = zip.finish().map_err(err)?;
    file.sync_all().map_err(err)?;
    drop(file);
    temporary
        .persist(&target)
        .map_err(|e| WorkbenchError::storage("Failed to publish package", e.error))?;
    let (package_hash, bytes) = hash_file(&target)?;
    Ok(ExchangeReport {
        path: target.to_string_lossy().into_owned(),
        bytes,
        package_hash,
        object_count: manifest.objects.len(),
        blob_count: manifest.blobs.len(),
        exclusions: manifest.exclusions,
        external_references: manifest.external_references,
        limitations: manifest.limitations,
    })
}

struct Extracted {
    _temp: tempfile::TempDir,
    root: PathBuf,
    manifest: ExchangeManifest,
    package_hash: String,
}
fn extract(path: &str) -> WorkbenchResult<Extracted> {
    let source = validate_path(path, "pwex")?;
    let (package_hash, size) = hash_file(&source)?;
    if size > MAX_PACKAGE_BYTES {
        return Err(WorkbenchError::invalid("Package exceeds 2 GiB"));
    }
    let temp = tempfile::tempdir().map_err(err)?;
    let mut archive = zip::ZipArchive::new(File::open(&source).map_err(err)?)
        .map_err(|e| WorkbenchError::invalid(format!("Invalid exchange package: {e}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(WorkbenchError::invalid("Package contains too many entries"));
    }
    let mut total = 0u64;
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| WorkbenchError::invalid(format!("Invalid package entry: {e}")))?;
        let name = entry.name().to_string();
        if !safe_archive_name(&name)
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(WorkbenchError::invalid(
                "Package contains a traversal, directory, or symbolic-link entry",
            ));
        }
        if !names.insert(name.clone()) {
            return Err(WorkbenchError::invalid(
                "Package contains duplicate entries",
            ));
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(WorkbenchError::invalid("Package entry exceeds 256 MiB"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_PACKAGE_BYTES {
            return Err(WorkbenchError::invalid("Expanded package exceeds 2 GiB"));
        }
        let destination = temp.path().join(&name);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(err)?;
        }
        let mut output = File::create(&destination).map_err(err)?;
        std::io::copy(&mut entry, &mut output).map_err(err)?;
    }
    let manifest: ExchangeManifest = serde_json::from_reader(
        File::open(temp.path().join("manifest.json"))
            .map_err(|_| WorkbenchError::invalid("Package manifest is missing"))?,
    )
    .map_err(|e| WorkbenchError::invalid(format!("Invalid package manifest: {e}")))?;
    if manifest.format != EXCHANGE_FORMAT || manifest.format_version != EXCHANGE_FORMAT_VERSION {
        return Err(WorkbenchError::invalid(
            "This is not a supported project exchange package",
        ));
    }
    if manifest.store_schema_version > CURRENT_SCHEMA_VERSION {
        return Err(WorkbenchError::invalid(
            "Package was written by a newer Pipeline; update before importing",
        ));
    }
    if manifest.objects.len() > MAX_OBJECTS {
        return Err(WorkbenchError::invalid("Package lists too many objects"));
    }
    let mut seen = HashSet::new();
    for o in &manifest.objects {
        if !safe_archive_name(&o.id) || !seen.insert((o.kind.clone(), o.id.clone())) {
            return Err(WorkbenchError::invalid(
                "Package manifest lists unsafe or duplicate objects",
            ));
        }
        let path = temp
            .path()
            .join(format!("objects/{}/{}.json", o.kind, o.id));
        if !path.is_file() {
            return Err(WorkbenchError::invalid(format!(
                "Package is missing object {}/{}",
                o.kind, o.id
            )));
        }
        if !names.contains(&format!("objects/{}/{}.json", o.kind, o.id)) {
            return Err(WorkbenchError::invalid(
                "Package object list disagrees with entries",
            ));
        }
    }
    let object_entries = names.iter().filter(|n| n.starts_with("objects/")).count();
    if object_entries != manifest.objects.len() {
        return Err(WorkbenchError::invalid(
            "Package contains objects absent from its manifest",
        ));
    }
    for b in &manifest.blobs {
        if !safe_archive_name(&b.archive_path) || !b.archive_path.starts_with("blobs/") {
            return Err(WorkbenchError::invalid("Package blob path is unsafe"));
        }
        let path = temp.path().join(&b.archive_path);
        if !path.is_file() {
            return Err(WorkbenchError::invalid(format!(
                "Package is missing blob {}",
                b.archive_path
            )));
        }
        let (hash, size) = hash_file(&path)?;
        if hash != b.sha256 || size != b.size_bytes {
            return Err(WorkbenchError::invalid(format!(
                "Blob {} failed its hash check",
                b.archive_path
            )));
        }
    }
    let blob_entries = names.iter().filter(|n| n.starts_with("blobs/")).count();
    if blob_entries != manifest.blobs.len() {
        return Err(WorkbenchError::invalid(
            "Package contains blobs absent from its manifest",
        ));
    }
    Ok(Extracted {
        root: temp.path().to_path_buf(),
        _temp: temp,
        manifest,
        package_hash,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInspection {
    pub path: String,
    pub package_hash: String,
    pub workspace_name: String,
    pub source_namespace: String,
    pub created_at: String,
    pub workspace_root: Option<String>,
    pub counts: BTreeMap<String, usize>,
    pub blob_count: usize,
    pub blob_bytes: u64,
    pub exclusions: Vec<String>,
    pub external_references: usize,
    pub limitations: Vec<String>,
    pub own_export: bool,
}
pub fn inspect_exchange(store: &Store, path: &str) -> WorkbenchResult<ExchangeInspection> {
    let x = extract(path)?;
    let mut counts = BTreeMap::new();
    for o in &x.manifest.objects {
        *counts.entry(o.kind.clone()).or_insert(0) += 1;
    }
    let own = x
        .manifest
        .source_namespace
        .starts_with(&format!("{}:", instance_id(store)?));
    Ok(ExchangeInspection {
        path: path.into(),
        package_hash: x.package_hash,
        workspace_name: x.manifest.workspace_name,
        source_namespace: x.manifest.source_namespace,
        created_at: x.manifest.created_at,
        workspace_root: x.manifest.workspace_root,
        counts,
        blob_count: x.manifest.blobs.len(),
        blob_bytes: x.manifest.blobs.iter().map(|b| b.size_bytes).sum(),
        exclusions: x.manifest.exclusions,
        external_references: x.manifest.external_references.len(),
        limitations: x.manifest.limitations,
        own_export: own,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExchangeTarget {
    NewWorkspace { name: String, root: Option<String> },
    ExistingWorkspace { workspace_id: String },
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportExchangeRequest {
    pub path: String,
    pub target: ExchangeTarget,
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDecision {
    pub kind: String,
    pub id: String,
    pub outcome: String,
    pub new_id: Option<String>,
    pub title: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub target_workspace_id: Option<String>,
    pub new: usize,
    pub identical: usize,
    pub remapped: usize,
    pub conflicts: Vec<ObjectDecision>,
    pub decisions: Vec<ObjectDecision>,
    pub blob_count: usize,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub import_id: String,
    pub workspace_id: String,
    pub new: usize,
    pub identical: usize,
    pub remapped: usize,
    pub conflicts: usize,
    pub blobs: usize,
    pub limitations: Vec<String>,
}

struct LocalObject {
    workspace_id: String,
    value: Value,
}
fn local_object(store: &Store, kind: &str, id: &str) -> WorkbenchResult<Option<LocalObject>> {
    let conn = store.connection()?;
    let ws: Option<String> = match kind {
        "note" => conn
            .query_row(
                "SELECT workspace_id FROM research_notes WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?,
        "paper" => conn
            .query_row("SELECT workspace_id FROM papers WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?,
        "execution" => conn
            .query_row(
                "SELECT workspace_id FROM research_executions WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?,
        "source" => conn
            .query_row("SELECT workspace_id FROM sources WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?,
        "claim" => conn
            .query_row("SELECT workspace_id FROM claims WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?,
        "evidence" => conn
            .query_row(
                "SELECT workspace_id FROM evidence_links WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?,
        _ => conn
            .query_row(
                "SELECT workspace_id FROM project_records WHERE id=?1 AND kind=?2",
                params![id, kind],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?,
    };
    drop(conn);
    let Some(ws) = ws else {
        return Ok(None);
    };
    let ix = index(store, &ws)?;
    let mut scratch = Package {
        objects: BTreeMap::new(),
        titles: BTreeMap::new(),
        blobs: BTreeMap::new(),
        external: Vec::new(),
        limitations: BTreeSet::new(),
    };
    Ok(
        load_object(store, &ws, &ix, kind, id, &mut scratch)?.map(|(value, _)| LocalObject {
            workspace_id: ws,
            value,
        }),
    )
}
fn derived_id(namespace: &str, id: &str) -> String {
    let prefix = id.split('_').next().unwrap_or("obj");
    format!(
        "{prefix}_x{}",
        &hex(format!("{namespace}\0{id}").as_bytes())[..24]
    )
}
/// Resolve artifact references that travel by content hash rather than ID so
/// identical content compares equal across stores.
fn resolve_hash_refs(store: &Store, ws: &str, value: &mut Value) -> WorkbenchResult<()> {
    if let Some(sha) = value
        .get("targetSha256")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        let local: Option<String> = store
            .connection()?
            .query_row(
                "SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 ORDER BY created_at LIMIT 1",
                params![ws, sha],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(local) = local {
            value["targetId"] = Value::String(local);
        }
    }
    Ok(())
}
fn plan(
    store: &Store,
    x: &Extracted,
    target_ws: Option<&str>,
) -> WorkbenchResult<(Vec<ObjectDecision>, HashMap<String, String>)> {
    let namespace = &x.manifest.source_namespace;
    let mut map: HashMap<String, String> = HashMap::new();
    let mut values: Vec<Value> = Vec::with_capacity(x.manifest.objects.len());
    // Pass 1: decide identities and build the complete ID map.
    let mut states: Vec<(String, Option<String>, bool)> = Vec::new();
    for o in &x.manifest.objects {
        let value: Value = serde_json::from_reader(
            File::open(x.root.join(format!("objects/{}/{}.json", o.kind, o.id))).map_err(err)?,
        )
        .map_err(|e| WorkbenchError::invalid(format!("Invalid object {}: {e}", o.id)))?;
        let in_target = |l: &Option<LocalObject>| {
            l.as_ref()
                .is_some_and(|l| Some(l.workspace_id.as_str()) == target_ws)
        };
        let local = local_object(store, &o.kind, &o.id)?;
        let (outcome, new_id, level_ns): (String, Option<String>, Option<String>) = if local
            .is_none()
        {
            ("new".into(), None, None)
        } else if in_target(&local) {
            ("existing".into(), None, None)
        } else {
            let c1 = derived_id(namespace, &o.id);
            let l1 = local_object(store, &o.kind, &c1)?;
            if l1.is_none() {
                ("remapped".into(), Some(c1), Some(namespace.clone()))
            } else if in_target(&l1) {
                ("existing".into(), Some(c1), Some(namespace.clone()))
            } else {
                match target_ws {
                    None => ("remapped".into(), None, None),
                    Some(t) => {
                        let ns2 = format!("{namespace}|{t}");
                        let c2 = derived_id(&ns2, &o.id);
                        let l2 = local_object(store, &o.kind, &c2)?;
                        if l2.is_none() {
                            ("remapped".into(), Some(c2), Some(ns2))
                        } else if in_target(&l2) {
                            ("existing".into(), Some(c2), Some(ns2))
                        } else {
                            return Err(WorkbenchError::invalid(format!(
                                "Derived ID for {} collides across Workspaces; import into a fresh Workspace",
                                o.id
                            )));
                        }
                    }
                }
            }
        };
        if let (Some(n), Some(ns)) = (&new_id, &level_ns) {
            map.insert(o.id.clone(), n.clone());
            for child in child_ids(&o.kind, &value) {
                map.insert(child.clone(), derived_id(ns, &child));
            }
        }
        states.push((outcome, new_id, in_target(&local) || level_ns.is_some()));
        values.push(value);
    }
    // Pass 2: compare content only after every reference is remapped.
    let mut decisions = Vec::with_capacity(values.len());
    for ((o, mut value), (outcome, new_id, _)) in x.manifest.objects.iter().zip(values).zip(states)
    {
        let outcome = if outcome == "existing" {
            rewrite_ids(&mut value, &map);
            if let Some(ws) = target_ws {
                resolve_hash_refs(store, ws, &mut value)?;
            }
            let id = new_id.clone().unwrap_or_else(|| o.id.clone());
            let local = local_object(store, &o.kind, &id)?
                .map(|l| l.value)
                .unwrap_or(Value::Null);
            if fingerprint(&local) == fingerprint(&value) {
                "identical".to_string()
            } else {
                "conflict".to_string()
            }
        } else {
            outcome
        };
        decisions.push(ObjectDecision {
            kind: o.kind.clone(),
            id: o.id.clone(),
            outcome,
            new_id,
            title: o.title.clone(),
        });
    }
    Ok((decisions, map))
}
fn child_ids(kind: &str, value: &Value) -> Vec<String> {
    let key = match kind {
        "paper" => "revisions",
        "source" => "versions",
        "claim" => "versions",
        "evidence" => "verifications",
        _ => return Vec::new(),
    };
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["id"].as_str().map(str::to_string))
        .collect()
}
fn rewrite_ids(value: &mut Value, map: &HashMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(n) = map.get(s) {
                *s = n.clone();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| rewrite_ids(v, map)),
        Value::Object(o) => o.values_mut().for_each(|v| rewrite_ids(v, map)),
        _ => {}
    }
}
pub fn preview_import(
    store: &Store,
    path: &str,
    target: &ExchangeTarget,
) -> WorkbenchResult<ImportPreview> {
    let x = extract(path)?;
    let target_ws = match target {
        ExchangeTarget::ExistingWorkspace { workspace_id } => {
            store.workspace(workspace_id)?;
            Some(workspace_id.clone())
        }
        ExchangeTarget::NewWorkspace { .. } => None,
    };
    let (decisions, _) = plan(store, &x, target_ws.as_deref())?;
    let count = |o: &str| decisions.iter().filter(|d| d.outcome == o).count();
    Ok(ImportPreview {
        target_workspace_id: target_ws,
        new: count("new"),
        identical: count("identical"),
        remapped: count("remapped"),
        conflicts: decisions
            .iter()
            .filter(|d| d.outcome == "conflict")
            .cloned()
            .collect(),
        decisions,
        blob_count: x.manifest.blobs.len(),
        limitations: x.manifest.limitations,
    })
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}
fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .map(|x| match x {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}
fn json_text(v: &Value, key: &str) -> Option<String> {
    match v.get(key) {
        None | Some(Value::Null) => None,
        Some(x) => Some(x.to_string()),
    }
}

pub fn import_exchange(
    store: &Store,
    request: ImportExchangeRequest,
) -> WorkbenchResult<ImportReport> {
    let x = extract(&request.path)?;
    let (workspace_id, created) = match &request.target {
        ExchangeTarget::ExistingWorkspace { workspace_id } => {
            let w = store.workspace(workspace_id)?;
            if w.archived_at.is_some() {
                return Err(WorkbenchError::invalid("Restore this Workspace first"));
            }
            (workspace_id.clone(), false)
        }
        ExchangeTarget::NewWorkspace { name, root } => {
            let w = store.create_workspace(CreateWorkspaceRequest {
                name: name.clone(),
                root: root.clone(),
                operation_id: request.operation_id.clone(),
            })?;
            (w.record.id, true)
        }
    };
    let (decisions, map) = plan(store, &x, Some(&workspace_id))?;
    let blob_dir = store.root_path().join("blobs");
    let mut blob_paths: HashMap<String, (PathBuf, String, u64)> = HashMap::new();
    for b in &x.manifest.blobs {
        let name = Path::new(&b.archive_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| WorkbenchError::invalid("Blob has no file name"))?;
        let destination = blob_dir.join(&name);
        if destination.exists() {
            let (hash, _) = hash_file(&destination)?;
            if hash != b.sha256 {
                return Err(WorkbenchError::invalid(format!(
                    "Existing private blob {name} differs from the package blob with the same name"
                )));
            }
        } else {
            let mut tmp = tempfile::NamedTempFile::new_in(&blob_dir).map_err(err)?;
            std::io::copy(
                &mut File::open(x.root.join(&b.archive_path)).map_err(err)?,
                &mut tmp,
            )
            .map_err(err)?;
            tmp.as_file().sync_all().map_err(err)?;
            match tmp.persist_noclobber(&destination) {
                Ok(_) => {}
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(err(e)),
            }
        }
        blob_paths.insert(
            b.archive_path.clone(),
            (destination, b.sha256.clone(), b.size_bytes),
        );
    }
    let mut conn = store.connection()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    let import_id = random_id("exchange")?;
    let timestamp = now();
    tx.execute("INSERT INTO exchange_imports(id,workspace_id,package_hash,source_namespace,manifest_json,decisions_json,summary_json,imported_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)", params![import_id, workspace_id, x.package_hash, x.manifest.source_namespace, serde_json::to_string(&x.manifest).map_err(err)?, serde_json::to_string(&decisions).map_err(err)?, json!({"outcome":"in_progress"}).to_string(), timestamp]).map_err(err)?;
    let order = [
        "paper",
        "source",
        "execution",
        "claim",
        "evidence",
        "note",
        "anchor",
        "task",
        "checkpoint",
        "application",
        "build",
        "response",
        "experiment",
        "specification",
        "series",
        "binding",
        "bibliography",
        "literature",
        "theory",
        "check",
        "direction",
    ];
    let mut conflicts = 0usize;
    let mut imported_blob_names: BTreeMap<String, (String, u64, String)> = BTreeMap::new();
    let mut register_blob = |archive_path: &str, role: &str| -> Option<String> {
        blob_paths.get(archive_path).map(|(p, hash, size)| {
            imported_blob_names.insert(
                p.to_string_lossy().into_owned(),
                (hash.clone(), *size, role.to_string()),
            );
            p.to_string_lossy().into_owned()
        })
    };
    for kind in order {
        for d in decisions.iter().filter(|d| d.kind == kind) {
            let mut value: Value = serde_json::from_reader(
                File::open(x.root.join(format!("objects/{}/{}.json", d.kind, d.id)))
                    .map_err(err)?,
            )
            .map_err(err)?;
            rewrite_ids(&mut value, &map);
            let id = d.new_id.clone().unwrap_or_else(|| d.id.clone());
            match d.outcome.as_str() {
                "identical" => {
                    tx.execute("INSERT INTO exchange_bases(workspace_id,source_namespace,kind,source_id,local_id,value_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(workspace_id,source_namespace,kind,source_id) DO UPDATE SET value_json=excluded.value_json,local_id=excluded.local_id",params![workspace_id,x.manifest.source_namespace,d.kind,d.id,id,value.to_string()]).map_err(err)?;
                    continue;
                }
                "conflict" => {
                    let local = local_object(store, &d.kind, &id)?
                        .map(|l| l.value)
                        .unwrap_or(Value::Null);
                    let base:Option<String>=tx.query_row("SELECT value_json FROM exchange_bases WHERE workspace_id=?1 AND source_namespace=?2 AND kind=?3 AND source_id=?4 AND local_id=?5",params![workspace_id,x.manifest.source_namespace,d.kind,d.id,id],|r|r.get(0)).optional().map_err(err)?;
                    tx.execute(
                        "INSERT INTO exchange_conflicts(id,import_id,workspace_id,object_kind,object_id,local_json,imported_json,state,recorded_at,base_json) VALUES(?1,?2,?3,?4,?5,?6,?7,'open',?8,?9)",
                        params![random_id("conflict")?, import_id, workspace_id, d.kind, id, local.to_string(), value.to_string(), timestamp,base],
                    )
                    .map_err(err)?;
                    conflicts += 1;
                    continue;
                }
                _ => {}
            }
            match kind {
                "paper" => {
                    tx.execute("INSERT INTO papers(id,workspace_id,title,role,current_revision_id,created_at,updated_at) VALUES(?1,?2,?3,?4,NULL,?5,?6)", params![id, workspace_id, text(&value,"title"), str_of(&value,"role").unwrap_or_else(|| "other".into()), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    let mut current: Option<String> = None;
                    for r in value["revisions"].as_array().into_iter().flatten() {
                        let rid = text(r, "id");
                        let text_reference = str_of(r, "textReference")
                            .and_then(|p| register_blob(&p, "paper_text"));
                        let compiled_id = match str_of(r, "compiledSha256").and_then(|sha| {
                            x.manifest.blobs.iter().find(|b| b.sha256 == sha).cloned()
                        }) {
                            Some(b) => match register_blob(&b.archive_path, "compiled_pdf") {
                                Some(path) => {
                                    tx.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,original_path,created_at) VALUES(?1,?2,?3,'pdf',?4,'exchange_import',?5,NULL,?6)", params![random_id("artifact")?, workspace_id, b.sha256, b.size_bytes as i64, path, timestamp]).map_err(err)?;
                                    tx.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind='pdf'", params![workspace_id, b.sha256], |row| row.get::<_, String>(0)).optional().map_err(err)?
                                }
                                None => None,
                            },
                            None => None,
                        };
                        tx.execute("INSERT INTO paper_revisions(id,paper_id,input_kind,entrypoint,dependency_manifest_json,content_hash,text_reference,compiled_artifact_id,extraction_json,capture_complete,captured_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![rid, id, str_of(r,"inputKind").unwrap_or_else(|| "text".into()), text(r,"entrypoint"), json_text(r,"dependencyManifest").unwrap_or_else(|| "{}".into()), text(r,"contentHash"), text_reference, compiled_id, json_text(r,"extraction").unwrap_or_else(|| "{}".into()), r["captureComplete"].as_bool().unwrap_or(false), text(r,"capturedAt")]).map_err(err)?;
                        current = Some(rid);
                    }
                    if let Some(current) = value["currentRevisionId"]
                        .as_str()
                        .map(str::to_string)
                        .or(current)
                    {
                        tx.execute(
                            "UPDATE papers SET current_revision_id=?2 WHERE id=?1",
                            params![id, current],
                        )
                        .map_err(err)?;
                    }
                }
                "source" => {
                    tx.execute("INSERT INTO sources(id,workspace_id,title,citation_key,identifiers_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id, workspace_id, text(&value,"title"), str_of(&value,"citationKey"), json_text(&value,"identifiers").unwrap_or_else(|| "{}".into()), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    for v in value["versions"].as_array().into_iter().flatten() {
                        let text_reference = str_of(v, "textReference")
                            .and_then(|p| register_blob(&p, "source_text"));
                        tx.execute("INSERT INTO source_versions(id,source_id,version_label,locator,access_state,acquired_via,accessed_at,content_hash,text_reference,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![text(v,"id"), id, str_of(v,"versionLabel"), str_of(v,"locator"), str_of(v,"accessState").unwrap_or_else(|| "metadata".into()), text(v,"acquiredVia"), str_of(v,"accessedAt"), str_of(v,"contentHash"), text_reference, text(v,"createdAt")]).map_err(err)?;
                    }
                }
                "execution" => {
                    let mut output_manifest = value["outputManifest"].clone();
                    if let Some(artifacts) = output_manifest["artifacts"].as_array_mut() {
                        for a in artifacts.iter_mut() {
                            let hash = text(a, "contentHash");
                            let Some(b) = x.manifest.blobs.iter().find(|b| b.sha256 == hash) else {
                                a["artifactId"] = Value::Null;
                                a["unavailable"] = Value::String("artifact not packaged".into());
                                continue;
                            };
                            let Some(path) = register_blob(&b.archive_path, "execution_output")
                            else {
                                continue;
                            };
                            let suffix = Path::new(&b.archive_path)
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("bin")
                                .to_string();
                            let aid = random_id("artifact")?;
                            tx.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,original_path,created_at) VALUES(?1,?2,?3,?4,?5,'exchange_import',?6,NULL,?7)", params![aid, workspace_id, hash, suffix, b.size_bytes as i64, path, timestamp]).map_err(err)?;
                            let resolved: String = tx.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 AND media_kind=?3", params![workspace_id, hash, suffix], |row| row.get(0)).map_err(err)?;
                            a["artifactId"] = Value::String(resolved);
                            a["sourceExecutionId"] = Value::String(id.clone());
                        }
                    }
                    tx.execute("INSERT INTO research_executions(id,workspace_id,session_id,binding_id,provider_turn_id,tool_call_id,profile_id,adapter,command_json,cwd,environment_identity,input_manifest_json,dependency_hash,outcome,started_at,ended_at,exit_status,stdout_text,stderr_text,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,?2,NULL,NULL,NULL,NULL,NULL,?3,?4,?5,NULL,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)", params![id, workspace_id, text(&value,"adapter"), json_text(&value,"command").unwrap_or_else(|| "[]".into()), text(&value,"cwd"), json_text(&value,"inputManifest").unwrap_or_else(|| "{}".into()), text(&value,"dependencyHash"), str_of(&value,"outcome").unwrap_or_else(|| "outcome_unknown".into()), str_of(&value,"startedAt"), str_of(&value,"endedAt"), value["exitStatus"].as_i64(), str_of(&value,"stdout"), str_of(&value,"stderr"), output_manifest.to_string(), json_text(&value,"validation").unwrap_or_else(|| "{}".into()), str_of(&value,"snapshotConsistency").unwrap_or_else(|| "uncertain".into()), text(&value,"createdAt")]).map_err(err)?;
                    for r in value["structuredResults"].as_array().into_iter().flatten() {
                        let mut body = r["body"].clone();
                        body["sourceExecutionId"] = Value::String(id.clone());
                        tx.execute("INSERT OR IGNORE INTO structured_results(id,execution_id,result_id,body_json,created_at) VALUES(?1,?2,?3,?4,?5)", params![random_id("result")?, id, text(r,"resultId"), body.to_string(), timestamp]).map_err(err)?;
                    }
                }
                "claim" => {
                    tx.execute("INSERT INTO claims(id,workspace_id,paper_id,current_version_id,workflow_state,created_at,updated_at) VALUES(?1,?2,?3,NULL,?4,?5,?6)", params![id, workspace_id, str_of(&value,"paperId").filter(|p| paper_exists(&tx, p)), str_of(&value,"workflowState").unwrap_or_else(|| "proposed".into()), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    for v in value["versions"].as_array().into_iter().flatten() {
                        tx.execute("INSERT INTO claim_versions(id,claim_id,version,claim_text,kind,origin,paper_locator_json,dependency_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![text(v,"id"), id, v["version"].as_i64().unwrap_or(1), text(v,"claim"), text(v,"kind"), text(v,"origin"), json_text(v,"paperLocator"), str_of(v,"dependencyHash"), text(v,"createdAt")]).map_err(err)?;
                    }
                    if let Some(current) = str_of(&value, "versionId") {
                        tx.execute(
                            "UPDATE claims SET current_version_id=?2 WHERE id=?1",
                            params![id, current],
                        )
                        .map_err(err)?;
                    }
                }
                "evidence" => {
                    let claim_version = text(&value, "claimVersionId");
                    let exists: bool = tx
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM claim_versions WHERE id=?1)",
                            [&claim_version],
                            |r| r.get(0),
                        )
                        .map_err(err)?;
                    if !exists {
                        continue;
                    }
                    let target_type = text(&value, "targetType");
                    let mut target_id = text(&value, "targetId");
                    if let Some(sha) = str_of(&value, "targetSha256") {
                        let local: Option<String> = tx.query_row("SELECT id FROM artifacts WHERE workspace_id=?1 AND content_hash=?2 ORDER BY created_at LIMIT 1", params![workspace_id, sha], |r| r.get(0)).optional().map_err(err)?;
                        if let Some(local) = local {
                            target_id = local;
                        }
                    }
                    tx.execute("INSERT INTO evidence_links(id,workspace_id,claim_version_id,target_type,target_id,locator_json,relation,assessment,assessor,dependency_hash,freshness,stale_reason,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'unknown',?11,?12,?13)", params![id, workspace_id, claim_version, target_type, target_id, json_text(&value,"locator"), text(&value,"relation"), text(&value,"assessment"), text(&value,"assessor"), str_of(&value,"dependencyHash"), Some("imported from an exchange package; dependency freshness is not tracked across stores"), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    for v in value["verifications"].as_array().into_iter().flatten() {
                        tx.execute("INSERT INTO verification_records(id,evidence_link_id,method,checker_identity,input_hashes_json,observed_result_json,limitations,passed,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![text(v,"id"), id, text(v,"method"), text(v,"checkerIdentity"), json_text(v,"inputHashes").unwrap_or_else(|| "[]".into()), json_text(v,"observedResult").unwrap_or_else(|| "null".into()), text(v,"limitations"), v["passed"].as_bool().unwrap_or(false), text(v,"createdAt")]).map_err(err)?;
                    }
                }
                "note" => {
                    tx.execute("INSERT INTO research_notes(id,workspace_id,paper_id,kind,body,state,origin,pinned,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10)", params![id, workspace_id, str_of(&value,"paperId").filter(|p| paper_exists(&tx, p)), text(&value,"kind"), text(&value,"body"), str_of(&value,"state").unwrap_or_else(|| "proposed".into()), text(&value,"origin"), value["pinned"].as_bool().unwrap_or(false), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                }
                record_kind => {
                    let body = value["body"].clone();
                    if body.is_null() {
                        return Err(WorkbenchError::invalid(format!("Record {id} has no body")));
                    }
                    tx.execute("INSERT INTO project_records(id,workspace_id,kind,revision,body_json,updated_at) VALUES(?1,?2,?3,1,?4,?5)", params![id, workspace_id, record_kind, body.to_string(), timestamp]).map_err(err)?;
                }
            }
            tx.execute("INSERT INTO exchange_bases(workspace_id,source_namespace,kind,source_id,local_id,value_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(workspace_id,source_namespace,kind,source_id) DO UPDATE SET value_json=excluded.value_json,local_id=excluded.local_id",params![workspace_id,x.manifest.source_namespace,d.kind,d.id,id,value.to_string()]).map_err(err)?;
        }
    }
    for (path, (hash, size, role)) in &imported_blob_names {
        let suffix = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let media = if role == "compiled_pdf" {
            "pdf".to_string()
        } else {
            suffix.to_string()
        };
        tx.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,original_path,created_at) VALUES(?1,?2,?3,?4,?5,'exchange_import',?6,NULL,?7)", params![random_id("artifact")?, workspace_id, hash, media, *size as i64, path, timestamp]).map_err(err)?;
        tx.execute("INSERT OR IGNORE INTO retained_blobs(storage_reference,reference_type,reference_id,reason,recorded_at) VALUES(?1,'exchange_import',?2,?3,?4)", params![path, import_id, format!("imported from exchange package {}", &x.package_hash[..12]), timestamp]).map_err(err)?;
    }
    let count = |o: &str| decisions.iter().filter(|d| d.outcome == o).count();
    let summary = json!({"new":count("new"),"identical":count("identical"),"remapped":count("remapped"),"conflicts":conflicts,"blobs":imported_blob_names.len(),"createdWorkspace":created});
    tx.execute(
        "UPDATE exchange_imports SET summary_json=?2 WHERE id=?1",
        params![import_id, summary.to_string()],
    )
    .map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(ImportReport {
        import_id,
        workspace_id,
        new: count("new"),
        identical: count("identical"),
        remapped: count("remapped"),
        conflicts,
        blobs: imported_blob_names.len(),
        limitations: x.manifest.limitations,
    })
}
fn paper_exists(tx: &rusqlite::Transaction<'_>, id: &str) -> bool {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM papers WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeConflict {
    pub id: String,
    pub import_id: String,
    pub object_kind: String,
    pub object_id: String,
    pub local: Value,
    pub base: Option<Value>,
    pub imported: Value,
    pub state: String,
    pub recorded_at: String,
}
pub fn list_conflicts(store: &Store, ws: &str) -> WorkbenchResult<Vec<ExchangeConflict>> {
    store.workspace(ws)?;
    let conn = store.connection()?;
    let mut stmt = conn.prepare("SELECT id,import_id,object_kind,object_id,local_json,imported_json,state,recorded_at,base_json FROM exchange_conflicts WHERE workspace_id=?1 ORDER BY recorded_at DESC LIMIT 500").map_err(err)?;
    let rows = stmt
        .query_map([ws], |r| {
            Ok(ExchangeConflict {
                id: r.get(0)?,
                import_id: r.get(1)?,
                object_kind: r.get(2)?,
                object_id: r.get(3)?,
                local: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(Value::Null),
                imported: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Value::Null),
                state: r.get(6)?,
                recorded_at: r.get(7)?,
                base: r
                    .get::<_, Option<String>>(8)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
            })
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    Ok(rows)
}
/// Resolve a conflict. `keep_local` records the decision; `take_imported`
/// replaces the local project record or note body with the imported one under
/// a fresh revision (the previous body stays in history). Papers, executions,
/// sources and ledger entries are immutable and can only be kept.
pub fn resolve_conflict(
    store: &Store,
    ws: &str,
    conflict_id: &str,
    take_imported: bool,
) -> WorkbenchResult<ExchangeConflict> {
    store.workspace(ws)?;
    let conflict = list_conflicts(store, ws)?
        .into_iter()
        .find(|c| c.id == conflict_id)
        .ok_or_else(|| WorkbenchError::invalid("Conflict was not found"))?;
    if conflict.state != "open" {
        return Err(WorkbenchError::invalid("This conflict is already resolved"));
    }
    let mut conn = store.connection()?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(err)?;
    if take_imported {
        match conflict.object_kind.as_str() {
            "note" => {
                tx.execute("UPDATE research_notes SET body=?2, state=?3, pinned=?4, revision=revision+1, updated_at=?5 WHERE id=?1 AND workspace_id=?6", params![conflict.object_id, text(&conflict.imported,"body"), str_of(&conflict.imported,"state").unwrap_or_else(|| "proposed".into()), conflict.imported["pinned"].as_bool().unwrap_or(false), now(), ws]).map_err(err)?;
            }
            kind if RECORD_KINDS.contains(&kind) => {
                let body = conflict.imported["body"].clone();
                tx.execute("UPDATE project_records SET revision=revision+1, body_json=?2, updated_at=?3 WHERE id=?1 AND workspace_id=?4", params![conflict.object_id, body.to_string(), now(), ws]).map_err(err)?;
            }
            _ => return Err(WorkbenchError::invalid(
                "Immutable objects keep the local version; the imported copy stays reviewable here",
            )),
        }
    }
    if take_imported {
        tx.execute("INSERT INTO exchange_bases(workspace_id,source_namespace,kind,source_id,local_id,value_json) SELECT ?1,i.source_namespace,?2,json_extract(d.value,'$.id'),?3,?4 FROM exchange_imports i,json_each(i.decisions_json) d WHERE i.id=?5 AND json_extract(d.value,'$.kind')=?2 AND COALESCE(json_extract(d.value,'$.newId'),json_extract(d.value,'$.id'))=?3 ON CONFLICT(workspace_id,source_namespace,kind,source_id) DO UPDATE SET value_json=excluded.value_json,local_id=excluded.local_id",params![ws,conflict.object_kind,conflict.object_id,conflict.imported.to_string(),conflict.import_id]).map_err(err)?;
    }
    tx.execute(
        "UPDATE exchange_conflicts SET state=?2, resolved_at=?3 WHERE id=?1",
        params![
            conflict_id,
            if take_imported {
                "took_imported"
            } else {
                "kept_local"
            },
            now()
        ],
    )
    .map_err(err)?;
    tx.commit().map_err(err)?;
    list_conflicts(store, ws)?
        .into_iter()
        .find(|c| c.id == conflict_id)
        .ok_or_else(|| WorkbenchError::invalid("Conflict was not found"))
}

#[cfg(test)]
mod tests;
