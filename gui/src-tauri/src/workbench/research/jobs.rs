//! Workspace-owned local queue. No database gate or worker spans a process wait.
use super::execution::*;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Semaphore;

static LIMIT: Semaphore = Semaphore::const_new(2);
static STOPPING: AtomicBool = AtomicBool::new(false);
static INSTANCE: OnceLock<String> = OnceLock::new();
static PENDING: OnceLock<Mutex<HashMap<String, Box<PreparedExecution>>>> = OnceLock::new();
static ACTIVE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static LOGS: OnceLock<Mutex<HashMap<String, JobLog>>> = OnceLock::new();
pub(crate) fn process_instance() -> &'static str {
    INSTANCE.get_or_init(|| new_id("instance").expect("OS randomness"))
}
fn pending() -> &'static Mutex<HashMap<String, Box<PreparedExecution>>> {
    PENDING.get_or_init(Mutex::default)
}
fn active() -> &'static Mutex<HashMap<String, String>> {
    ACTIVE.get_or_init(Mutex::default)
}
#[derive(Default)]
struct JobLog {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}
pub(crate) fn append_log(id: &str, stream: &str, bytes: &[u8]) {
    let mut logs = LOGS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let log = logs.entry(id.into()).or_default();
    let target = if stream == "stdout" {
        &mut log.stdout
    } else {
        &mut log.stderr
    };
    let take = bytes
        .len()
        .min((4 * 1024 * 1024usize).saturating_sub(target.len()));
    target.extend_from_slice(&bytes[..take]);
    log.truncated |= take < bytes.len();
}
pub(crate) fn execution_active(id: &str) -> bool {
    active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(id)
}
pub(crate) fn workspace_active(ws: &str) -> bool {
    active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .any(|id| id == ws)
}
pub(crate) fn has_active() -> bool {
    !active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty()
}

pub(crate) fn queue(
    store: &Store,
    request: RunExecutionRequest,
    owner: &str,
    origin: Option<(&str, &str, &str)>,
) -> WorkbenchResult<ResearchExecution> {
    if STOPPING.load(Ordering::Acquire) {
        return Err(WorkbenchError::invalid(
            "The application is stopping its local jobs",
        ));
    }
    if !matches!(owner, "detached" | "turn") || (owner == "turn" && origin.is_none()) {
        return Err(WorkbenchError::invalid(
            "Turn-owned jobs require an active research-tool caller",
        ));
    }
    validate_id("operation id", &request.operation_id)?;
    let fingerprint = hash_bytes(
        &serde_json::to_vec(&json!({"request":request,"owner":owner,"origin":origin}))
            .map_err(|e| WorkbenchError::storage("Failed to encode job request", e))?,
    );
    let prior: Option<(String, String)> = open_connection(store)?
        .query_row(
            "SELECT execution_id,request_hash FROM execution_jobs WHERE operation_id=?1",
            [&request.operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| WorkbenchError::storage("Failed to inspect job receipt", e))?;
    if let Some((id, old)) = prior {
        if old != fingerprint {
            return Err(WorkbenchError::conflict(
                "Job operation ID was reused with different arguments",
            ));
        }
        return get_execution(store, &id);
    }
    if active().lock().unwrap_or_else(|e| e.into_inner()).len() >= 16 {
        return Err(WorkbenchError::invalid("The local queue is full (16 jobs)"));
    }
    match prepare_execution(store, request.clone(), origin)? {
        PreparedOrExisting::Existing(e) => Ok(*e),
        PreparedOrExisting::Prepared(p) => {
            open_connection(store)?.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![p.execution_id,request.operation_id,fingerprint,owner,process_instance(),now()]).map_err(|e|WorkbenchError::storage("Failed to record job ownership",e))?;
            let receipt = get_execution(store, &p.execution_id)?;
            active()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(p.execution_id.clone(), p.profile.workspace_id.clone());
            pending()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(p.execution_id.clone(), p);
            Ok(receipt)
        }
    }
}

