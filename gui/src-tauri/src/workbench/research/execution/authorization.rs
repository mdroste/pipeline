//! Declared input identities and exact host-execution authorization.

use super::file_identity::{hash_regular_file, FileProblem, SNAPSHOT_BUDGET};
use super::launch::launch_manifest;
use super::profiles::get_execution_profile;
use super::types::{ExecutionProfile, HostExecutionPreview};
use crate::workbench::research::{hash_bytes, now, open_connection};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::params;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub(crate) fn input_manifest(
    profile: &ExecutionProfile,
) -> WorkbenchResult<(Value, String, String)> {
    let cwd = Path::new(&profile.cwd);
    let mut files = Vec::new();
    let mut complete = true;
    for relative in &profile.inputs {
        let path = cwd.join(relative);
        match hash_regular_file(&path) {
            // Usable and authorizable: its identity is the hash either way.
            Ok((hash, size)) if size > SNAPSHOT_BUDGET => files.push(
                json!({"path":relative,"contentHash":hash,"sizeBytes":size,"snapshot":"Hash only: larger than the 256 MiB snapshot budget"}),
            ),
            Ok((hash, size)) => {
                files.push(json!({"path":relative,"contentHash":hash,"sizeBytes":size}))
            }
            Err(FileProblem::Absent) => {
                complete = false;
                files.push(json!({"path":relative,"missing":true}));
            }
            Err(problem) => {
                complete = false;
                files.push(json!({"path":relative,"unavailable":problem.reason()}));
            }
        }
    }
    let manifest = json!({"complete":complete,"declarationComplete":complete,"dependencyCoverage":"declared_only","snapshotConsistency":"unverified_live_files","files":files});
    let encoded =
        serde_json::to_vec(&json!({"profile":profile,"manifest":manifest})).map_err(|error| {
            WorkbenchError::storage("Failed to encode execution dependency manifest", error)
        })?;
    Ok((
        manifest,
        hash_bytes(&encoded),
        if complete { "uncertain" } else { "partial" }.to_string(),
    ))
}

/// Names each unusable input, because "unavailable" alone sends a researcher
/// looking for a file that may be right there.
fn input_problems(inputs: &Value) -> String {
    let problems = inputs["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let path = file["path"].as_str()?;
            let reason = file["unavailable"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| (file["missing"] == true).then(|| FileProblem::Absent.reason()))?;
            Some(format!("{path} {reason}"))
        })
        .collect::<Vec<_>>();
    format!(
        "Fix these declared inputs before authorizing this command: {}.",
        problems.join("; ")
    )
}

pub fn preview_host_execution(
    store: &Store,
    profile_id: &str,
) -> WorkbenchResult<HostExecutionPreview> {
    let profile = get_execution_profile(store, profile_id)?;
    let root = store
        .workspace(&profile.workspace_id)?
        .root
        .ok_or_else(|| {
            WorkbenchError::invalid("Attach a folder to this project before approving this command")
        })?;
    let root = fs::canonicalize(root)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution root", e))?;
    let cwd = fs::canonicalize(&profile.cwd)
        .map_err(|e| WorkbenchError::storage("Failed to resolve execution directory", e))?;
    if !cwd.starts_with(&root) {
        return Err(WorkbenchError::invalid(
            "Execution directory no longer belongs to this Workspace",
        ));
    }
    let (inputs, _, _) = input_manifest(&profile)?;
    if inputs["complete"] != true {
        return Err(WorkbenchError::invalid(input_problems(&inputs)));
    }
    let launch = launch_manifest(&profile)?;
    let fingerprint=hash_bytes(&serde_json::to_vec(&json!({"profileId":profile.id,"revision":profile.revision,"argv":profile.argv,"cwd":cwd,"rootIdentity":crate::workbench::store::root_identity(&root)?,"environment":profile.environment,"launch":launch,"inputs":inputs,"outputs":profile.outputs,"timeout":profile.timeout_seconds})).map_err(|e|WorkbenchError::storage("Failed to fingerprint host execution",e))?);
    let authorized=open_connection(store)?.query_row("SELECT EXISTS(SELECT 1 FROM execution_authorizations WHERE profile_id=?1 AND fingerprint=?2)",params![profile.id,fingerprint],|r|r.get(0)).map_err(|e|WorkbenchError::storage("Failed to read execution authorization",e))?;
    Ok(HostExecutionPreview{profile_id:profile.id,fingerprint,command:profile.argv,cwd:profile.cwd,inputs,launch,outputs:profile.outputs,authorized,boundary:"Host execution: this command runs with your operating-system access, outside the conversation sandbox. It may read/write other files or use the network. Authorization covers this exact profile, resolved executable, known shell startup files, and current declared input bytes. Changes require a new review. Other sourced scripts and transitive dependencies remain unverified. Dependency coverage remains declared-only.".into()})
}

// UI-only capability grant. This operation is deliberately not a dynamic tool.
pub fn authorize_host_execution(
    store: &Store,
    profile_id: &str,
    fingerprint: &str,
) -> WorkbenchResult<HostExecutionPreview> {
    let preview = preview_host_execution(store, profile_id)?;
    if preview.fingerprint != fingerprint {
        return Err(WorkbenchError::conflict(
            "The command or its inputs changed during review; preview it again",
        ));
    }
    open_connection(store)?.execute("INSERT INTO execution_authorizations(profile_id,fingerprint,authorized_at) VALUES(?1,?2,?3) ON CONFLICT(profile_id) DO UPDATE SET fingerprint=excluded.fingerprint,authorized_at=excluded.authorized_at",params![profile_id,fingerprint,now()]).map_err(|e|WorkbenchError::storage("Failed to authorize host execution",e))?;
    preview_host_execution(store, profile_id)
}
