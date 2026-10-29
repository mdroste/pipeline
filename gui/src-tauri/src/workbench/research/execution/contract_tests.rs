//! Disposable process-boundary and finalization contracts before owner extraction.

use super::{
    authorize_host_execution, cancel_execution, cancelled_executions, finalize_execution,
    get_execution, get_execution_profile, prepare_execution, preview_host_execution,
    save_execution_profile, start_execution, wait_execution, CancelExecutionRequest,
    ExecutionOutput, PreparedExecution, PreparedOrExisting, RunExecutionRequest,
    SaveExecutionProfileRequest,
};
use crate::workbench::store::{CreateWorkspaceRequest, Store};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

struct Fixture {
    _temporary: tempfile::TempDir,
    store: Store,
    root: PathBuf,
    workspace_id: String,
}

fn fixture() -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temporary.path().join("store")).unwrap();
    let workspace_id = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Execution contract".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create-workspace".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temporary: temporary,
        store,
        root,
        workspace_id,
    }
}

fn prepare(f: &Fixture, script: &str, outputs: &[&str]) -> PreparedExecution {
    let profile = save_execution_profile(
        &f.store,
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.workspace_id.clone(),
            name: "Contract profile".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            timeout_seconds: 5,
            outputs: outputs.iter().map(|value| (*value).into()).collect(),
            expected_revision: None,
            operation_id: "save-profile".into(),
        },
    )
    .unwrap();
    let preview = preview_host_execution(&f.store, &profile.id).unwrap();
    authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    let PreparedOrExisting::Prepared(prepared) = prepare_execution(
        &f.store,
        RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id,
            session_id: None,
            test_only: true,
            operation_id: "execute-contract".into(),
        },
        None,
    )
    .unwrap() else {
        panic!("A fresh operation must prepare an execution");
    };
    // Keep this fixture outside the global asynchronous queue while retaining
    // the same durable job row consumed by finalization.
    f.store.connection().unwrap().execute(
        "INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,'execute-contract','contract','detached','contract-instance','2026-01-01T00:00:00Z')",
        [&prepared.execution_id],
    ).unwrap();
    *prepared
}

fn completed_output() -> ExecutionOutput {
    ExecutionOutput {
        outcome: "completed".into(),
        status: Some(0),
        stdout: b"completed output\n".to_vec(),
        stderr: vec![],
        stdout_truncated: false,
        stderr_truncated: false,
        wrapper_completed_after_stop: false,
        process_started: true,
    }
}

