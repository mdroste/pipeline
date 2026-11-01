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
    prepare_with_inputs(f, script, &[], outputs)
}

fn prepare_with_inputs(
    f: &Fixture,
    script: &str,
    inputs: &[&str],
    outputs: &[&str],
) -> PreparedExecution {
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
            inputs: inputs.iter().map(|value| (*value).into()).collect(),
            timeout_seconds: 30,
            outputs: outputs.iter().map(|value| (*value).into()).collect(),
            expected_revision: None,
            operation_id: "save-profile".into(),
        },
    )
    .unwrap();
    let preview = preview_host_execution(&f.store, &profile.id).unwrap();
    authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    let PreparedOrExisting::Prepared(prepared, _) = prepare_execution(
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
fn spawn_failure_is_a_failed_run_that_claims_no_started_process() {
    let f = fixture();
    let prepared = prepare(&f, "exit 0", &[]);
    start_execution(&f.store, &prepared.execution_id).unwrap();
    // The executable identity remains authorized, but its working directory
    // disappears between preparation and spawn.
    fs::remove_dir(&f.root).unwrap();
    let output = wait_execution(&prepared);
    assert_eq!(output.outcome, "failed");
    assert!(!output.process_started);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to start research execution"));
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();
    assert_eq!(receipt.outcome, "failed");
    assert_eq!(receipt.validation["processStarted"], false);
    assert!(receipt.output_manifest["artifacts"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn a_linked_output_is_reported_without_following_it_and_keeps_the_run() {
    let f = fixture();
    let prepared = prepare(&f, "exit 0", &["result.txt"]);
    let id = prepared.execution_id.clone();
    start_execution(&f.store, &id).unwrap();
    let outside = f._temporary.path().join("outside.txt");
    fs::write(&outside, b"outside output").unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("result.txt")).unwrap();
    let receipt = finalize_execution(&f.store, prepared, completed_output()).unwrap();
    assert_eq!(receipt.outcome, "completed");
    assert_eq!(receipt.exit_status, Some(0));
    assert_eq!(receipt.stdout.as_deref(), Some("completed output\n"));
    assert!(journal(&f.store, &id).is_some());
    let entry = &receipt.output_manifest["notRetained"][0];
    assert_eq!(entry["path"], "result.txt");
    assert!(entry.get("contentHash").is_none(), "{entry}");
    let reason = entry["reason"].as_str().unwrap();
    assert!(reason.contains("symbolic link"), "{reason}");
    assert!(!reason.contains("Paper"), "{reason}");
    assert_eq!(artifact_count(&f.store), 0);
    assert_eq!(fs::read(&outside).unwrap(), b"outside output");
    assert!(crate::workbench::project::execution_lock(&f.store, &f.workspace_id).is_ok());
}

fn sparse_file(path: &std::path::Path, bytes: u64) {
    fs::File::create(path).unwrap().set_len(bytes).unwrap();
}

const MIB: u64 = 1024 * 1024;

#[test]
fn oversized_declared_files_keep_their_hash_and_the_finished_run_keeps_its_logs() {
    let f = fixture();
    sparse_file(&f.root.join("panel.dta"), 257 * MIB);
    let script = "printf 'read the panel\\n'; printf 'a warning\\n' >&2; \
                  printf x | dd of=estimates.dta bs=1048576 seek=201 2>/dev/null; exit 0";
    let prepared = prepare_with_inputs(&f, script, &["panel.dta"], &["estimates.dta"]);
    let id = prepared.execution_id.clone();
    let input = &prepared.manifest["files"][0];
    assert_eq!(input["sizeBytes"], 257 * MIB);
    assert!(input["contentHash"].is_string());
    assert!(input["snapshot"].as_str().unwrap().contains("Hash only"));
    assert!(prepared.manifest["snapshotCapture"].is_string());
    assert!(prepared.manifest.get("snapshots").is_none());

    start_execution(&f.store, &id).unwrap();
    let output = wait_execution(&prepared);
    assert_eq!(output.outcome, "completed", "{output:?}");
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();

    assert_eq!(receipt.outcome, "completed");
    assert_eq!(receipt.exit_status, Some(0));
    assert!(receipt.output_manifest["artifacts"]
        .as_array()
        .unwrap()
        .is_empty());
    let kept = &receipt.output_manifest["notRetained"][0];
    assert_eq!(kept["path"], "estimates.dta");
    assert_eq!(kept["sizeBytes"], 201 * MIB + 1);
    assert_eq!(
        kept["contentHash"].as_str().map(str::len),
        Some(64),
        "{kept}"
    );
    assert!(kept["reason"].as_str().unwrap().contains("200 MiB"));
    assert_eq!(artifact_count(&f.store), 0);

    let logs =
        crate::workbench::research::jobs::execution_output(&f.store, &f.workspace_id, &id).unwrap();
    assert_eq!(logs.stdout.as_deref(), Some("read the panel\n"));
    assert_eq!(logs.stderr.as_deref(), Some("a warning\n"));
    assert_eq!(
        get_execution(&f.store, &id).unwrap().stdout.as_deref(),
        Some("read the panel\n")
    );
}

#[test]
fn a_failed_command_reports_its_own_outcome_when_its_output_is_oversized() {
    let f = fixture();
    let prepared = prepare(
        &f,
        "printf partial; printf x | dd of=big.out bs=1048576 seek=201 2>/dev/null; exit 3",
        &["big.out"],
    );
    let id = prepared.execution_id.clone();
    start_execution(&f.store, &id).unwrap();
    let output = wait_execution(&prepared);
    let receipt = finalize_execution(&f.store, prepared, output).unwrap();
    assert_eq!(receipt.outcome, "failed");
    assert_eq!(receipt.exit_status, Some(3));
    assert_eq!(receipt.stdout.as_deref(), Some("partial"));
    assert_eq!(receipt.output_manifest["notRetained"][0]["path"], "big.out");
    assert!(journal(&f.store, &id).is_some());
}

#[test]
fn an_adoption_error_still_saves_the_logs_and_the_process_outcome() {
    use std::os::unix::fs::PermissionsExt as _;
    let f = fixture();
    let prepared = prepare(
        &f,
        "printf done; printf result > result.txt",
        &["result.txt"],
    );
    let id = prepared.execution_id.clone();
    start_execution(&f.store, &id).unwrap();
    let output = wait_execution(&prepared);
    assert_eq!(output.outcome, "completed", "{output:?}");
    let blobs = f.store.root_path().join("blobs");
    fs::set_permissions(&blobs, fs::Permissions::from_mode(0o500)).unwrap();
    let result = finalize_execution(&f.store, prepared, output);
    fs::set_permissions(&blobs, fs::Permissions::from_mode(0o700)).unwrap();
    let receipt = result.unwrap();
    assert_eq!(receipt.outcome, "completed");
    assert_eq!(receipt.exit_status, Some(0));
    assert_eq!(receipt.stdout.as_deref(), Some("done"));
    assert!(journal(&f.store, &id).is_some());
    let entry = &receipt.output_manifest["notRetained"][0];
    assert_eq!(entry["path"], "result.txt");
    assert!(entry["contentHash"].is_string(), "{entry}");
    assert!(entry["reason"].is_string(), "{entry}");
    assert_eq!(artifact_count(&f.store), 0);
}

#[test]
fn an_input_within_the_snapshot_budget_is_kept_even_above_the_import_limit() {
    let f = fixture();
    sparse_file(&f.root.join("sample.dta"), 201 * MIB);
    let profile = save_execution_profile(
        &f.store,
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.workspace_id.clone(),
            name: "Budget".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["sample.dta".into()],
            timeout_seconds: 5,
            outputs: vec![],
            expected_revision: None,
            operation_id: "save-budget".into(),
        },
    )
    .unwrap();
    let snapshots = super::outputs::snapshot_execution_inputs(&f.store, &profile, "execution");
    assert!(snapshots.get("notRetained").is_none(), "{snapshots}");
    assert_eq!(snapshots["artifacts"][0]["path"], "sample.dta");
    assert_eq!(snapshots["artifacts"][0]["sizeBytes"], 201 * MIB);
    assert_eq!(artifact_count(&f.store), 1);
}

fn input_error(f: &Fixture, inputs: &[&str]) -> (Value, String) {
    let profile = save_execution_profile(
        &f.store,
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.workspace_id.clone(),
            name: "Input problems".into(),
            adapter: "command".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: inputs.iter().map(|value| (*value).into()).collect(),
            timeout_seconds: 5,
            outputs: vec![],
            expected_revision: None,
            operation_id: "save-input-problems".into(),
        },
    )
    .unwrap();
    let message = preview_host_execution(&f.store, &profile.id)
        .unwrap_err()
        .message;
    let manifest = super::input_manifest(&get_execution_profile(&f.store, &profile.id).unwrap())
        .unwrap()
        .0;
    (manifest, message)
}

#[test]
fn inputs_are_absent_linked_or_unreadable_rather_than_all_missing() {
    use std::os::unix::fs::PermissionsExt as _;
    let f = fixture();
    let outside = f._temporary.path().join("outside.csv");
    fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, f.root.join("linked.csv")).unwrap();
    let locked = f.root.join("locked.csv");
    fs::write(&locked, b"locked").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let (manifest, message) = input_error(&f, &["gone.csv", "linked.csv", "locked.csv"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();

    assert_eq!(manifest["complete"], false);
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files[0], json!({"path":"gone.csv","missing":true}));
    assert!(files[1]["unavailable"]
        .as_str()
        .unwrap()
        .contains("symbolic link"));
    assert!(files[1].get("missing").is_none());
    assert!(files[2]["unavailable"]
        .as_str()
        .unwrap()
        .contains("can't be read"));
    assert!(files[2].get("missing").is_none());
    for expected in [
        "gone.csv was not found",
        "linked.csv is a symbolic link",
        "locked.csv can't be read",
    ] {
        assert!(message.contains(expected), "{message}");
    }
    assert!(!message.contains("Paper"), "{message}");
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
