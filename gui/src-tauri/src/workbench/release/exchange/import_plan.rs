//! Import identity mapping, content comparison, and decision previews.
use super::package::{extract, Extracted};
use super::selection::{index, load_object, Package};
use super::types::{ExchangeTarget, ImportPreview, ObjectDecision};
use super::{err, fingerprint, hex};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;

pub(super) struct LocalObject {
    workspace_id: String,
    pub(super) value: Value,
}
pub(super) fn local_object(
    store: &Store,
    kind: &str,
    id: &str,
) -> WorkbenchResult<Option<LocalObject>> {
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
    // The row exists: it is never "new". A value that cannot be loaded (a
    // retired object, a kind beyond its list view) compares as a conflict
    // rather than letting the import INSERT onto its primary key.
    let value = load_object(store, &ws, &ix, kind, id, &mut scratch)?
        .map(|(value, _)| value)
        .unwrap_or(Value::Null);
    Ok(Some(LocalObject {
        workspace_id: ws,
        value,
    }))
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
pub(super) fn plan(
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
pub(super) fn rewrite_ids(value: &mut Value, map: &HashMap<String, String>) {
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
