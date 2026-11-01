//! Existing target resolution, blob staging, and transactional record import.
use super::authority::{claim_status, evidence_status, note_status, stamp};
use super::import_plan::{local_object, plan, rewrite_ids};
use super::package::extract;
use super::types::{BlobEntry, ExchangeTarget, ImportExchangeRequest, ImportReport};
use super::{err, hash_file, json_text, now, random_id, str_of, text};
use crate::workbench::research::imported_mark;
use crate::workbench::store::{CreateWorkspaceRequest, Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};

pub fn import_exchange(
    store: &Store,
    request: ImportExchangeRequest,
) -> WorkbenchResult<ImportReport> {
    let x = extract(&request.path)?;
    // A new Workspace is saved with the import, so a failed one leaves nothing.
    let (workspace_id, mut new_workspace) = match &request.target {
        ExchangeTarget::ExistingWorkspace { workspace_id } => {
            let w = store.workspace(workspace_id)?;
            if w.archived_at.is_some() {
                return Err(WorkbenchError::invalid("Restore this Workspace first"));
            }
            (workspace_id.clone(), None)
        }
        ExchangeTarget::NewWorkspace { name, root } => {
            let w = store.prepare_workspace(CreateWorkspaceRequest {
                name: name.clone(),
                root: root.clone(),
                operation_id: request.operation_id.clone(),
            })?;
            (w.id.clone(), Some(w))
        }
    };
    let created = new_workspace.is_some();
    let (decisions, map) = plan(store, &x, Some(&workspace_id))?;
    let blob_dir = store.root_path().join("blobs");
    let mut blob_paths: HashMap<String, (PathBuf, String, u64)> = HashMap::new();
    for b in &x.manifest.blobs {
        let name = stored_name(b);
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
    if let Some(workspace) = new_workspace.take() {
        workspace.insert(&tx)?;
    }
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
                    // Already decided: the researcher kept this local value
                    // against this imported one.
                    let decided: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM exchange_conflicts WHERE workspace_id=?1 AND object_kind=?2 AND object_id=?3 AND state='kept_local' AND local_json=?4 AND imported_json=?5)", params![workspace_id, d.kind, id, local.to_string(), value.to_string()], |r| r.get(0)).map_err(err)?;
                    if decided {
                        continue;
                    }
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
                        let input_kind = str_of(r, "inputKind").unwrap_or_else(|| "text".into());
                        // Reviews, page views, and captured-file reads find the
                        // paper's own file by its hash and input kind.
                        let content_hash = text(r, "contentHash");
                        let paper_file = x
                            .manifest
                            .blobs
                            .iter()
                            .filter(|b| b.sha256 == content_hash)
                            .max_by_key(|b| b.role != "paper_text");
                        if let Some(b) = paper_file {
                            if let Some(path) = register_blob(&b.archive_path, "paper_file") {
                                tx.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,original_path,created_at) VALUES(?1,?2,?3,?4,?5,'exchange_import',?6,NULL,?7)", params![random_id("artifact")?, workspace_id, b.sha256, input_kind, b.size_bytes as i64, path, timestamp]).map_err(err)?;
                            }
                        }
                        tx.execute("INSERT INTO paper_revisions(id,paper_id,input_kind,entrypoint,dependency_manifest_json,content_hash,text_reference,compiled_artifact_id,extraction_json,capture_complete,captured_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![rid, id, input_kind, text(r,"entrypoint"), json_text(r,"dependencyManifest").unwrap_or_else(|| "{}".into()), content_hash, text_reference, compiled_id, json_text(r,"extraction").unwrap_or_else(|| "{}".into()), r["captureComplete"].as_bool().unwrap_or(false), text(r,"capturedAt")]).map_err(err)?;
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
                    tx.execute("INSERT INTO research_executions(id,workspace_id,session_id,binding_id,provider_turn_id,tool_call_id,profile_id,adapter,command_json,cwd,environment_identity,input_manifest_json,dependency_hash,outcome,started_at,ended_at,exit_status,stdout_text,stderr_text,output_manifest_json,validation_json,snapshot_consistency,created_at) VALUES(?1,?2,NULL,NULL,NULL,NULL,NULL,?3,?4,?5,NULL,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)", params![id, workspace_id, text(&value,"adapter"), json_text(&value,"command").unwrap_or_else(|| "[]".into()), text(&value,"cwd"), json_text(&value,"inputManifest").unwrap_or_else(|| "{}".into()), text(&value,"dependencyHash"), str_of(&value,"outcome").unwrap_or_else(|| "outcome_unknown".into()), str_of(&value,"startedAt"), str_of(&value,"endedAt"), value["exitStatus"].as_i64(), str_of(&value,"stdout"), str_of(&value,"stderr"), output_manifest.to_string(), stamp(&value["validation"], &x.manifest.source_namespace).to_string(), str_of(&value,"snapshotConsistency").unwrap_or_else(|| "uncertain".into()), text(&value,"createdAt")]).map_err(err)?;
                    for r in value["structuredResults"].as_array().into_iter().flatten() {
                        let mut body = r["body"].clone();
                        body["sourceExecutionId"] = Value::String(id.clone());
                        tx.execute("INSERT OR IGNORE INTO structured_results(id,execution_id,result_id,body_json,created_at) VALUES(?1,?2,?3,?4,?5)", params![random_id("result")?, id, text(r,"resultId"), body.to_string(), timestamp]).map_err(err)?;
                    }
                }
                "claim" => {
                    let (state, sender_status) = claim_status(&value);
                    tx.execute("INSERT INTO claims(id,workspace_id,paper_id,current_version_id,workflow_state,created_at,updated_at) VALUES(?1,?2,?3,NULL,?4,?5,?6)", params![id, workspace_id, str_of(&value,"paperId").filter(|p| paper_exists(&tx, p)), state, text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    for v in value["versions"].as_array().into_iter().flatten() {
                        let origin = match &sender_status {
                            Some(status) => imported_mark(status, &text(v, "origin")),
                            None => text(v, "origin"),
                        };
                        tx.execute("INSERT INTO claim_versions(id,claim_id,version,claim_text,kind,origin,paper_locator_json,dependency_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![text(v,"id"), id, v["version"].as_i64().unwrap_or(1), text(v,"claim"), text(v,"kind"), origin, json_text(v,"paperLocator"), str_of(v,"dependencyHash"), text(v,"createdAt")]).map_err(err)?;
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
                    let (assessment, assessor) = evidence_status(&value);
                    tx.execute("INSERT INTO evidence_links(id,workspace_id,claim_version_id,target_type,target_id,locator_json,relation,assessment,assessor,dependency_hash,freshness,stale_reason,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'unknown',?11,?12,?13)", params![id, workspace_id, claim_version, target_type, target_id, json_text(&value,"locator"), text(&value,"relation"), assessment, assessor, str_of(&value,"dependencyHash"), Some("imported from an exchange package; dependency freshness is not tracked across stores"), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
                    for v in value["verifications"].as_array().into_iter().flatten() {
                        tx.execute("INSERT INTO verification_records(id,evidence_link_id,method,checker_identity,input_hashes_json,observed_result_json,limitations,passed,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![text(v,"id"), id, text(v,"method"), text(v,"checkerIdentity"), json_text(v,"inputHashes").unwrap_or_else(|| "[]".into()), stamp(&v["observedResult"], &x.manifest.source_namespace).to_string(), text(v,"limitations"), v["passed"].as_bool().unwrap_or(false), text(v,"createdAt")]).map_err(err)?;
                    }
                }
                "note" => {
                    let (state, origin) = note_status(&value);
                    tx.execute("INSERT INTO research_notes(id,workspace_id,paper_id,kind,body,state,origin,pinned,revision,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10)", params![id, workspace_id, str_of(&value,"paperId").filter(|p| paper_exists(&tx, p)), text(&value,"kind"), text(&value,"body"), state, origin, value["pinned"].as_bool().unwrap_or(false), text(&value,"createdAt"), text(&value,"updatedAt")]).map_err(err)?;
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
        // A paper's file already has its row, under the revision's input kind.
        if role != "paper_file" {
            tx.execute("INSERT OR IGNORE INTO artifacts(id,workspace_id,content_hash,media_kind,size_bytes,origin,storage_reference,original_path,created_at) VALUES(?1,?2,?3,?4,?5,'exchange_import',?6,NULL,?7)", params![random_id("artifact")?, workspace_id, hash, media, *size as i64, path, timestamp]).map_err(err)?;
        }
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
/// The verified hash names the stored file, and the package's own name is ignored:
/// private blobs are found by name, so a name a package chose could stand in for
/// content it does not hold. Only the suffix travels, as the file's type.
fn stored_name(blob: &BlobEntry) -> String {
    let suffix = Path::new(&blob.archive_path)
        .extension()
        .and_then(|suffix| suffix.to_str())
        .filter(|suffix| suffix.len() <= 16 && suffix.bytes().all(|b| b.is_ascii_alphanumeric()));
    match suffix {
        Some(suffix) => format!("{}.{suffix}", blob.sha256),
        None => blob.sha256.clone(),
    }
}
fn paper_exists(tx: &rusqlite::Transaction<'_>, id: &str) -> bool {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM papers WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}
