//! Regression cases for the exchange, relocation and recovery boundaries.
use super::coordinator::{coordinator_fixture, drive};
use super::*;
use crate::workbench::{release, store as ws, tasks::delivery};
use std::io::Read;

fn workspace_session(w: &ws::Store) -> String {
    w.create_session(ws::CreateSessionRequest {
        workspace_id: None,
        title: "Results".into(),
        operation_id: store::id(),
    })
    .unwrap()
    .record
    .id
}

#[cfg(unix)]
#[tokio::test]
async fn delivery_survives_the_producing_runs_retention_pass() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let session = workspace_session(&w);
    let (_temp, m) = coordinator_fixture();
    let plan = chain(vec![
        step(
            "paper",
            Action::Snapshot {
                input: Binding::Literal {
                    value: json!("Verified paper"),
                },
                filename: "paper.md".into(),
                require_change: false,
            },
        ),
        step(
            "returnPaper",
            Action::Deliver {
                input: output("paper", ""),
            },
        ),
    ]);
    let scope = adapters::prepare(&plan, Some(session.clone()), &json!({}))
        .await
        .unwrap();
    let run = m
        .store
        .create("deliver", plan, scope, json!({}), now(), true)
        .unwrap();
    let run = drive(&m, &run.id, "finished").await;
    let file = delivery::list(&w, &session, 0).unwrap().remove(0);
    let source = std::path::PathBuf::from(&run.progress.outputs["paper"]["path"].as_str().unwrap());
    std::fs::File::open(source.parent().unwrap())
        .unwrap()
        .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(40 * 86400))
        .unwrap();
    m.store.mark_seen(&run.id).unwrap();
    m.store
        .connection()
        .unwrap()
        .execute("UPDATE runs SET updated_at=0 WHERE id=?1", [&run.id])
        .unwrap();
    m.store.set_preference("retentionAt", &json!(0)).unwrap();
    m.tick().await.unwrap();
    assert!(m.store.get(&run.id).is_err());
    assert!(!source.exists());
    assert_eq!(
        delivery::read(&w, &session, &file.exchange_id, file.ordinal)
            .unwrap()
            .bytes,
        b"Verified paper"
    );
    assert!(delivery::read(&w, &workspace_session(&w), &file.exchange_id, file.ordinal).is_err());
}

#[test]
fn delivered_tree_and_review_survive_research_backup_and_restore() {
    let root = tempfile::tempdir().unwrap();
    let w = ws::Store::open_at(&root.path().join("workspace")).unwrap();
    let session = workspace_session(&w);
    let s = Store::open(&root.path().join("automation")).unwrap();
    let files = root.path().join("input");
    std::fs::create_dir_all(files.join("sections")).unwrap();
    std::fs::write(files.join("sections/one.tex"), "Proof").unwrap();
    let scope = Scope {
        runtime_root: Some(files.to_string_lossy().into()),
        root_identity: Some(ws::root_identity(&files).unwrap()),
        ..Default::default()
    };
    let tree = adapters::snapshot(&s, &scope, json!({"path":"."}), "paper").unwrap();
    let paper = adapters::snapshot(&s, &scope, json!("Paper"), "paper.md").unwrap();
    let value = json!({"reviewedArtifact":paper,"attachments":[tree]});
    for _ in 0..2 {
        delivery::deliver(
            &w,
            &session,
            "retry",
            value.clone(),
            adapters::delivery::files(&s, &value).unwrap(),
        )
        .unwrap();
    }
    let backup = root
        .path()
        .join("research.pwrx")
        .to_string_lossy()
        .to_string();
    release::export_archive(
        &w,
        release::ExportArchiveRequest {
            path: backup.clone(),
        },
    )
    .unwrap();
    let restored = ws::Store::open_at(&root.path().join("new-workspace")).unwrap();
    release::import_archive(
        &restored,
        release::ImportArchiveRequest {
            path: backup,
            root_mappings: Default::default(),
        },
    )
    .unwrap();
    let rows = delivery::list(&restored, &session, 0).unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let file = delivery::read(&restored, &session, &row.exchange_id, row.ordinal).unwrap();
        if file.name == "paper.md" {
            assert_eq!(file.bytes, b"Paper");
        } else {
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file.bytes)).unwrap();
            let mut content = String::new();
            zip.by_name("sections/one.tex")
                .unwrap()
                .read_to_string(&mut content)
                .unwrap();
            assert_eq!(content, "Proof");
        }
    }
}

