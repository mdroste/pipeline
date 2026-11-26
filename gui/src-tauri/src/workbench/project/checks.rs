//! One narrow local check using saved project inputs and immutable receipts.
//! No model, network, shell, or general-purpose scheduler is involved.
use super::*;
use crate::workbench::desk;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckPolicy {
    pub enabled: bool,
    pub source_path: String,
    pub bibliographies: Vec<String>,
}

pub fn save_policy(
    store: &Store,
    ws: &str,
    expected: i64,
    policy: CheckPolicy,
) -> WorkbenchResult<ProjectRecord> {
    let _guard = lock(store, ws)?;
    files::relative(&policy.source_path)?;
    if ignored(&policy.source_path, &[])
        || !policy.source_path.to_lowercase().ends_with(".tex")
        || policy.bibliographies.is_empty()
        || policy.bibliographies.len() > 32
    {
        return Err(WorkbenchError::invalid(
            "Choose a TeX source outside generated folders and 1–32 bibliography files",
        ));
    }
    for path in &policy.bibliographies {
        files::relative(path)?;
        if ignored(path, &[]) || !(path.ends_with(".bib") || path.ends_with(".json")) {
            return Err(WorkbenchError::invalid(
                "Choose BibTeX or CSL JSON outside generated folders",
            ));
        }
    }
    put(
        store,
        ws,
        &format!("citation_policy_{ws}"),
        "check_policy",
        expected,
        &policy,
    )
}

pub fn run(
    store: &Store,
    ws: &str,
    changed: Option<&[String]>,
) -> WorkbenchResult<Option<ProjectRecord>> {
    let _guard = lock(store, ws)?;
    let Some(policy_record) = records(store, ws, "check_policy")?.into_iter().next() else {
        return Ok(None);
    };
    let policy: CheckPolicy = decode(&policy_record)?;
    if let Some(paths) = changed {
        if !policy.enabled
            || !paths
                .iter()
                .any(|path| path == &policy.source_path || policy.bibliographies.contains(path))
        {
            return Ok(None);
        }
    }
    let result = citations::inspect(
        store,
        ws,
        citations::CitationRequest {
            bibliographies: policy.bibliographies.clone(),
            source_path: Some(policy.source_path.clone()),
            source_text: None,
            checkpoint_id: None,
        },
    );
    let details = match result {
        Ok(index) => {
            json!({"versions":index["versions"],"sourceHash":index["sourceHash"],"missing":index["missing"],"warnings":index["warnings"],"coverage":index["coverage"]})
        }
        Err(error) => {
            json!({"error":error.message,"coverage":"Inputs could not all be read; no validation result is claimed."})
        }
    };
    let signature = hash(
        json!([policy_record.revision, details])
            .to_string()
            .as_bytes(),
    );
    let old = records(store, ws, "check_state")?.into_iter().next();
    if old
        .as_ref()
        .is_some_and(|old| old.body["signature"] == signature)
    {
        return Ok(old);
    }
    let failed = details["error"].is_string()
        || details["missing"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        || details["warnings"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
    let body = json!({"policyId":policy_record.id,"policyRevision":policy_record.revision,"sourcePath":policy.source_path,"tool":"Pipeline citation parser v1","trigger":if changed.is_some(){"selected source saved in Pipeline"}else{"manual"},"signature":signature,"outcome":if failed{"needs_attention"}else{"passed"},"details":details,"checkedAt":now()});
    // Immutable evidence is retained once for an input signature. A later return
    // to those bytes reuses the receipt and updates only the latest projection.
    let operation = format!("citation-check-{ws}-{signature}");
    let receipt = match desk::operation_record(store, ws, &operation)? {
        Some(receipt) => receipt,
        None => desk::insert(
            store,
            ws,
            "check_receipt",
            "Citation validation",
            body.clone(),
            None,
            &operation,
        )?,
    };
    let mut latest = body;
    latest["receiptId"] = json!(receipt.id);
    latest["receiptHash"] = json!(receipt.content_hash);
    Ok(Some(put(
        store,
        ws,
        &format!("citation_state_{ws}"),
        "check_state",
        old.map_or(0, |old| old.revision),
        &latest,
    )?))
}