pub(crate) fn launch_pending() {
    let work = std::mem::take(&mut *pending().lock().unwrap_or_else(|e| e.into_inner()));
    for (id, p) in work {
        tokio::spawn(async move {
            let _slot = LIMIT.acquire().await.expect("local job queue stays open");
            let start_id = id.clone();
            let job_store = p.store.clone();
            let start_store = job_store.clone();
            let finalize_store = job_store.clone();
            let result = async {
                super::super::commands::run_store_owned(start_store, move |store| {
                    start_execution(&store, &start_id)
                })
                .await?;
                let (p, output) = tokio::task::spawn_blocking(move || {
                    let output = wait_execution(&p);
                    (p, output)
                })
                .await
                .map_err(|e| {
                    WorkbenchError::worker(format!("Job worker ended unexpectedly: {e}"), false)
                })?;
                super::super::commands::run_store_owned(finalize_store, move |store| {
                    finalize_execution(&store, *p, output)
                })
                .await
            }
            .await;
            if let Err(error) = result {
                let failed_id = id.clone();
                let _=super::super::commands::run_store_owned(job_store,move |store| {
                    open_connection(&store)?.execute("UPDATE research_executions SET outcome='outcome_unknown',ended_at=?2,validation_json=?3 WHERE id=?1 AND outcome IN ('queued','running')",params![failed_id,now(),json!({"finalizationError":error.message,"recovery":"Inspect the job and reconcile adopted outputs; the command will not be rerun."}).to_string()]).map_err(|e|WorkbenchError::storage("Failed to record job failure",e))?; Ok(())
                }).await;
            }
            active()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            cancelled_executions()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            LOGS.get_or_init(Mutex::default)
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
        });
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub execution: ResearchExecution,
    pub state: String,
    pub owner: String,
    pub duration_seconds: Option<i64>,
    pub can_reconcile: bool,
    pub resource: String,
}
fn reconcile_abandoned(store: &Store) -> WorkbenchResult<()> {
    open_connection(store)?.execute("UPDATE research_executions SET outcome='outcome_unknown',validation_json=json_set(validation_json,'$.recovery','The application ended before a terminal receipt. Inspect retained evidence; no automatic restart.','$.stataCleanupRequired',CASE WHEN adapter='stata' THEN json('true') ELSE json('false') END) WHERE outcome IN ('queued','running') AND (id IN (SELECT execution_id FROM execution_jobs WHERE process_instance<>?1) OR coalesce(json_extract(validation_json,'$.processInstance'),'')<>?1)",[process_instance()]).map_err(|e|WorkbenchError::storage("Failed to reconcile interrupted jobs",e))?;
    Ok(())
}
pub fn list_jobs(store: &Store, ws: &str) -> WorkbenchResult<Vec<JobStatus>> {
    reconcile_abandoned(store)?;
    let conn = open_connection(store)?;
    let mut result = Vec::new();
    for execution in list_executions(store, ws)? {
        let meta:Option<(String,bool,bool)>=conn.query_row("SELECT owner,cancel_requested,finalization_json IS NOT NULL FROM execution_jobs WHERE execution_id=?1",[&execution.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|WorkbenchError::storage("Failed to read job ownership",e))?;
        // A crash between receipt creation and ownership registration must
        // remain visible, including receipts imported from an older store.
        let (owner, cancelling, finalized) =
            meta.unwrap_or_else(|| ("unknown".into(), false, false));
        let state = match execution.outcome.as_str() {
            "queued" | "running" if cancelling => "cancelling",
            "interrupted" => "cancelled",
            "outcome_unknown" => "unknown",
            other => other,
        }
        .to_string();
        let duration_seconds = execution
            .started_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|start| {
                execution
                    .ended_at
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|end| (end - start).num_seconds())
                    .unwrap_or_else(|| {
                        chrono::Utc::now()
                            .signed_duration_since(start)
                            .num_seconds()
                    })
                    .max(0)
            });
        result.push(JobStatus {
            can_reconcile: finalized && state == "unknown",
            resource: execution.cwd.clone(),
            execution,
            state,
            owner,
            duration_seconds,
        });
    }
    Ok(result)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLogPage {
    pub text: String,
    pub next_offset: usize,
    pub truncated: bool,
}
pub fn log_page(
    store: &Store,
    ws: &str,
    id: &str,
    stream: &str,
    offset: usize,
) -> WorkbenchResult<JobLogPage> {
    let receipt = get_execution(store, id)?;
    if receipt.workspace_id != ws || !matches!(stream, "stdout" | "stderr") {
        return Err(WorkbenchError::invalid(
            "Job log is not available in this project",
        ));
    }
    let logs = LOGS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let (bytes, truncated) = if let Some(log) = logs.get(id) {
        (
            if stream == "stdout" {
                log.stdout.as_slice()
            } else {
                log.stderr.as_slice()
            },
            log.truncated,
        )
    } else {
        (
            if stream == "stdout" {
                receipt.stdout.as_deref()
            } else {
                receipt.stderr.as_deref()
            }
            .unwrap_or("")
            .as_bytes(),
            false,
        )
    };
    let start = offset.min(bytes.len());
    let end = (start + 64 * 1024).min(bytes.len());
    Ok(JobLogPage {
        text: String::from_utf8_lossy(&bytes[start..end]).into_owned(),
        next_offset: end,
        truncated,
    })
}
pub fn reconcile_job(store: &Store, ws: &str, id: &str) -> WorkbenchResult<ResearchExecution> {
    let current = get_execution(store, id)?;
    if current.workspace_id != ws
        || current.outcome != "outcome_unknown"
        || active()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    {
        return Err(WorkbenchError::invalid(
            "Only an inactive unknown job in this project can be reconciled",
        ));
    }
    let body: Option<String> = open_connection(store)?
        .query_row(
            "SELECT finalization_json FROM execution_jobs WHERE execution_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| WorkbenchError::storage("Failed to read finalization journal", e))?;
    let v: Value = serde_json::from_str(&body.ok_or_else(|| {
        WorkbenchError::invalid("No completed adoption journal exists; the outcome remains unknown")
    })?)
    .map_err(|e| WorkbenchError::storage("Invalid finalization journal", e))?;
    if let Some(artifacts) = v["outputs"]["artifacts"].as_array() {
        if artifacts.len() > 100 {
            return Err(WorkbenchError::invalid(
                "Adoption journal exceeds its limit",
            ));
        }
        for artifact in artifacts {
            let (reference,digest):(String,String)=open_connection(store)?.query_row("SELECT storage_reference,content_hash FROM artifacts WHERE id=?1 AND workspace_id=?2",params![artifact["artifactId"].as_str(),ws],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|WorkbenchError::storage("Adopted output is unavailable",e))?;
            let reference = fs::canonicalize(reference)
                .map_err(|e| WorkbenchError::storage("Adopted output is unavailable", e))?;
            if !reference.starts_with(
                fs::canonicalize(store.root_path().join("blobs"))
                    .map_err(|e| WorkbenchError::storage("Blob store is unavailable", e))?,
            ) || artifact["contentHash"] != digest
                || hash_file(&reference)?.0 != digest
            {
                return Err(WorkbenchError::invalid(
                    "Adopted output failed integrity validation; outcome remains unknown",
                ));
            }
        }
    }
    open_connection(store)?.execute("UPDATE research_executions SET outcome=?2,ended_at=?3,exit_status=?4,stdout_text=?5,stderr_text=?6,output_manifest_json=?7,validation_json=?8 WHERE id=?1",params![id,v["outcome"].as_str(),v["endedAt"].as_str(),v["exitStatus"].as_i64(),v["stdout"].as_str(),v["stderr"].as_str(),v["outputs"].to_string(),v["validation"].to_string()]).map_err(|e|WorkbenchError::storage("Failed to reconcile adopted outputs",e))?;
    if v["testOnly"] == true {
        open_connection(store)?.execute("UPDATE execution_profiles SET tested_at=?2,test_status=?3 WHERE id=?1 AND revision=?4",params![v["profileId"].as_str(),v["endedAt"].as_str(),if v["outcome"] == "completed" {"passed"} else {"failed"},v["profileRevision"].as_i64()]).map_err(|e|WorkbenchError::storage("Failed to reconcile profile test",e))?;
    }
    get_execution(store, id)
}
pub async fn shutdown() {
    STOPPING.store(true, Ordering::Release);
    let ids = active().lock().unwrap_or_else(|e| e.into_inner()).clone();
    cancelled_executions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .extend(ids.into_keys());
    launch_pending();
    let start = Instant::now();
    while has_active() && start.elapsed() < Duration::from_secs(35) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

pub(crate) fn complete_tool_result(store: &Store, execution_id: &str) -> WorkbenchResult<Value> {
    reconcile_abandoned(store)?;
    let e = get_execution(store, execution_id)?;
    let response = tool_response(
        true,
        &serde_json::to_value(&e)
            .map_err(|e| WorkbenchError::storage("Failed to encode execution", e))?,
    );
    open_connection(store)?.execute("UPDATE tool_receipts SET result_json=?2,ended_at=?3 WHERE (binding_id,provider_turn_id,call_id) IN (SELECT binding_id,provider_turn_id,tool_call_id FROM research_executions WHERE id=?1) AND state='completed'",params![execution_id,response.to_string(),now()]).map_err(|e|WorkbenchError::storage("Failed to finish execution tool receipt",e))?;
    Ok(response)
}
