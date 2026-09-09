//! Durable portfolio research, dispatched exclusively through the existing task adapters.
pub mod commands;
mod engine;
mod export;
pub mod model;
mod prompts;
pub(crate) mod storage;
#[cfg(test)]
mod tests;
mod validate;

use super::{
    adapters,
    definition::{Action, Binding, Chain, Limits, Step},
    store::{self, now, Result, TaskRun},
    Coordinator,
};
use crate::workbench::{commands::run_store, discovery as workspace, missions as research};
use model::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use tauri::Emitter;

fn notify(c: &Coordinator, r: &Run) {
    c.notify();
    if let Some(app) = &c.app {
        let _ = app.emit("discovery:changed", json!({"id":r.id,"state":r.state}));
    }
}
async fn save(
    c: &Coordinator,
    p: &mut Portfolio,
    kind: &str,
    adopted: Option<String>,
) -> Result<()> {
    let mut copy = p.clone();
    let kind = kind.to_string();
    *p = c
        .db(move |s| {
            storage::save(&s, &mut copy, &kind, adopted.as_deref())?;
            Ok(copy)
        })
        .await?;
    notify(c, &p.run);
    Ok(())
}
pub(crate) async fn tick(c: &Arc<Coordinator>) -> Result<()> {
    let ids = c.db(|s| storage::due(&s)).await?;
    for id in ids {
        let mut p = c.db(move |s| storage::load(&s, &id)).await?;
        if let Err(e) = advance(c, &mut p).await {
            p.run.reason = e;
            if let Some(id) = p.run.active_child.clone() {
                super::missions::stop_child(c, &id).await?;
                p.run.state = "stopping".into();
                p.run.stop_outcome = Some("blocked".into());
                p.run.due_at = Some(now() + 2);
            } else {
                p.run.state = "blocked".into();
                p.run.due_at = None;
            }
            save(c, &mut p, "blocked", None).await?;
        }
        if p.run.terminal() {
            let id = p.run.id.clone();
            run_store(move |s| workspace::revoke(&s, &id))
                .await
                .map_err(|e| e.message)?;
        }
    }
    Ok(())
}
fn account(p: &mut Portfolio, child: &TaskRun) {
    p.run.active_seconds = p.run.active_seconds.saturating_add(
        child
            .progress
            .receipts
            .values()
            .map(|r| {
                r.finished_at
                    .unwrap_or(now())
                    .saturating_sub(r.started_at)
                    .max(0) as u64
            })
            .sum::<u64>(),
    );
    if let Some(index) = p.run.phase.paper() {
        if let Some(paper) = p.papers.get_mut(index) {
            if paper.scope.as_ref().and_then(|s| s.session_id.as_ref())
                == child.scope.session_id.as_ref()
            {
                paper.scope = Some(child.scope.clone());
            }
        }
    }
    p.run.active_child = None;
    p.run.active_scope = None;
}
async fn advance(c: &Arc<Coordinator>, p: &mut Portfolio) -> Result<()> {
    if p.run.terminal() {
        return Ok(());
    }
    if p.run.deadline_at <= now() && p.run.state != "stopping" {
        p.run.reason = "Elapsed research deadline reached; all recorded work is retained".into();
        if let Some(id) = p.run.active_child.clone() {
            super::missions::stop_child(c, &id).await?;
            p.run.state = "stopping".into();
            p.run.stop_outcome = Some("exhausted".into());
            p.run.due_at = Some(now() + 2);
        } else {
            p.run.state = "exhausted".into();
            p.run.due_at = None;
        }
        return save(c, p, "deadline", None).await;
    }
    if let Some(id) = p.run.active_child.clone() {
        let child = c.db(move |s| s.get(&id)).await?;
        if ["finished", "attention", "failed", "cancelled"].contains(&child.state.as_str()) {
            if p.run.state == "stopping" {
                account(p, &child);
                p.run.state = p.run.stop_outcome.clone().unwrap_or("cancelled".into());
                p.run.due_at = None;
                return save(c, p, "stopped", Some(child.id)).await;
            }
            if child.state != "finished" {
                // A lost acknowledgement can contain completed work. Reconcile, never resubmit.
                let mut recovered = child.clone();
                c.recover_results(&mut recovered).await?;
                if recovered
                    .progress
                    .receipts
                    .values()
                    .all(|r| r.state == "completed")
                    && !recovered.progress.receipts.is_empty()
                {
                    recovered.state = "queued".into();
                    recovered.due_at = Some(now());
                    c.save(
                        &mut recovered,
                        "reconciled",
                        "Adopted the recorded result without replaying it",
                    )
                    .await?;
                    return Ok(());
                }
                return Err(format!(
                    "{}; the recorded child was not replayed",
                    child
                        .reason
                        .as_deref()
                        .unwrap_or("A research action did not finish")
                ));
            }
            let paused = p.run.state == "paused";
            let output = child
                .progress
                .outputs
                .get("action")
                .ok_or("Completed research action omitted its output")?
                .clone();
            let mut next = p.clone();
            let result = consume(c, &mut next, &child, &output).await;
            match result {
                Ok(()) => {
                    account(&mut next, &child);
                    *p = next;
                }
                Err(e) => {
                    // Only a completed, journaled model response may receive a bounded repair turn.
                    account(p, &child);
                    if p.run.repairs < 2
                        && !matches!(
                            p.run.phase,
                            Phase::Acquire { .. } | Phase::ExternalReview { .. } | Phase::Deliver
                        )
                    {
                        p.run.repairs += 1;
                        p.run.reason = format!("Repair the response contract: {e}");
                        p.run.due_at = Some(now());
                    } else {
                        p.run.state = "blocked".into();
                        p.run.reason = format!("Research response could not be adopted: {e}");
                        p.run.due_at = None;
                    }
                }
            }
            if paused && !p.run.terminal() {
                p.run.state = "paused".into();
                p.run.due_at = None;
            }
            save(c, p, "adopted", Some(child.id)).await?;
        } else {
            p.run.due_at = Some(now() + 60);
            return save(c, p, "waiting", None).await;
        }
    }
    if p.run.state != "running" {
        return Ok(());
    }
    if p.run.actions_reserved >= p.run.definition.max_actions
        || p.run.active_seconds + 30 > p.run.definition.active_seconds
    {
        p.run.state = "exhausted".into();
        p.run.reason =
            "Research budget reached; recorded papers, reviews and other work remain available"
                .into();
        p.run.due_at = None;
        return save(c, p, "budget", None).await;
    }
    if p.run.catalog_revision != crate::auto_review::catalog_revision() {
        return Err("The research specialist catalog changed; start a new automation using the retained work".into());
    }
    let phase = p.run.phase.clone();
    let writer = matches!(phase, Phase::Research { .. } | Phase::Draft { .. })
        && p.run.definition.allow_computation;
    let persistent = matches!(phase, Phase::Research { .. } | Phase::Draft { .. });
    let source = p.run.source_scope.clone();
    let expected = p.run.source_authority.clone();
    run_store(move |s| research::refresh_scope(&s, &source, &expected))
        .await
        .map_err(|e| e.message)?;
    let scope = if let Some(index) = phase.paper().filter(|_| persistent) {
        if let (Some(scope), Some(fp)) = (&p.papers[index].scope, &p.papers[index].authority) {
            let scope = scope.clone();
            let fp = fp.clone();
            run_store(move |s| research::refresh_scope(&s, &scope, &fp))
                .await
                .map_err(|e| e.message)?
        } else {
            let source = p.run.source_scope.clone();
            let id = p.run.id.clone();
            let deadline = p.run.deadline_at;
            let role = format!("paper-{}", p.papers[index].candidate_id);
            let (scope, fp) =
                run_store(move |s| workspace::role(&s, &source, &id, &role, writer, deadline))
                    .await
                    .map_err(|e| e.message)?;
            materialize(c, &scope, &p.run.input_artifacts).await?;
            p.papers[index]
                .evidence
                .extend(p.run.input_artifacts.clone());
            p.papers[index].scope = Some(scope.clone());
            p.papers[index].authority = Some(fp);
            scope
        }
    } else if matches!(
        phase,
        Phase::Acquire { .. } | Phase::ExternalReview { .. } | Phase::Deliver
    ) {
        p.run.source_scope.clone()
    } else {
        let source = p.run.source_scope.clone();
        let id = p.run.id.clone();
        let deadline = p.run.deadline_at;
        let role = format!("action-{}", p.run.actions_reserved);
        run_store(move |s| workspace::role(&s, &source, &id, &role, false, deadline))
            .await
            .map_err(|e| e.message)?
            .0
    };
    if phase.reviewer() {
        if let Some(index) = phase.paper() {
            let mut files = p.run.input_artifacts.clone();
            for (id, a) in &p.papers[index].evidence {
                if a["kind"] == "artifact" {
                    files.insert(
                        format!("{id}-{}", a["filename"].as_str().unwrap_or("evidence")),
                        a.clone(),
                    );
                }
            }
            if let Some(v) = p.papers[index].versions.last() {
                files.extend(v.artifacts.clone());
            }
            materialize(c, &scope, &files).await?;
        }
    }
    let action = match phase {
        Phase::Acquire { index } => Action::LiteratureLookup {
            query: p
                .run
                .orientation
                .as_ref()
                .ok_or("Orientation missing")?
                .literature_queries[index]
                .clone(),
        },
        Phase::ExternalReview { paper } => Action::Review {
            profile_id: p
                .run
                .definition
                .review_profile_id
                .clone()
                .ok_or("Review profile missing")?,
            input: Binding::Literal {
                value: p.papers[paper]
                    .versions
                    .last()
                    .and_then(|v| v.artifacts.get("paper.md"))
                    .ok_or("Paper snapshot missing")?
                    .clone(),
            },
            interpretation: "document".into(),
            variables: BTreeMap::new(),
        },
        Phase::Deliver => Action::Deliver {
            input: Binding::Literal {
                value: export::delivery(p),
            },
        },
        _ => Action::Workspace {
            prompt: prompts::prompt(p)?,
            model: if phase.reviewer() {
                p.run.definition.reviewer_model.clone()
            } else {
                p.run.definition.author_model.clone()
            },
            effort: None,
        },
    };
    let timeout = u64::from(p.run.definition.action_timeout_seconds).min(
        p.run
            .definition
            .active_seconds
            .saturating_sub(p.run.active_seconds),
    ) as u32;
    let name = format!("Self-discovery · {}", export::phase_name(&p.run.phase));
    let chain = Chain {
        schema_version: 1,
        name: name.clone(),
        description: CONTRACT.into(),
        steps: vec![Step {
            id: "action".into(),
            label: name,
            action,
        }],
        limits: Limits {
            max_actions: 1,
            deadline_hours: p.run.definition.deadline_hours,
            action_timeout_secs: timeout,
        },
    };
    let mut copy = p.clone();
    *p = c
        .db(move |s| {
            storage::admit(&s, &mut copy, chain, scope)?;
            Ok(copy)
        })
        .await?;
    notify(c, &p.run);
    Ok(())
}
async fn consume(
    c: &Coordinator,
    p: &mut Portfolio,
    child: &TaskRun,
    output: &Value,
) -> Result<()> {
    let mut evidence = BTreeMap::new();
    let mut artifacts = BTreeMap::new();
    if let Phase::Research { paper } = p.run.phase {
        let v: Investigation = validate::response(output)?;
        validate::investigation(&v)?;
        let ws = p.run.workspace_id.clone();
        let refs = v.sources.clone();
        let sources = run_store(move |s| research::sources(&s, &ws, &refs))
            .await
            .map_err(|e| e.message)?;
        let round = p.papers[paper].rounds.len() + 1;
        for (i, source) in sources.into_iter().enumerate() {
            evidence.insert(format!("r{round}-source{i}"), source);
        }
        let scope = child.scope.clone();
        let memo = serde_json::to_string_pretty(&v).map_err(store::err)?;
        let files = v.files;
        let captured = c
            .db(move |s| {
                let mut out = BTreeMap::new();
                out.insert(
                    format!("r{round}-dossier"),
                    adapters::snapshot(&s, &scope, json!(memo), "dossier.json")?,
                );
                for (i, path) in files.into_iter().enumerate() {
                    let filename = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .ok_or("Artifact filename missing")?;
                    out.insert(
                        format!("r{round}-file{i}"),
                        adapters::snapshot(&s, &scope, json!({"path":path}), filename)?,
                    );
                }
                check_artifacts(&out)?;
                Ok(out)
            })
            .await?;
        evidence.extend(captured);
    }
    if let Phase::Draft { paper } = p.run.phase {
        let v: Manuscript = validate::response(output)?;
        validate::manuscript(&v, &p.papers[paper])?;
        let scope = child.scope.clone();
        artifacts = c
            .db(move |s| {
                let mut out = BTreeMap::new();
                for (name, text) in [
                    ("paper.md", v.markdown),
                    ("paper.tex", v.latex),
                    ("references.bib", v.bibliography),
                ] {
                    out.insert(
                        name.into(),
                        adapters::snapshot(&s, &scope, json!(text), name)?,
                    );
                }
                for (i, path) in v.files.into_iter().enumerate() {
                    let name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|p| p.to_str())
                        .ok_or("Artifact name missing")?;
                    let artifact = adapters::snapshot(&s, &scope, json!({"path":path}), name)?;
                    if let Some(canonical) = out.get(name) {
                        if canonical["hash"] != artifact["hash"] {
                            return Err(format!(
                                "Listed {name} differs from the returned manuscript source"
                            ));
                        }
                    }
                    out.insert(format!("attachment-{i}-{name}"), artifact);
                }
                if out.keys().any(|k| k.ends_with(".pdf"))
                    && !out
                        .keys()
                        .any(|k| k.starts_with("attachment-") && k.ends_with("-paper.tex"))
                {
                    return Err(
                        "A PDF attachment must include its matching paper.tex source".into(),
                    );
                }
                check_artifacts(&out)?;
                Ok(out)
            })
            .await?;
    }
    engine::consume(p, output, evidence, artifacts)
}
fn check_artifacts(a: &BTreeMap<String, Value>) -> Result<()> {
    if a.values().any(|v| v["kind"] != "artifact") {
        return Err("List individual research files rather than directories".into());
    }
    if a.values()
        .map(|v| v["bytes"].as_u64().unwrap_or(u64::MAX))
        .try_fold(0u64, |a, b| a.checked_add(b))
        .is_none_or(|n| n > 64 * 1024 * 1024)
    {
        return Err("Research artifacts exceed 64 MiB per action".into());
    }
    Ok(())
}

async fn materialize(
    c: &Coordinator,
    scope: &store::Scope,
    artifacts: &BTreeMap<String, Value>,
) -> Result<()> {
    let artifacts = artifacts.clone();
    let entries = c
        .db(move |s| {
            check_artifacts(&artifacts)?;
            let mut entries = Vec::new();
            for (name, a) in artifacts {
                if a["kind"] != "artifact" {
                    continue;
                }
                let path = adapters::artifact_path(&s, &a)?;
                use std::io::Read;
                let mut bytes = Vec::new();
                crate::safety::open_regular_file(std::path::Path::new(&path))?
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(store::err)?;
                entries.push((name, bytes));
            }
            Ok(entries)
        })
        .await?;
    let session = scope
        .session_id
        .clone()
        .ok_or("Research role has no session")?;
    run_store(move |s| {
        let root = workspace::runtime_root(&s, &session)?.ok_or_else(|| {
            crate::workbench::store::WorkbenchError::invalid(
                "Research input destination is not an isolated role",
            )
        })?;
        crate::workbench::project::materialize_research_files(&root, entries)
    })
    .await
    .map_err(|e| e.message)
}
