//! Selected-object loading and dependency closure under Workspace scope.
use super::types::{
    ExchangeSelection, ExternalReference, CAPTURED_OUTPUT_LIMIT, MAX_BLOB_BYTES, MAX_OBJECTS,
    RECORD_KINDS,
};
use super::{err, hash_file};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use crate::workbench::{project, research};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

pub(super) struct Package {
    pub(super) objects: BTreeMap<(String, String), Value>,
    pub(super) titles: BTreeMap<(String, String), String>,
    pub(super) blobs: BTreeMap<String, (PathBuf, String, u64, String)>,
    pub(super) external: Vec<ExternalReference>,
    pub(super) limitations: BTreeSet<String>,
    /// Selected paper files that can't travel, which stop an export.
    pub(super) unpackaged: Vec<String>,
}

pub(super) struct Index {
    share_data: bool,
    records: HashMap<String, String>,
    notes: HashSet<String>,
    executions: HashSet<String>,
    revisions: HashMap<String, String>,
    paper_files: HashMap<String, String>,
    papers: HashSet<String>,
    source_versions: HashMap<String, String>,
    sources: HashSet<String>,
    artifacts: HashMap<String, (String, String, u64, String)>,
    claims: HashSet<String>,
    claim_versions: HashMap<String, String>,
    evidence: HashSet<String>,
}
pub(super) fn index(store: &Store, ws: &str) -> WorkbenchResult<Index> {
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
    let paper_files = list("SELECT pr.id,a.id FROM paper_revisions pr JOIN papers p ON p.id=pr.paper_id JOIN artifacts a ON a.workspace_id=p.workspace_id AND a.content_hash=pr.content_hash AND a.media_kind=pr.input_kind WHERE p.workspace_id=?1")?.into_iter().collect();
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
        paper_files,
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

pub(super) fn load_object(
    store: &Store,
    ws: &str,
    ix: &Index,
    kind: &str,
    id: &str,
    pkg: &mut Package,
) -> WorkbenchResult<Option<(Value, String)>> {
    let conn = store.connection()?;
    Ok(match kind {
        "note" => research::note(store, ws, id)?.map(|n| {
            let title = n.body.chars().take(80).collect::<String>();
            (serde_json::to_value(n).unwrap_or(Value::Null), title)
        }),
        "paper" => {
            let Some(paper) = research::paper_by_id(store, ws, id)? else {
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
            let title = paper.title.clone();
            let mut v = serde_json::to_value(&paper).map_err(err)?;
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
            let Ok(s) = research::source_by_id(store, ws, id) else {
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
                        pkg.limitations.insert(format!(
                            "Extracted text for source version {} was unavailable at export.",
                            v["id"].as_str().unwrap_or_default()
                        ));
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
            let Some(c) = research::claim(store, ws, id)? else {
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
            let Some(e) = research::evidence(store, ws, id)? else {
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
    if path.is_dir() {
        pkg.external.push(ExternalReference {
            kind: "artifact".into(),
            id: artifact_id.into(),
            reason: "This was captured from a folder, which a package can't hold. Its extracted text is included.".into(),
        });
        return;
    }
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
/// A paper revision's own file. It is what page views, captured-file reads, and
/// Reviews open, so a selected file that can't travel stops the export and is
/// named. A folder capture or a text-only paper has no file to send, and is
/// listed instead.
fn include_paper_file(ix: &Index, id: &str, paper: &Value, revision: &Value, pkg: &mut Package) {
    let title = paper["title"].as_str().unwrap_or(id);
    let revision_id = revision["id"].as_str().unwrap_or(id);
    let Some(artifact) = ix.paper_files.get(revision_id) else {
        pkg.external.push(ExternalReference {
            kind: "paper_file".into(),
            id: revision_id.into(),
            reason: format!("{title} has only its extracted text in this project"),
        });
        return;
    };
    let referenced = pkg.external.len();
    include_artifact(ix, artifact, "paper_file", pkg);
    if let Some(reason) = pkg.external.get(referenced) {
        let is_folder = ix
            .artifacts
            .get(artifact)
            .is_some_and(|(reference, ..)| Path::new(reference).is_dir());
        if !is_folder {
            pkg.unpackaged.push(format!("{title}: {}", reason.reason));
        }
    }
}
/// Refuses an oversized selection and says what it holds, so the researcher
/// knows what to narrow.
fn too_many_objects(
    pkg: &Package,
    queue: &VecDeque<(String, String)>,
    next: &str,
) -> WorkbenchError {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (kind, _) in pkg.objects.keys() {
        *counts.entry(kind).or_default() += 1;
    }
    *counts.entry(next).or_default() += 1;
    let waiting: HashSet<_> = queue
        .iter()
        .filter(|key| !pkg.objects.contains_key(key))
        .collect();
    for (kind, _) in waiting {
        *counts.entry(kind).or_default() += 1;
    }
    let held = counts
        .iter()
        .map(|(kind, count)| format!("{kind} {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    WorkbenchError::invalid(format!(
        "A package holds up to 5,000 items, and this selection needs more: {held}. Narrow the selection."
    ))
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

pub(super) fn collect(
    store: &Store,
    ws: &str,
    selection: &ExchangeSelection,
) -> WorkbenchResult<Package> {
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
        unpackaged: Vec::new(),
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
            return Err(too_many_objects(&pkg, &queue, &kind));
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
    let papers: Vec<_> = pkg
        .objects
        .iter()
        .filter(|((k, _), _)| k == "paper")
        .map(|((_, id), v)| (id.clone(), v.clone()))
        .collect();
    if selection.include_compiled_pdfs {
        for (id, paper) in papers {
            for r in paper["revisions"].as_array().into_iter().flatten() {
                if let Some(aid) = r["compiledArtifactId"].as_str() {
                    include_artifact(&ix, aid, "compiled_pdf", &mut pkg);
                }
                include_paper_file(&ix, &id, &paper, r, &mut pkg);
            }
        }
    } else if !papers.is_empty() {
        pkg.limitations.insert("Paper files are not included, so the recipient gets extracted text only: no page views and no Reviews of these papers.".into());
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
