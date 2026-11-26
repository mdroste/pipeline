//! Workspace task delivery and pending prompts for the Automations adapter.

use super::{run_store, turn_queue};
use serde_json::Value;

pub(crate) async fn deliver_task(
    binding: crate::workbench::tasks::TaskBinding,
    operation: String,
    value: Value,
    files: Vec<crate::workbench::tasks::delivery::DeliveryFile>,
) -> Result<Value, String> {
    let _permit = turn_queue()
        .clone()
        .try_acquire_owned()
        .map_err(|_| "task_busy: Workspace is running another turn".to_string())?;
    run_store(move |store| {
        crate::workbench::tasks::validate_context(&store, &binding)?;
        let mut result = crate::workbench::tasks::delivery::deliver(
            &store,
            &binding.session_id,
            &operation,
            value,
            files,
        )?;
        let next = crate::workbench::tasks::binding(&store, &binding.session_id)?;
        result["harnessFingerprint"] = serde_json::json!(next.harness_fingerprint);
        result["cursor"] = serde_json::json!(next.cursor);
        Ok(result)
    })
    .await
    .map_err(|e| e.message)
}
