//! Tools owned by the Workspace research service.

use super::*;
use crate::workbench::{data, project, search, tasks};

pub(super) fn provider_identifier(label: &str, value: &str) -> WorkbenchResult<()> {
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(WorkbenchError::invalid(format!("Invalid {label}")));
    }
    Ok(())
}

fn required_argument<'a>(arguments: &'a Value, key: &str) -> WorkbenchResult<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid(format!("Tool argument {key} must be a string")))
}

pub(super) fn tool_response(success: bool, value: &Value) -> Value {
    let text = serde_json::to_string(value)
        .unwrap_or_else(|_| "{\"error\":\"Tool result could not be encoded\"}".to_string());
    json!({"success":success,"contentItems":[{"type":"inputText","text":text}]})
}

pub fn handle_dynamic_tool_call(store: &Store, params: &Value) -> WorkbenchResult<Value> {
    let thread_id = params
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid("Dynamic tool call omitted threadId"))?;
    let turn_id = params
        .get("turnId")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid("Dynamic tool call omitted turnId"))?;
    let call_id = params
        .get("callId")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid("Dynamic tool call omitted callId"))?;
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .ok_or_else(|| WorkbenchError::invalid("Dynamic tool call omitted tool"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    for (label, value) in [
        ("thread id", thread_id),
        ("turn id", turn_id),
        ("tool call id", call_id),
    ] {
        provider_identifier(label, value)?;
    }
    validate_id("dynamic tool name", tool)?;
    let encoded_arguments = serde_json::to_string(&arguments).map_err(|error| {
        WorkbenchError::storage("Failed to encode dynamic tool arguments", error)
    })?;
    if !arguments.is_object() || encoded_arguments.len() > 256 * 1024 {
        return Err(WorkbenchError::invalid(
            "Dynamic tool arguments must be an object no larger than 256 KiB",
        ));
    }
    let connection = open_connection(store)?;
    let identity: (String, String, String) = connection.query_row("SELECT b.id, b.session_id, s.workspace_id FROM session_bindings b JOIN sessions s ON s.id = b.session_id WHERE b.provider_thread_id = ?1 AND b.retired_at IS NULL AND s.workspace_id IS NOT NULL", [thread_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional().map_err(|error| WorkbenchError::storage("Failed to resolve dynamic tool caller", error))?.ok_or_else(|| WorkbenchError::invalid("Dynamic tool caller is not bound to an active Workspace conversation"))?;
    let (binding_id, session_id, workspace_id) = identity;
    let effective = resolve_harness(store, &session_id)?;
    if !effective
        .dynamic_tools
        .iter()
        .any(|spec| spec.get("name").and_then(Value::as_str) == Some(tool))
    {
        return Err(WorkbenchError::invalid(
            "Dynamic tool is not enabled by this conversation's immutable harness",
        ));
    }
    let existing: Option<(String, Option<String>, Option<String>)> = connection.query_row("SELECT state, result_json, error_json FROM tool_receipts WHERE binding_id = ?1 AND provider_turn_id = ?2 AND call_id = ?3", params![binding_id, turn_id, call_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional().map_err(|error| WorkbenchError::storage("Failed to inspect dynamic tool receipt", error))?;
    if let Some((state, result, error)) = existing {
        return match (state.as_str(), result, error) {
            ("completed", Some(result), _) => serde_json::from_str(&result).map_err(|error| {
                WorkbenchError::storage("Failed to decode recorded tool result", error)
            }),
            ("failed", _, Some(error)) => Ok(tool_response(
                false,
                &serde_json::from_str(&error)
                    .unwrap_or_else(|_| json!({"error":"Recorded tool failure"})),
            )),
            _ => Ok(tool_response(
                false,
                &json!({"error":"This tool call is already in progress or has an unknown outcome; it was not repeated."}),
            )),
        };
    }
    let calls_this_turn: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM tool_receipts WHERE binding_id=?1 AND provider_turn_id=?2",
            params![binding_id, turn_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            WorkbenchError::storage("Failed to enforce dynamic tool call budget", error)
        })?;
    if calls_this_turn >= 64 {
        return Err(WorkbenchError::invalid(
            "This turn exhausted its 64-call Workspace tool budget",
        ));
    }
    let receipt_id = new_id("tool")?;
    connection.execute("INSERT INTO tool_receipts (id, workspace_id, session_id, binding_id, provider_turn_id, call_id, tool_name, catalog_version, arguments_json, state, started_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'running', ?10)", params![receipt_id, workspace_id, session_id, binding_id, turn_id, call_id, tool, TOOL_CATALOG_VERSION, encoded_arguments, now()]).map_err(|error| WorkbenchError::storage("Failed to start dynamic tool receipt", error))?;

    let result = (|| -> WorkbenchResult<Value> {
        match tool {
            "workbench_task_propose" => tasks::propose(
                store,
                &session_id,
                &format!("{turn_id}-{call_id}"),
                &arguments,
            ),
            "workbench_task_catalog" => {
                crate::orchestration::definition::catalog().map_err(WorkbenchError::invalid)
            }
            "workbench_research_records" => {
                let records = project::studio_records(
                    store,
                    &workspace_id,
                    required_argument(&arguments, "kind")?,
                )?;
                let mut selected = Vec::new();
                let mut bytes = 0usize;
                let total = records.len();
                for r in records {
                    let encoded = serde_json::to_value(r).map_err(|e| {
                        WorkbenchError::storage("Failed to encode research record", e)
                    })?;
                    let size = encoded.to_string().len();
                    if selected.len() >= 20 || bytes + size > 64 * 1024 {
                        break;
                    }
                    bytes += size;
                    selected.push(encoded);
                }
                Ok(
                    json!({"records":selected,"truncated":selected.len()<total,"limit":"20 records / 64 KiB; open the project inspector for remaining records","authority":"Record provenance and explicit researcher decisions remain separate from scientific verification"}),
                )
            }
            "workbench_dataset_rows_v1" => Ok(serde_json::to_value(data::preview_rows(
                store,
                &workspace_id,
                required_argument(&arguments, "datasetId")?,
                arguments["start"].as_u64().unwrap_or(0) as usize,
                true,
            )?)
            .map_err(|e| WorkbenchError::storage("Row preview", e))?),
            "workbench_research_search_v1" => {
                search::advance(store, &workspace_id)?;
                Ok(serde_json::to_value(search::search(
                    store,
                    search::SearchRequest {
                        workspace_id: workspace_id.clone(),
                        query: required_argument(&arguments, "query")?.into(),
                        kind: arguments["kind"].as_str().map(str::to_string),
                        cursor: arguments["cursor"].as_str().map(str::to_string),
                        limit: Some(10),
                    },
                )?)
                .map_err(|e| WorkbenchError::storage("Search response", e))?)
            }
            "workbench_research_object_v1" => {
                let object = serde_json::from_value(arguments["object"].clone())
                    .map_err(|e| WorkbenchError::storage("Research reference", e))?;
                Ok(serde_json::to_value(search::read_object(
                    store,
                    &workspace_id,
                    &object,
                    16 * 1024,
                )?)
                .map_err(|e| WorkbenchError::storage("Research object", e))?)
            }
            "workbench_project_context" => {
                Ok(json!({"context":project::project_context(store,&workspace_id)?}))
            }
            "workbench_anchor_read" => serde_json::to_value(project::anchor_read(
                store,
                &workspace_id,
                required_argument(&arguments, "anchorId")?,
            )?)
            .map_err(|e| WorkbenchError::storage("Failed to encode selection", e)),
            "workbench_paper_read" => {
                let value = paper_read(
                    store,
                    PaperReadRequest {
                        workspace_id: workspace_id.clone(),
                        revision_id: required_argument(&arguments, "revisionId")?.to_string(),
                        start: arguments
                            .get("start")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize),
                        length: arguments
                            .get("length")
                            .and_then(Value::as_u64)
                            .map(|value| value as usize),
                    },
                )?;
                Ok(serde_json::to_value(value).map_err(|error| {
                    WorkbenchError::storage("Failed to encode paper read result", error)
                })?)
            }
            "workbench_paper_search" => {
                let value = paper_search(
                    store,
                    &workspace_id,
                    required_argument(&arguments, "revisionId")?,
                    required_argument(&arguments, "query")?,
                    arguments.get("limit").and_then(Value::as_u64).unwrap_or(10) as usize,
                )?;
                Ok(serde_json::to_value(value).map_err(|error| {
                    WorkbenchError::storage("Failed to encode paper search result", error)
                })?)
            }
            "workbench_paper_page" => paper_page_image(
                store,
                &workspace_id,
                required_argument(&arguments, "revisionId")?,
                arguments.get("page").and_then(Value::as_u64).unwrap_or(0) as u32,
            ),
            "workbench_source_read" => {
                let value = source_read(
                    store,
                    &workspace_id,
                    required_argument(&arguments, "sourceVersionId")?,
                    arguments.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
                    arguments
                        .get("length")
                        .and_then(Value::as_u64)
                        .unwrap_or(32 * 1024) as usize,
                )?;
                Ok(serde_json::to_value(value).map_err(|error| {
                    WorkbenchError::storage("Failed to encode source read result", error)
                })?)
            }
            "workbench_note_propose" => {
                let note = create_note(
                    store,
                    CreateNoteRequest {
                        workspace_id: workspace_id.clone(),
                        paper_id: None,
                        kind: required_argument(&arguments, "kind")?.to_string(),
                        body: required_argument(&arguments, "body")?.to_string(),
                        state: Some("proposed".to_string()),
                        origin: format!("model:{session_id}:{turn_id}"),
                        pinned: false,
                        operation_id: format!("tool-note-{receipt_id}"),
                    },
                    true,
                )?;
                Ok(serde_json::to_value(note).map_err(|error| {
                    WorkbenchError::storage("Failed to encode note proposal", error)
                })?)
            }
            "workbench_claim_propose" => {
                let claim = propose_claim(
                    store,
                    ProposeClaimRequest {
                        workspace_id: workspace_id.clone(),
                        paper_id: None,
                        claim: required_argument(&arguments, "claim")?.to_string(),
                        kind: required_argument(&arguments, "kind")?.to_string(),
                        origin: format!("model:{session_id}:{turn_id}"),
                        operation_id: format!("tool-claim-{receipt_id}"),
                    },
                    true,
                )?;
                Ok(serde_json::to_value(claim).map_err(|error| {
                    WorkbenchError::storage("Failed to encode claim proposal", error)
                })?)
            }
            "workbench_evidence_propose" => {
                let evidence = propose_evidence(
                    store,
                    ProposeEvidenceRequest {
                        workspace_id: workspace_id.clone(),
                        claim_version_id: required_argument(&arguments, "claimVersionId")?
                            .to_string(),
                        target_type: required_argument(&arguments, "targetType")?.to_string(),
                        target_id: required_argument(&arguments, "targetId")?.to_string(),
                        locator: arguments.get("locator").cloned(),
                        relation: required_argument(&arguments, "relation")?.to_string(),
                        assessor: format!("model:{session_id}:{turn_id}"),
                        operation_id: format!("tool-evidence-{receipt_id}"),
                    },
                    true,
                )?;
                Ok(serde_json::to_value(evidence).map_err(|error| {
                    WorkbenchError::storage("Failed to encode evidence proposal", error)
                })?)
            }
            "workbench_source_propose" => {
                let title = required_argument(&arguments, "title")?;
                let body = format!(
                    "Proposed source: {title}\nCitation key: {}\nLocator: {}",
                    arguments
                        .get("citationKey")
                        .and_then(Value::as_str)
                        .unwrap_or("not supplied"),
                    arguments
                        .get("locator")
                        .and_then(Value::as_str)
                        .unwrap_or("not supplied")
                );
                let note = create_note(
                    store,
                    CreateNoteRequest {
                        workspace_id: workspace_id.clone(),
                        paper_id: None,
                        kind: "question".into(),
                        body,
                        state: Some("proposed".into()),
                        origin: format!("model:{session_id}:{turn_id}"),
                        pinned: false,
                        operation_id: format!("tool-source-{receipt_id}"),
                    },
                    true,
                )?;
                Ok(json!({"proposalType":"source","note":note,"registered":false}))
            }
            "workbench_run_lookup" => {
                let execution =
                    execution::get_execution(store, required_argument(&arguments, "executionId")?)?;
                if execution.workspace_id != workspace_id {
                    return Err(WorkbenchError::invalid(
                        "Execution receipt belongs to another workspace",
                    ));
                }
                Ok(serde_json::to_value(execution).map_err(|error| {
                    WorkbenchError::storage("Failed to encode execution receipt", error)
                })?)
            }
            "workbench_research_run" => {
                let profile_id = required_argument(&arguments, "profileId")?.to_string();
                let profile = execution::get_execution_profile(store, &profile_id)?;
                if profile.workspace_id != workspace_id
                    || profile.test_status.as_deref() != Some("passed")
                {
                    return Err(WorkbenchError::invalid(
                        "The execution profile is not tested and enabled in this workspace",
                    ));
                }
                let execution = jobs::queue(
                    store,
                    RunExecutionRequest {
                        plan_id: None,
                        profile_id,
                        session_id: Some(session_id.clone()),
                        test_only: false,
                        operation_id: format!("tool-run-{receipt_id}"),
                    },
                    "turn",
                    Some((&binding_id, turn_id, call_id)),
                )?;
                Ok(serde_json::to_value(execution).map_err(|error| {
                    WorkbenchError::storage("Failed to encode research execution", error)
                })?)
            }
            _ => Err(WorkbenchError::invalid("Unknown dynamic research tool")),
        }
    })();
    let finished = now();
    match result {
        Ok(value) => {
            let response = value
                .get("_contentItems")
                .cloned()
                .map(|content_items| json!({"success":true,"contentItems":content_items}))
                .unwrap_or_else(|| tool_response(true, &value));
            let encoded = serde_json::to_string(&response).map_err(|error| {
                WorkbenchError::storage("Failed to encode dynamic tool response", error)
            })?;
            open_connection(store)?.execute("UPDATE tool_receipts SET state = 'completed', result_json = ?2, ended_at = ?3 WHERE id = ?1 AND state = 'running'", params![receipt_id, encoded, finished]).map_err(|error| WorkbenchError::storage("Failed to complete dynamic tool receipt", error))?;
            Ok(response)
        }
        Err(error) => {
            let body =
                json!({"code":error.code,"message":error.message,"retryable":error.retryable});
            let response = tool_response(false, &body);
            open_connection(store)?.execute("UPDATE tool_receipts SET state = 'failed', error_json = ?2, ended_at = ?3 WHERE id = ?1 AND state = 'running'", params![receipt_id, serde_json::to_string(&body).unwrap_or_default(), finished]).map_err(|storage| WorkbenchError::storage("Failed to record dynamic tool failure", storage))?;
            Ok(response)
        }
    }
}
