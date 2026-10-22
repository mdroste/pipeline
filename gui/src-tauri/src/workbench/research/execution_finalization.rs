//! One atomic, idempotent finalizer for normal completion and journal recovery.
use super::*;

pub(super) fn apply(store: &Store, id: &str, v: &Value) -> WorkbenchResult<()> {
    let current = get_execution(store, id)?;
    if (v.get("profileId").is_some() && v["profileId"].as_str() != current.profile_id.as_deref())
        || v["validation"]["planId"] != current.input_manifest["planId"]
    {
        return Err(WorkbenchError::invalid(
            "Finalization journal does not match the captured execution identity",
        ));
    }
    let mut c = open_connection(store)?;
    let tx = c
        .transaction()
        .map_err(|e| WorkbenchError::storage("Begin execution finalization", e))?;
    if v["testOnly"] == true {
        let status = if v["outcome"] == "completed" {
            "passed"
        } else {
            "failed"
        };
        if let Some(plan_id) = current.input_manifest["planId"].as_str() {
            let changed = tx.execute("UPDATE execution_plan_state SET test_status=?2 WHERE plan_id=?1 AND fingerprint=?3 AND EXISTS(SELECT 1 FROM desk_records WHERE id=?1 AND workspace_id=?4 AND kind='execution_plan')", params![plan_id,status,current.input_manifest["planFingerprint"].as_str(),current.workspace_id])
                .map_err(|e| WorkbenchError::storage("Record captured plan test", e))?;
            if changed != 1 {
                return Err(WorkbenchError::invalid(
                    "Captured plan identity is unavailable; test result was not applied",
                ));
            }
        } else {
            tx.execute("UPDATE execution_profiles SET tested_at=?2,test_status=?3 WHERE id=?1 AND revision=?4 AND workspace_id=?5",params![v["profileId"].as_str(),v["endedAt"].as_str(),status,v["profileRevision"].as_i64(),current.workspace_id])
                .map_err(|e|WorkbenchError::storage("Record profile test",e))?;
        }
    }
    tx.execute("UPDATE research_executions SET outcome=?2,ended_at=?3,exit_status=?4,stdout_text=?5,stderr_text=?6,output_manifest_json=?7,validation_json=?8 WHERE id=?1",params![id,v["outcome"].as_str(),v["endedAt"].as_str(),v["exitStatus"].as_i64(),v["stdout"].as_str(),v["stderr"].as_str(),v["outputs"].to_string(),v["validation"].to_string()])
        .map_err(|e|WorkbenchError::storage("Finalize execution receipt",e))?;
    tx.commit()
        .map_err(|e| WorkbenchError::storage("Commit execution finalization", e))
}
