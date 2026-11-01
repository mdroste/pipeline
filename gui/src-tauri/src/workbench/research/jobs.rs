//! Workspace-owned local queue. No database gate or worker spans a process wait.
//!
//! Jobs in one project run one at a time, in the order they were queued, so two
//! commands never race on its files. Nothing else in the project waits for them:
//! a write is refused only if it touches a queued or running job's declared
//! inputs, outputs, or working copy, and the refusal names the job.
//!
//! A job is owned by its turn (it stops with the turn) or detached (stopped from
//! the job view). Each job records the process instance that started it. A queued
//! or running job from another instance becomes `outcome_unknown` and never
//! restarts. Reconciling applies a completed finalization journal only after
//! re-hashing every adopted blob. Shutdown cancels jobs and waits long enough for
//! a Stata launcher's 30-second cooperative stop.
use super::execution::*;
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{oneshot, Semaphore};

static LIMIT: Semaphore = Semaphore::const_new(2);
static STOPPING: AtomicBool = AtomicBool::new(false);
static INSTANCE: OnceLock<String> = OnceLock::new();
static PENDING: OnceLock<Mutex<Vec<PreparedExecution>>> = OnceLock::new();
static ACTIVE: OnceLock<Mutex<HashMap<String, ActiveJob>>> = OnceLock::new();
/// The last job launched in each project; it signals by dropping its sender.
type Turns = HashMap<(PathBuf, String), oneshot::Receiver<()>>;
static TURNS: OnceLock<Mutex<Turns>> = OnceLock::new();
static LOGS: OnceLock<Mutex<HashMap<String, JobLog>>> = OnceLock::new();
pub(crate) fn process_instance() -> &'static str {
    INSTANCE.get_or_init(|| new_id("instance").expect("OS randomness"))
}
fn pending() -> &'static Mutex<Vec<PreparedExecution>> {
    PENDING.get_or_init(Mutex::default)
}
fn active() -> &'static Mutex<HashMap<String, ActiveJob>> {
    ACTIVE.get_or_init(Mutex::default)
}
/// What a queued or running job has claimed in its project.
#[derive(Clone)]
struct ActiveJob {
    workspace_id: String,
    name: String,
    cwd: PathBuf,
    declared: Vec<PathBuf>,
}
impl ActiveJob {
    fn new(profile: &ExecutionProfile) -> Self {
        let cwd = resolved(Path::new(&profile.cwd));
        Self {
            workspace_id: profile.workspace_id.clone(),
            name: profile.name.clone(),
            declared: profile
                .inputs
                .iter()
                .chain(&profile.outputs)
                .map(|path| folded(&cwd.join(path)))
                .collect(),
            cwd: folded(&cwd),
        }
    }
    fn claims(&self, target: &Path) -> bool {
        self.declared
            .iter()
            .any(|path| path.starts_with(target) || target.starts_with(path))
    }
}
fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
/// Most Mac and Windows volumes ignore case. On the others a false match only
/// asks a researcher to wait for the job.
fn folded(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}
fn jobs_in(ws: &str) -> Vec<ActiveJob> {
    active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|job| job.workspace_id == ws)
        .cloned()
        .collect()
}
fn in_use(job: &ActiveJob, what: &str) -> WorkbenchError {
    WorkbenchError::conflict(format!(
        "The local job \"{}\" is using {what}. Wait for it to finish or stop it, then try again.",
        job.name
    ))
}
/// Refuse a write under `root` to a file that a queued or running job declares.
pub(crate) fn ensure_paths_free<'a>(
    ws: &str,
    root: &Path,
    paths: impl IntoIterator<Item = &'a str>,
) -> WorkbenchResult<()> {
    let jobs = jobs_in(ws);
    if jobs.is_empty() {
        return Ok(());
    }
    let root = resolved(root);
    for path in paths {
        let target = folded(&root.join(path));
        if let Some(job) = jobs.iter().find(|job| job.claims(&target)) {
            return Err(in_use(job, path));
        }
    }
    Ok(())
}
/// Refuse to read or change a whole working copy while a job works inside it.
pub(crate) fn ensure_copy_free(ws: &str, copy: &Path) -> WorkbenchResult<()> {
    let target = folded(&resolved(copy));
    match jobs_in(ws)
        .iter()
        .find(|job| job.cwd.starts_with(&target) || job.claims(&target))
    {
        Some(job) => Err(in_use(job, "this working copy")),
        None => Ok(()),
    }
}
/// Refuse a change that could reach any file in the project, such as a Git branch
/// switch, while a job is queued or running there.
pub(crate) fn ensure_project_free(ws: &str) -> WorkbenchResult<()> {
    match jobs_in(ws).first() {
        Some(job) => Err(in_use(job, "this project's files")),
        None => Ok(()),
    }
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
        .any(|job| job.workspace_id == ws)
}
pub(crate) fn has_active() -> bool {
    !active()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty()
}

