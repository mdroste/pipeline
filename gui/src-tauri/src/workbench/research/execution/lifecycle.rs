//! Execution preparation, waiting, finalization, and cancellation ownership.

use super::authorization::preview_host_execution;
use super::outputs::{adopt_execution_outputs, observed_dependencies};
use super::profiles::get_execution_profile;
use super::types::{
    CancelExecutionRequest, ExecutionProfile, HostExecutionPreview, RunExecutionRequest,
};
use super::{adapters, toolchains};
use super::{
    cancelled_executions, get_execution, input_manifest, run_profile_process,
    validate_execution_outputs, ExecutionOutput, ResearchExecution,
};
use crate::workbench::research::{
    append_change, hash_bytes, new_id, now, open_connection, provider_identifier, validate_id,
};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _, TransactionBehavior};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub(crate) struct PreparedExecution {
    pub store: Store,
    pub request: RunExecutionRequest,
    pub profile: ExecutionProfile,
    pub authorization: HostExecutionPreview,
    pub manifest: Value,
    pub before_outputs: HashMap<String, Option<std::time::SystemTime>>,
    pub execution_id: String,
    pub _project_guard: fs::File,
}
pub(crate) enum PreparedOrExisting {
    Prepared(Box<PreparedExecution>),
    Existing(Box<ResearchExecution>),
}

pub(crate) fn prepare_execution(
    store: &Store,
    request: RunExecutionRequest,
    tool_origin: Option<(&str, &str, &str)>,
) -> WorkbenchResult<PreparedOrExisting> {
    validate_id("execution profile id", &request.profile_id)?;
    let mut profile = get_execution_profile(store, &request.profile_id)?;
    let project_guard = crate::workbench::project::execution_lock(store, &profile.workspace_id)?;
    let mut authorization = if request.plan_id.is_none() {
        Some(preview_host_execution(store, &profile.id)?)
    } else {
        None
    };
    if authorization.as_ref().is_some_and(|a| !a.authorized) {
        return Err(WorkbenchError::invalid("Review and authorize this exact host command and its current inputs before testing or running it"));
    }
    if let Some(session_id) = &request.session_id {
        if store
            .session_snapshot(session_id)?
            .session
            .overrides
            .get("projectCheckpointId")
            .is_some()
        {
            return Err(WorkbenchError::invalid("Workspace host profiles are unavailable in isolated task sessions; use the task sandbox"));
        }
    }
    if request.plan_id.is_none()
        && !request.test_only
        && profile.test_status.as_deref() != Some("passed")
    {
        return Err(WorkbenchError::invalid(
            "Run the profile test successfully before using it for research execution",
        ));
    }
    if let Some(session_id) = request.session_id.as_deref() {
        validate_id("session id", session_id)?;
        let belongs: bool = open_connection(store)?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND workspace_id=?2)",
                params![session_id, profile.workspace_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to scope execution session", error))?;
        if !belongs {
            return Err(WorkbenchError::invalid(
                "Execution session belongs to another workspace",
            ));
        }
    }
    let previous_operation: Option<(String, String)> = open_connection(store)?.query_row("SELECT entity_id,details_json FROM change_log WHERE operation_id=?1 AND entity_type='research_execution'", [&request.operation_id], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|WorkbenchError::storage("Failed to inspect prior execution",e))?;
    if let Some((id, details)) = previous_operation {
        let details: Value = serde_json::from_str(&details)
            .map_err(|e| WorkbenchError::storage("Invalid prior execution receipt", e))?;
        if details["request"]
            != serde_json::to_value(&request)
                .map_err(|e| WorkbenchError::storage("Failed to encode execution request", e))?
        {
            return Err(WorkbenchError::conflict("Execution operation ID was reused with different arguments or has an older incomplete receipt"));
        }
        return Ok(PreparedOrExisting::Existing(Box::new(get_execution(
            store, &id,
        )?)));
    }
    let execution_id = new_id("execution")?;
    let (mut manifest, dependency_hash, consistency) = if let Some(plan_id) = &request.plan_id {
        let (captured, grant, manifest) = crate::workbench::research::execution_plan::stage(
            store,
            &profile.workspace_id,
            plan_id,
            &execution_id,
            request.test_only,
        )?;
        if captured.id != request.profile_id {
            return Err(WorkbenchError::invalid("Plan belongs to another profile"));
        }
        profile = captured;
        authorization = Some(grant);
        let dependency = hash_bytes(
            &serde_json::to_vec(&manifest)
                .map_err(|e| WorkbenchError::storage("Plan manifest", e))?,
        );
        (manifest, dependency, "uncertain".to_string())
    } else {
        input_manifest(&profile)?
    };
    let authorization =
        authorization.ok_or_else(|| WorkbenchError::invalid("Missing execution authorization"))?;
    // Existing outputs cannot count as newly produced artifacts. Record their
    // metadata before launch; unchanged outputs remain explicitly unverified.
    let before_outputs = profile
        .outputs
        .iter()
        .map(|p| {
            let path = Path::new(&profile.cwd).join(p);
            (
                p.clone(),
                fs::metadata(path).ok().and_then(|m| m.modified().ok()),
            )
        })
        .collect::<HashMap<_, _>>();
    let input_size = manifest["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| f["sizeBytes"].as_u64().unwrap_or(0))
        .sum::<u64>();
    if input_size <= 256 * 1024 * 1024 {
        let mut input_profile = profile.clone();
        input_profile.outputs = profile.inputs.clone();
        manifest["snapshots"] = adopt_execution_outputs(store, &input_profile, &execution_id)?;
    } else {
        manifest["snapshotCapture"] =
            json!("Declared inputs exceed the 256 MiB snapshot budget; hashes only");
    }
    let created = now();
    let command_json = serde_json::to_string(&profile.argv)
        .map_err(|error| WorkbenchError::storage("Failed to encode execution command", error))?;
    let (binding_id, provider_turn_id, tool_call_id) = tool_origin
        .map(|(binding, turn, call)| (Some(binding), Some(turn), Some(call)))
        .unwrap_or((None, None, None));
    let mut queue_connection = open_connection(store)?;
    let queue = queue_connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            WorkbenchError::storage("Failed to start research execution receipt", error)
        })?;
    queue.execute("INSERT INTO research_executions (id, workspace_id, session_id, binding_id, provider_turn_id, tool_call_id, profile_id, adapter, command_json, cwd, input_manifest_json, dependency_hash, outcome, output_manifest_json, validation_json, snapshot_consistency, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'queued', '{}', ?15, ?13, ?14)", params![execution_id, profile.workspace_id, request.session_id, binding_id, provider_turn_id, tool_call_id, profile.id, profile.adapter, command_json, profile.cwd, serde_json::to_string(&manifest).unwrap_or_default(), dependency_hash, consistency, created, json!({"processInstance":crate::workbench::research::jobs::process_instance()}).to_string()]).map_err(|error| WorkbenchError::storage("Failed to queue research execution", error))?;
    append_change(
        &queue,
        &request.operation_id,
        "research_execution",
        &execution_id,
        (Some(&profile.workspace_id), request.session_id.as_deref()),
        "queued",
        &json!({"profileId":profile.id,"testOnly":request.test_only,"request":request}),
    )?;
    queue.commit().map_err(|error| {
        WorkbenchError::storage("Failed to commit research execution receipt", error)
    })?;
    Ok(PreparedOrExisting::Prepared(Box::new(PreparedExecution {
        store: store.clone(),
        request,
        profile,
        authorization,
        manifest,
        before_outputs,
        execution_id,
        _project_guard: project_guard,
    })))
}