#[test]
fn moved_snapshot_always_reads_the_selected_library() {
    let root = tempfile::tempdir().unwrap();
    let old = Store::open(&root.path().join("old")).unwrap();
    let new = Store::open(&root.path().join("new")).unwrap();
    let a = adapters::snapshot(&old, &Scope::default(), json!("Original"), "report.md").unwrap();
    let b = adapters::snapshot(&new, &Scope::default(), json!("Original"), "report.md").unwrap();
    std::fs::write(a["path"].as_str().unwrap(), "Damaged original").unwrap();
    assert_eq!(
        adapters::artifact_path(&new, &a).unwrap(),
        adapters::artifact_path(&new, &b).unwrap()
    );
    std::fs::rename(&old.root, root.path().join("gone")).unwrap();
    assert!(adapters::artifact_path(&new, &a).is_ok());
    std::fs::write(b["path"].as_str().unwrap(), "Tampered copy").unwrap();
    assert!(adapters::artifact_path(&new, &a).is_err());
}

#[tokio::test]
async fn committed_literature_lookup_is_adopted_by_retry_without_dispatch() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let session = crate::workbench::discovery::source(&w, &store::id(), "Lookup", true).unwrap();
    let (_temp, m) = coordinator_fixture();
    let plan = chain(vec![step(
        "lookup",
        Action::LiteratureLookup {
            query: "productivity".into(),
        },
    )]);
    let scope = adapters::prepare(&plan, Some(session), &json!({}))
        .await
        .unwrap();
    let ws = scope.workspace_id.clone().unwrap();
    let mut run = m
        .store
        .create("lookup", plan, scope, json!({}), now(), true)
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], Value::Null);
    let receipt = run.progress.receipts.get_mut("lookup").unwrap();
    receipt.state = "running".into();
    receipt.output = None;
    let op = receipt.operation.clone();
    run.state = "running".into();
    run.progress.outputs = json!({});
    run.progress.actions = 1;
    crate::workbench::acquisition::record_lookup(&w, &ws, "productivity", Ok(Vec::new()), &op)
        .unwrap();
    m.store.save(&mut run, "stepStarted", "Lookup").unwrap();
    m.store.recover().unwrap();
    let interrupted = m.store.get(&run.id).unwrap();
    let retried =
        super::super::commands::control(&m, run.id.clone(), interrupted.revision, "retry".into())
            .await
            .unwrap();
    assert_eq!(retried.progress.receipts["lookup"].operation, op);
    assert_eq!(retried.progress.receipts["lookup"].state, "completed");
    assert_eq!(retried.progress.outputs["lookup"]["query"], "productivity");
    let finished = drive(&m, &run.id, "finished").await;
    assert_eq!(finished.progress.actions, 1);
}

