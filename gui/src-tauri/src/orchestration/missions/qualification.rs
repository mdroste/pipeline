//! Deterministic provider seam for native development qualification only.
//! Release binaries do not compile this module. Both disposable store overrides
//! and an explicit script are required; production permissions are never changed.
use super::{storage, store, Phase};
use serde_json::{json, Value};

pub(crate) fn scripted(s: &store::Store, run: &store::TaskRun) -> store::Result<Option<Value>> {
    let Some(path) = std::env::var_os("PIPELINE_E2E_MISSION_SCRIPT") else {
        return Ok(None);
    };
    if std::env::var_os("PIPELINE_WORKBENCH_DEV_ROOT").is_none()
        || std::env::var_os("PIPELINE_E2E_TASK_ROOT").is_none()
    {
        return Err(
            "Scripted mission qualification requires disposable Workspace and task stores".into(),
        );
    }
    let Some(id) = storage::owner(s, &run.id)? else {
        return Ok(None);
    };
    let m = storage::get(s, &id)?;
    let bytes = std::fs::read(path).map_err(store::err)?;
    if bytes.len() > 128 * 1024 {
        return Err("Mission qualification script is too large".into());
    }
    let script: Value = serde_json::from_slice(&bytes).map_err(store::err)?;
    let phase = match m.phase {
        Phase::Plan => "plan",
        Phase::Investigate => "investigate",
        Phase::Challenge => "challenge",
        Phase::Review => return Ok(None),
    };
    let result = script
        .get(phase)
        .ok_or("Scripted mission response is missing")?
        .to_string();
    Ok(Some(
        json!({"state":"completed","text":result,"finalText":result,"qualification":"scripted-native-development"}),
    ))
}