pub(crate) fn start_execution(store: &Store, execution_id: &str) -> WorkbenchResult<()> {
    open_connection(store)?.execute("UPDATE research_executions SET outcome='running',started_at=?2 WHERE id=?1 AND outcome='queued'", params![execution_id,now()]).map_err(|e|WorkbenchError::storage("Failed to start execution",e))?;
    Ok(())
}
pub(crate) fn wait_execution(prepared: &PreparedExecution) -> ExecutionOutput {
    if cancelled_executions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&prepared.execution_id)
    {
        return ExecutionOutput {
            outcome: "interrupted".into(),
            status: None,
            stdout: vec![],
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
            wrapper_completed_after_stop: true,
            process_started: false,
        };
    }
    if input_manifest(&prepared.profile)
        .map(|(m, _, _)| m["files"] != prepared.manifest["files"])
        .unwrap_or(true)
    {
        return ExecutionOutput { outcome:"failed".into(),status:None,stdout:vec![],stderr:b"Declared inputs changed while queued; no command was launched. Review and authorize current inputs.".to_vec(),stdout_truncated:false,stderr_truncated:false,wrapper_completed_after_stop:false,process_started:false };
    }
    run_profile_process(
        &prepared.execution_id,
        &prepared.profile,
        &prepared.authorization.launch,
    )
    .unwrap_or_else(|error| ExecutionOutput {
        outcome: "outcome_unknown".into(),
        status: None,
        stdout: Vec::new(),
        stderr: error.into_bytes(),
        stdout_truncated: false,
        stderr_truncated: false,
        wrapper_completed_after_stop: false,
        process_started: false,
    })
}
pub(crate) fn finalize_execution(
    store: &Store,
    prepared: PreparedExecution,
    output: ExecutionOutput,
) -> WorkbenchResult<ResearchExecution> {
    let PreparedExecution {
        request,
        profile,
        authorization,
        manifest,
        before_outputs,
        execution_id,
        _project_guard,
        store: _,
    } = prepared;
    let (validation, valid) = validate_execution_outputs(&profile, &output.outcome);
    let (after_manifest, _, _) = input_manifest(&profile)?;
    let inputs_changed = manifest["files"] != after_manifest["files"];
    let stale_outputs = profile
        .outputs
        .iter()
        .filter(|p| {
            let before = before_outputs.get(*p).and_then(|v| *v);
            before.is_some()
                && before
                    == fs::metadata(Path::new(&profile.cwd).join(p))
                        .ok()
                        .and_then(|m| m.modified().ok())
        })
        .cloned()
        .collect::<Vec<_>>();
    let valid = valid && stale_outputs.is_empty() && !inputs_changed;
    let outcome = if output.outcome == "completed" && !valid {
        "failed".to_string()
    } else {
        output.outcome
    };
    let outputs = match adopt_execution_outputs(store, &profile, &execution_id) {
        Ok(outputs) => outputs,
        Err(error) => {
            open_connection(store)?.execute("UPDATE research_executions SET outcome='outcome_unknown',ended_at=?2,validation_json=?3 WHERE id=?1",params![execution_id,now(),json!({"adoptionError":error.message}).to_string()]).map_err(|e|WorkbenchError::storage("Failed to record adoption failure",e))?;
            return Err(error);
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let observed = observed_dependencies(&profile);
    // Direct Stata is stopped with its whole process group; only a custom
    // launcher can leave machine state that needs checking after a forced stop.
    let waits_for_launcher = adapters::waits_for_stata_launcher(&profile.adapter, &profile.argv);
    let runtime_identity = if output.process_started {
        toolchains::runtime_identity(&profile, &authorization.launch)
    } else {
        Value::Null
    };
    let validation = json!({"observedDependencies":observed,"adapter":validation,"stdoutTruncated":output.stdout_truncated,"stderrTruncated":output.stderr_truncated,"executionBoundary":"explicitly_authorized_host","executionContainment":"host_access","capturedInputIdentity":manifest["snapshotConsistency"],"planId":request.plan_id,"authorizationFingerprint":authorization.fingerprint,"authorizedLaunch":authorization.launch,"dependencyCoverage":"declared_only","inputsChanged":inputs_changed,"unchangedPreexistingOutputs":stale_outputs,"stataWrapperReturnedAfterStop":output.wrapper_completed_after_stop,"processStarted":output.process_started,"stataLaunchMode":adapters::stata_mode_label(&profile.adapter, &profile.argv),"stataCleanupRequired":waits_for_launcher && output.process_started && outcome!="completed" && !output.wrapper_completed_after_stop,"runtimeIdentity":runtime_identity});
    let ended = now();
    let journal = json!({"outcome":outcome,"endedAt":ended,"exitStatus":output.status,"stdout":stdout,"stderr":stderr,"outputs":outputs,"validation":validation,"testOnly":request.test_only,"profileId":profile.id,"profileRevision":profile.revision});
    open_connection(store)?
        .execute(
            "UPDATE execution_jobs SET finalization_json=?2 WHERE execution_id=?1",
            params![execution_id, journal.to_string()],
        )
        .map_err(|e| WorkbenchError::storage("Failed to journal adopted execution", e))?;
    crate::workbench::research::execution_finalization::apply(store, &execution_id, &journal)?;
    get_execution(store, &execution_id)
}

#[cfg(test)]
pub fn run_execution(
    store: &Store,
    request: RunExecutionRequest,
) -> WorkbenchResult<ResearchExecution> {
    match prepare_execution(store, request, None)? {
        PreparedOrExisting::Existing(e) => Ok(*e),
        PreparedOrExisting::Prepared(p) => {
            start_execution(store, &p.execution_id)?;
            let output = wait_execution(&p);
            finalize_execution(store, *p, output)
        }
    }
}

pub fn cancel_executions_for_provider_turn(
    store: &Store,
    provider_thread_id: &str,
    provider_turn_id: &str,
) -> WorkbenchResult<usize> {
    provider_identifier("provider thread id", provider_thread_id)?;
    provider_identifier("provider turn id", provider_turn_id)?;
    let connection = open_connection(store)?;
    let mut statement = connection.prepare("SELECT e.id FROM research_executions e JOIN session_bindings b ON b.id=e.binding_id WHERE b.provider_thread_id=?1 AND e.provider_turn_id=?2 AND e.outcome IN ('queued','running')").map_err(|error| WorkbenchError::storage("Failed to prepare turn execution cancellation", error))?;
    let ids = statement
        .query_map(params![provider_thread_id, provider_turn_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| WorkbenchError::storage("Failed to find turn executions", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode turn executions", error))?;
    drop(statement);
    drop(connection);
    let mut cancelled = 0;
    for id in ids {
        if cancel_execution(store, CancelExecutionRequest { execution_id: id }).is_ok() {
            cancelled += 1;
        }
    }
    Ok(cancelled)
}

pub fn cancel_execution(store: &Store, request: CancelExecutionRequest) -> WorkbenchResult<()> {
    validate_id("execution id", &request.execution_id)?;
    let active = get_execution(store, &request.execution_id)?;
    let changed = usize::from(matches!(active.outcome.as_str(), "queued" | "running"));
    open_connection(store)?
        .execute(
            "UPDATE execution_jobs SET cancel_requested=1 WHERE execution_id=?1",
            [&request.execution_id],
        )
        .map_err(|e| WorkbenchError::storage("Failed to request cancellation", e))?;
    if changed == 0 {
        return Err(WorkbenchError::invalid("Research execution is not active"));
    }
    cancelled_executions()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(request.execution_id.clone());
    Ok(())
}