fn journal(store: &Store, id: &str) -> Option<String> {
    store
        .connection()
        .unwrap()
        .query_row(
            "SELECT finalization_json FROM execution_jobs WHERE execution_id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap()
}

fn artifact_count(store: &Store) -> i64 {
    store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn cancellation_before_launch_retains_an_unstarted_interrupted_receipt() {
    let f = fixture();
    let prepared = prepare(&f, "printf launched > marker", &[]);
    let id = prepared.execution_id.clone();
    cancel_execution(
        &f.store,
        CancelExecutionRequest {
            execution_id: id.clone(),
        },
    )
    .unwrap();
    let output = wait_execution(&prepared);
    cancelled_executions().lock().unwrap().remove(&id);
    assert_eq!(output.outcome, "interrupted");
    assert!(!output.process_started);
    assert!(output.wrapper_completed_after_stop);
    assert!(!f.root.join("marker").exists());
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();
    assert_eq!(receipt.outcome, "interrupted");
    assert_eq!(receipt.validation["processStarted"], false);
    assert_eq!(receipt.validation["stataCleanupRequired"], false);
    assert!(journal(&f.store, &id).is_some());
    assert!(crate::workbench::project::execution_lock(&f.store, &f.workspace_id).is_ok());
}

#[test]
fn spawn_failure_retains_unknown_outcome_without_claiming_a_started_process() {
    let f = fixture();
    let prepared = prepare(&f, "exit 0", &[]);
    start_execution(&f.store, &prepared.execution_id).unwrap();
    // The executable identity remains authorized, but its working directory
    // disappears between preparation and spawn.
    fs::remove_dir(&f.root).unwrap();
    let output = wait_execution(&prepared);
    assert_eq!(output.outcome, "outcome_unknown");
    assert!(!output.process_started);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to start research execution"));
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();
    assert_eq!(receipt.outcome, "outcome_unknown");
    assert_eq!(receipt.validation["processStarted"], false);
    assert!(receipt.output_manifest["artifacts"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn adoption_failure_leaves_unknown_receipt_without_a_completed_journal() {
    let f = fixture();
    let prepared = prepare(&f, "exit 0", &["result.txt"]);
    let id = prepared.execution_id.clone();
    let profile = prepared.profile.id.clone();
    start_execution(&f.store, &id).unwrap();
    let outside = f._temporary.path().join("outside.txt");
    fs::write(&outside, b"outside output").unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("result.txt")).unwrap();
    let error = finalize_execution(&f.store, prepared, completed_output()).unwrap_err();
    let receipt = get_execution(&f.store, &id).unwrap();
    assert_eq!(receipt.outcome, "outcome_unknown");
    assert_eq!(receipt.validation, json!({"adoptionError":error.message}));
    assert_eq!(receipt.output_manifest, json!({}));
    assert!(journal(&f.store, &id).is_none());
    assert_eq!(artifact_count(&f.store), 0);
    assert!(get_execution_profile(&f.store, &profile)
        .unwrap()
        .test_status
        .is_none());
    assert_eq!(fs::read(&outside).unwrap(), b"outside output");
    assert!(crate::workbench::project::execution_lock(&f.store, &f.workspace_id).is_ok());
}

#[test]
fn finalization_journal_replay_is_idempotent_and_identity_bound() {
    let f = fixture();
    let prepared = prepare(&f, "exit 0", &["result.txt"]);
    let id = prepared.execution_id.clone();
    let profile_id = prepared.profile.id.clone();
    start_execution(&f.store, &id).unwrap();
    fs::write(f.root.join("result.txt"), b"fresh output").unwrap();
    let receipt = finalize_execution(&f.store, prepared, completed_output()).unwrap();
    assert_eq!(receipt.outcome, "completed");
    let profile = get_execution_profile(&f.store, &profile_id).unwrap();
    assert_eq!(profile.test_status.as_deref(), Some("passed"));
    let original_journal = journal(&f.store, &id).unwrap();
    let value: Value = serde_json::from_str(&original_journal).unwrap();
    assert_eq!(artifact_count(&f.store), 1);
    for _ in 0..2 {
        crate::workbench::research::execution_finalization::apply(&f.store, &id, &value).unwrap();
        assert_eq!(get_execution(&f.store, &id).unwrap(), receipt);
        assert_eq!(
            get_execution_profile(&f.store, &profile_id).unwrap(),
            profile
        );
        assert_eq!(artifact_count(&f.store), 1);
        assert_eq!(
            journal(&f.store, &id).as_deref(),
            Some(original_journal.as_str())
        );
    }
    for field in ["profileId", "planId"] {
        let mut mismatched = value.clone();
        if field == "planId" {
            mismatched["validation"][field] = json!("different-plan");
        } else {
            mismatched[field] = json!("different-profile");
        }
        assert!(crate::workbench::research::execution_finalization::apply(
            &f.store,
            &id,
            &mismatched
        )
        .is_err());
        assert_eq!(get_execution(&f.store, &id).unwrap(), receipt);
        assert_eq!(
            get_execution_profile(&f.store, &profile_id).unwrap(),
            profile
        );
    }
}

#[test]
fn successful_exit_keeps_unchanged_preexisting_output_unverified() {
    let f = fixture();
    fs::write(f.root.join("old.txt"), b"old retained output").unwrap();
    let prepared = prepare(&f, "exit 0", &["old.txt"]);
    let profile_id = prepared.profile.id.clone();
    start_execution(&f.store, &prepared.execution_id).unwrap();
    let output = wait_execution(&prepared);
    assert_eq!(output.outcome, "completed");
    assert!(output.process_started);
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();
    assert_eq!(receipt.outcome, "failed");
    assert_eq!(
        receipt.validation["unchangedPreexistingOutputs"],
        json!(["old.txt"])
    );
    assert_eq!(receipt.validation["inputsChanged"], false);
    assert_eq!(
        receipt.output_manifest["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fs::read(f.root.join("old.txt")).unwrap(),
        b"old retained output"
    );
    assert_eq!(
        get_execution_profile(&f.store, &profile_id)
            .unwrap()
            .test_status
            .as_deref(),
        Some("failed")
    );
}