#[tokio::test]
async fn nested_action_limit_prevents_second_dispatch_after_restart() {
    let (_temp, m) = coordinator_fixture();
    let mut group = chain(vec![
        step("first", Action::Delay { seconds: 0 }),
        step("second", Action::Delay { seconds: 0 }),
    ]);
    group.limits.max_actions = 1;
    let run = m
        .store
        .create(
            "group",
            chain(vec![step(
                "bounded",
                Action::Chain {
                    chain: Box::new(group),
                },
            )]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    m.tick().await.unwrap();
    m.store.recover().unwrap();
    m.tick().await.unwrap();
    let r = m.store.get(&run.id).unwrap();
    assert_eq!(r.state, "attention");
    assert_eq!(r.progress.actions, 1);
    assert!(!r.progress.receipts.contains_key("bounded/second"));
}

#[tokio::test]
async fn legacy_delivery_backfill_is_idempotent() {
    let _turn = exclusive_turn_state().await;
    let w = disposable_workspace();
    let session = workspace_session(&w);
    let (_temp, m) = coordinator_fixture();
    let a = adapters::snapshot(
        &m.store,
        &Scope::default(),
        json!("Legacy paper"),
        "paper.md",
    )
    .unwrap();
    let input = json!({"reviewedArtifact":a});
    let plan = chain(vec![step(
        "return",
        Action::Deliver {
            input: Binding::Literal {
                value: input.clone(),
            },
        },
    )]);
    let scope = adapters::prepare(&plan, Some(session.clone()), &json!({}))
        .await
        .unwrap();
    let mut run = m
        .store
        .create("legacy", plan, scope, json!({}), now(), true)
        .unwrap();
    let Advance::Ready(ready) = state::advance(&run.chain, &mut run.progress, &run.inputs) else {
        panic!()
    };
    done(&mut run.progress, &ready[0], input.clone());
    let key = format!("task-result-{}", run.progress.receipts["return"].operation);
    m.store
        .save(&mut run, "stepCompleted", "Delivered")
        .unwrap();
    let value = json!({"sessionId":session,"origin":"task","result":input});
    w.connection().unwrap().execute("INSERT INTO task_exchanges(id,session_id,kind,value_json,created_at) VALUES(?1,?2,'result',?3,'2026-01-01')",rusqlite::params![key,session,value.to_string()]).unwrap();
    for _ in 0..2 {
        adapters::delivery::backfill(Some(&m.store), Some(session.clone()))
            .await
            .unwrap();
    }
    assert_eq!(delivery::list(&w, &session, 0).unwrap().len(), 1);
    assert_eq!(
        delivery::read(&w, &session, &key, 0).unwrap().bytes,
        b"Legacy paper"
    );
    assert_eq!(
        crate::workbench::tasks::delivery_outcome(
            &w,
            &session,
            key.strip_prefix("task-result-").unwrap()
        )
        .unwrap()
        .unwrap(),
        value
    );
}

#[test]
fn relocated_tree_verifies_its_members() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("inputs");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.tex"), "Proof").unwrap();
    let old = Store::open(&tmp.path().join("old")).unwrap();
    let scope = Scope {
        runtime_root: Some(source.to_string_lossy().into()),
        root_identity: Some(ws::root_identity(&source).unwrap()),
        ..Default::default()
    };
    let tree = adapters::snapshot(&old, &scope, json!({"path":"."}), "paper").unwrap();
    let dest = tmp.path().join("new");
    std::fs::rename(&old.root, &dest).unwrap();
    let moved = Store::open(&dest).unwrap();
    let tree_dir = adapters::artifact_path(&moved, &tree).unwrap();
    std::fs::write(
        std::path::Path::new(&tree_dir).join("main.tex"),
        "Replacement",
    )
    .unwrap();
    assert!(adapters::artifact_path(&moved, &tree).is_err());
}

#[tokio::test]
async fn retry_does_not_refill_a_groups_action_budget() {
    let (_temp, m) = coordinator_fixture();
    let mut group = chain(vec![step("leaf", Action::Delay { seconds: 0 })]);
    group.limits.max_actions = 1;
    let mut r = m
        .store
        .create(
            "retry-group",
            chain(vec![step(
                "group",
                Action::Chain {
                    chain: Box::new(group),
                },
            )]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    let Advance::Ready(ready) = state::advance(&r.chain, &mut r.progress, &r.inputs) else {
        panic!()
    };
    done(&mut r.progress, &ready[0], Value::Null);
    r.progress.receipts.get_mut("group/leaf").unwrap().state = "failed".into();
    r.progress.actions = 1;
    r.state = "attention".into();
    m.store.save(&mut r, "attention", "Failed").unwrap();
    m.store.retry(&mut r).unwrap();
    m.tick().await.unwrap();
    let after = m.store.get(&r.id).unwrap();
    assert_eq!(after.state, "attention");
    assert_eq!(after.progress.actions, 1);
    assert!(after.progress.receipts.is_empty());
}

#[test]
fn a_json_reply_cannot_grant_access_to_another_runs_captured_file() {
    let (_temp, m) = coordinator_fixture();
    let other = adapters::snapshot(
        &m.store,
        &Scope::default(),
        json!("Private result"),
        "private.md",
    )
    .unwrap();
    let binding = output("agent", "");
    let plan = chain(vec![
        step(
            "agent",
            Action::Input {
                prompt: "Result".into(),
                timeout_seconds: None,
            },
        ),
        step(
            "return",
            Action::Deliver {
                input: binding.clone(),
            },
        ),
    ]);
    let mut run = m
        .store
        .create(
            "foreign-handle",
            plan,
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    run.progress.outputs["agent"] = other.clone();
    assert!(adapters::delivery::action_files(&m.store, &run, &binding, &other).is_err());
}