// Lets a test look at the project in the moment before the queue lets go of it.
#[cfg(test)]
thread_local! {
    pub(crate) static BEFORE_LOCK_RELEASE: std::cell::Cell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::Cell::new(None) };
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
        PreparedOrExisting::Prepared(p, guard) => {
            open_connection(store)?.execute("INSERT INTO execution_jobs(execution_id,operation_id,request_hash,owner,process_instance,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![p.execution_id,request.operation_id,fingerprint,owner,process_instance(),now()]).map_err(|e|WorkbenchError::storage("Failed to record job ownership",e))?;
            let receipt = get_execution(store, &p.execution_id)?;
            // Nothing after this point can fail, so a listed job always reaches
            // launch_pending, and a refused one was never listed.
            active()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(p.execution_id.clone(), ActiveJob::new(&p.profile));
            pending().lock().unwrap_or_else(|e| e.into_inner()).push(*p);
            #[cfg(test)]
            BEFORE_LOCK_RELEASE.with(|probe| {
                if let Some(check) = probe.take() {
                    check();
                }
            });
            // Writers take this lock and then look for jobs, so they see this one.
            drop(guard);
            Ok(receipt)
        }
    }
}

/// Takes this job's place in its project's line: the receiver, if any, ends
/// when the job before it does.
fn join_line(p: &PreparedExecution) -> (oneshot::Sender<()>, Option<oneshot::Receiver<()>>) {
    let (done, finished) = oneshot::channel();
    let mut turns = TURNS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    turns.retain(|_, tail| matches!(tail.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
    let key = (
        p.store.root_path().to_path_buf(),
        p.profile.workspace_id.clone(),
    );
    (done, turns.insert(key, finished))
}
fn stop_requested(id: &str) -> bool {
    cancelled_executions()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(id)
}
/// Waits for the job ahead, or until this one is stopped. A stopped job records
/// its receipt at once but keeps its place, so the job behind it still waits
/// for the one that is running.
async fn wait_for_turn(id: &str, ahead: &mut Option<oneshot::Receiver<()>>) {
    while let Some(finished) = ahead.as_mut() {
        tokio::select! {
            _ = finished => *ahead = None,
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                if stop_requested(id) {
                    return;
                }
            }
        }
    }
}

pub(crate) fn launch_pending() {
    let work = std::mem::take(&mut *pending().lock().unwrap_or_else(|e| e.into_inner()));
    for mut p in work {
        let id = p.execution_id.clone();
        let (turn, mut ahead) = join_line(&p);
        tokio::spawn(async move {
            wait_for_turn(&id, &mut ahead).await;
            let slot = LIMIT.acquire().await.expect("local job queue stays open");
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
                    p.record_output_times();
                    let output = wait_execution(&p);
                    (p, output)
                })
                .await
                .map_err(|e| {
                    WorkbenchError::worker(format!("Job worker ended unexpectedly: {e}"), false)
                })?;
                super::super::commands::run_store_owned(finalize_store, move |store| {
                    finalize_execution(&store, p, output)
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
            drop(slot);
            if let Some(finished) = ahead {
                let _ = finished.await;
            }
            drop(turn);
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
#[derive(Debug, Serialize)]
pub struct ExecutionOutput {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}
/// The saved output of one run, which execution lists leave out.
pub fn execution_output(store: &Store, ws: &str, id: &str) -> WorkbenchResult<ExecutionOutput> {
    let receipt = get_execution(store, id)?;
    if receipt.workspace_id != ws {
        return Err(WorkbenchError::invalid(
            "Job log is not available in this project",
        ));
    }
    Ok(ExecutionOutput {
        stdout: receipt.stdout,
        stderr: receipt.stderr,
    })
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
                || !hash_regular_file(&reference).is_ok_and(|(hash, _)| hash == digest)
            {
                return Err(WorkbenchError::invalid(
                    "Adopted output failed integrity validation; outcome remains unknown",
                ));
            }
        }
    }
    super::execution_finalization::apply(store, id, &v)?;
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
