//! Review run queue.
//!
//! Reviews started from the app enter one queue. Up to
//! `Settings::max_concurrent_reviews` run at once, each in its own run scope
//! holding a shared run slot; the rest wait in order. The queue publishes its
//! state as `runs:queue` and tags each run's `pipeline:*` events with the run's
//! `runKey`, so the window can follow several runs and return to any of them.
//! Runs are owned by the queue, not by the invoking window: closing or
//! reloading the view never cancels them.

use super::*;
use std::sync::{LazyLock, Mutex};

/// Finished entries kept in the list (oldest are dropped first).
const MAX_FINISHED: usize = 20;
/// Finished entries whose full result (report, markdown, extracted text) is
/// kept in memory; older ones open from Review history instead.
const MAX_RESULTS: usize = 6;
/// Delay before retrying a start blocked by an exclusive job or another
/// Pipeline process.
const BLOCKED_RETRY: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QueuedRunStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedRun {
    pub key: String,
    /// "review" or "rerun".
    pub kind: String,
    pub label: String,
    pub input_path: String,
    pub profile_name: String,
    pub status: QueuedRunStatus,
    /// Latest runtime stage kind and label while running.
    pub stage: Option<String>,
    pub stage_label: Option<String>,
    /// Why a queued run cannot start yet beyond waiting its turn.
    pub waiting: Option<String>,
    pub queued_at: u64,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub run_id: Option<String>,
    pub error: Option<String>,
    /// Finished, but one or more steps failed.
    pub partial: bool,
    pub result_available: bool,
}

/// The queue as the window sees it.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunQueueSnapshot {
    /// Reviews that may run at the same time.
    pub limit: usize,
    pub runs: Vec<QueuedRun>,
}

struct ReviewLaunch {
    paper_path: String,
    input_interpretation: Option<String>,
    diff: bool,
    variables: std::collections::HashMap<String, String>,
    extra_inputs: std::collections::HashMap<String, String>,
    snapshot: RunSnapshot,
}

enum Launch {
    Review(Box<ReviewLaunch>),
    Rerun {
        run_id: String,
        from_step: Option<String>,
        only_failed: bool,
    },
}

struct Entry {
    info: QueuedRun,
    launch: Option<Launch>,
    /// Registered while queued or running, so cancellation reaches the run.
    run: Option<crate::run_scope::ActiveScope>,
    result: Option<serde_json::Value>,
}

#[derive(Default)]
struct Queue {
    entries: Vec<Entry>,
    bus: Option<crate::emit::EventBus>,
    retry_pending: bool,
}

static QUEUE: LazyLock<Mutex<Queue>> = LazyLock::new(Mutex::default);
static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn lock_queue() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// A process-unique key for a run scope.
pub(crate) fn next_run_key() -> String {
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("run-{}-{sequence}", now_ms())
}

/// Tags a queued run's `pipeline:*` events with its key and mirrors stage
/// transitions into the queue entry.
struct QueuedRunEvents {
    inner: crate::emit::EventBus,
    key: String,
}

impl crate::emit::Events for QueuedRunEvents {
    fn emit_event(
        &self,
        event: &str,
        mut payload: serde_json::Value,
    ) -> Result<(), crate::emit::EmitError> {
        if event.starts_with("pipeline:") {
            if let serde_json::Value::Object(fields) = &mut payload {
                fields.insert("runKey".into(), self.key.clone().into());
            }
            if event == "pipeline:stage" {
                note_stage(&self.key, &payload);
            }
        }
        self.inner.emit_event(event, payload)
    }
}

fn note_stage(key: &str, payload: &serde_json::Value) {
    let stage = payload.get("stage").and_then(|value| value.as_str());
    let skipped = payload.get("skipped").and_then(|value| value.as_bool()) == Some(true);
    let Some(stage) = stage.filter(|stage| *stage != "done" && !skipped) else {
        return;
    };
    {
        let mut queue = lock_queue();
        let Some(entry) = queue.entries.iter_mut().find(|entry| entry.info.key == key) else {
            return;
        };
        entry.info.stage = Some(stage.to_string());
        entry.info.stage_label = payload
            .get("label")
            .and_then(|value| value.as_str())
            .map(str::to_string);
    }
    publish();
}

fn snapshot(queue: &Queue, limit: usize) -> RunQueueSnapshot {
    RunQueueSnapshot {
        limit,
        runs: queue
            .entries
            .iter()
            .map(|entry| entry.info.clone())
            .collect(),
    }
}

fn publish() {
    // Read settings before taking the queue lock.
    let limit = concurrency_limit();
    let (bus, snapshot) = {
        let queue = lock_queue();
        (queue.bus.clone(), snapshot(&queue, limit))
    };
    if let Some(bus) = bus {
        let _ = bus.emit_event(
            "runs:queue",
            serde_json::to_value(snapshot).unwrap_or_default(),
        );
    }
}

/// Publish a changed concurrency limit and start reviews it now allows.
pub(crate) fn settings_changed() {
    publish();
    pump();
}

fn concurrency_limit() -> usize {
    crate::settings::load_persisted()
        .max_concurrent_reviews
        .clamp(1, 4) as usize
}

fn is_finished(status: &QueuedRunStatus) -> bool {
    matches!(
        status,
        QueuedRunStatus::Done | QueuedRunStatus::Failed | QueuedRunStatus::Cancelled
    )
}

/// Drop the oldest finished entries and results beyond their limits.
fn prune(queue: &mut Queue) {
    let mut finished: Vec<(u64, String)> = queue
        .entries
        .iter()
        .filter(|entry| is_finished(&entry.info.status))
        .map(|entry| {
            (
                entry.info.finished_at.unwrap_or_default(),
                entry.info.key.clone(),
            )
        })
        .collect();
    finished.sort();
    let excess = finished.len().saturating_sub(MAX_FINISHED);
    let dropped: Vec<&String> = finished.iter().take(excess).map(|(_, key)| key).collect();
    queue
        .entries
        .retain(|entry| !dropped.contains(&&entry.info.key));
    let mut with_results: Vec<(u64, usize)> = queue
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.result.is_some())
        .map(|(index, entry)| (entry.info.finished_at.unwrap_or_default(), index))
        .collect();
    with_results.sort();
    let excess = with_results.len().saturating_sub(MAX_RESULTS);
    for (_, index) in with_results.into_iter().take(excess) {
        let entry = &mut queue.entries[index];
        entry.result = None;
        entry.info.result_available = false;
    }
}

fn enqueue(
    bus: crate::emit::EventBus,
    kind: &str,
    label: String,
    input_path: String,
    profile_name: String,
    launch: Launch,
) -> QueuedRun {
    let key = next_run_key();
    let info = QueuedRun {
        key: key.clone(),
        kind: kind.to_string(),
        label,
        input_path,
        profile_name,
        status: QueuedRunStatus::Queued,
        stage: None,
        stage_label: None,
        waiting: None,
        queued_at: now_ms(),
        started_at: None,
        finished_at: None,
        run_id: None,
        error: None,
        partial: false,
        result_available: false,
    };
    {
        let mut queue = lock_queue();
        queue.bus = Some(bus);
        queue.entries.push(Entry {
            info: info.clone(),
            launch: Some(launch),
            run: Some(crate::run_scope::ActiveScope::begin(key)),
            result: None,
        });
    }
    publish();
    pump();
    info
}

fn schedule_retry(queue: &mut Queue) {
    if std::mem::replace(&mut queue.retry_pending, true) {
        return;
    }
    tauri::async_runtime::spawn(async {
        tokio::time::sleep(BLOCKED_RETRY).await;
        lock_queue().retry_pending = false;
        pump();
    });
}

/// Start queued runs while slots are free. Called after every enqueue,
/// completion, cancellation, and concurrency change.
pub(crate) fn pump() {
    let limit = concurrency_limit();
    loop {
        let started = {
            let mut queue = lock_queue();
            let running = queue
                .entries
                .iter()
                .filter(|entry| entry.info.status == QueuedRunStatus::Running)
                .count();
            if running >= limit {
                return;
            }
            let Some(bus) = queue.bus.clone() else {
                return;
            };
            let Some(index) = queue
                .entries
                .iter()
                .position(|entry| entry.info.status == QueuedRunStatus::Queued)
            else {
                return;
            };
            let guard = match acquire_run_slot() {
                Ok(guard) => guard,
                Err(error) => {
                    // An exclusive job (batch, automation, history purge) or
                    // another Pipeline process holds the run lock.
                    let waiting = Some(error);
                    let changed = queue.entries[index].info.waiting != waiting;
                    queue.entries[index].info.waiting = waiting;
                    schedule_retry(&mut queue);
                    drop(queue);
                    if changed {
                        publish();
                    }
                    return;
                }
            };
            let entry = &mut queue.entries[index];
            entry.info.status = QueuedRunStatus::Running;
            entry.info.waiting = None;
            entry.info.started_at = Some(now_ms());
            let key = entry.info.key.clone();
            let scope = entry
                .run
                .as_ref()
                .map(|run| run.scope().clone())
                .expect("queued run scope missing");
            let launch = entry.launch.take().expect("queued run launch missing");
            let run_bus: crate::emit::EventBus = std::sync::Arc::new(QueuedRunEvents {
                inner: bus,
                key: key.clone(),
            });
            (key, scope, launch, run_bus, guard)
        };
        publish();
        let (key, scope, launch, bus, guard) = started;
        tauri::async_runtime::spawn(async move {
            let result = PipelineTask::spawn_future(scope, async move {
                let _guard = guard;
                match launch {
                    Launch::Review(review) => {
                        let review = *review;
                        run_pipeline_inner_with_snapshot(
                            &bus,
                            &review.paper_path,
                            review.input_interpretation.as_deref(),
                            review.diff,
                            review.variables,
                            review.extra_inputs,
                            Some(review.snapshot),
                        )
                        .await
                    }
                    Launch::Rerun {
                        run_id,
                        from_step,
                        only_failed,
                    } => rerun_run_inner(&bus, &run_id, from_step, only_failed).await,
                }
            })
            .join()
            .await;
            finish(&key, result);
            pump();
        });
    }
}

fn finish(key: &str, result: Result<serde_json::Value, String>) {
    {
        let mut queue = lock_queue();
        let Some(entry) = queue.entries.iter_mut().find(|entry| entry.info.key == key) else {
            return;
        };
        let cancelled = entry
            .run
            .take()
            .is_some_and(|run| run.scope().is_cancelled());
        entry.info.finished_at = Some(now_ms());
        match result {
            Ok(value) => {
                entry.info.run_id = value
                    .get("run_id")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                let failed_steps = value
                    .pointer("/report/failed_steps")
                    .and_then(|value| value.as_array())
                    .is_some_and(|steps| !steps.is_empty());
                entry.info.partial = failed_steps
                    || value.get("status").and_then(|value| value.as_str()) == Some("partial");
                entry.info.status = QueuedRunStatus::Done;
                entry.info.result_available = true;
                entry.result = Some(value);
            }
            Err(error) => {
                entry.info.status = if cancelled || is_pipeline_cancellation_error(&error) {
                    QueuedRunStatus::Cancelled
                } else {
                    QueuedRunStatus::Failed
                };
                entry.info.error = Some(error);
            }
        }
        prune(&mut queue);
    }
    publish();
}

/// Cancel one run: a queued run never starts; a running one stops.
pub(super) fn cancel(key: &str) -> Result<(), String> {
    {
        let mut queue = lock_queue();
        let entry = queue
            .entries
            .iter_mut()
            .find(|entry| entry.info.key == key)
            .ok_or("That review is no longer in the queue")?;
        match entry.info.status {
            QueuedRunStatus::Queued => {
                entry.info.status = QueuedRunStatus::Cancelled;
                entry.info.finished_at = Some(now_ms());
                entry.info.waiting = None;
                entry.launch = None;
                entry.run = None;
                prune(&mut queue);
            }
            QueuedRunStatus::Running => {
                if let Some(run) = &entry.run {
                    run.scope().cancel();
                }
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
    publish();
    pump();
    Ok(())
}

/// Cancel every queued and running review.
pub(super) fn cancel_all() {
    let keys: Vec<String> = lock_queue()
        .entries
        .iter()
        .filter(|entry| !is_finished(&entry.info.status))
        .map(|entry| entry.info.key.clone())
        .collect();
    for key in keys {
        let _ = cancel(&key);
    }
}

fn label_for(path: &str, profile_name: &str) -> String {
    if path.trim().is_empty() {
        profile_name.to_string()
    } else {
        basename(path.trim_end_matches(['/', '\\']))
    }
}

/// Queue one review with the same preflight as an immediate launch. Returns
/// as soon as the run is queued; progress arrives as tagged `pipeline:*`
/// events and `runs:queue` updates.
// Tauri exposes these launch fields as separate command arguments to keep the
// frontend IPC contract explicit, matching `run_pipeline`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn enqueue_run(
    app: AppHandle,
    paper_path: String,
    input_interpretation: Option<String>,
    diff: bool,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_snapshot_id: Option<String>,
    run_parallel_overrides: Option<RunParallelOverrides>,
) -> Result<QueuedRun, String> {
    let variables = variables.unwrap_or_default();
    let extra_inputs = extra_inputs.unwrap_or_default();
    let snapshot = prepare_foreground_launch(
        &paper_path,
        input_interpretation.as_deref(),
        diff,
        &variables,
        &extra_inputs,
        expected_profile_snapshot_id.as_deref(),
        run_parallel_overrides.as_ref(),
    )
    .await?;
    let profile_name = snapshot.profile_name.clone();
    Ok(enqueue(
        crate::emit::from_app(app),
        "review",
        label_for(&paper_path, &profile_name),
        paper_path.clone(),
        profile_name,
        Launch::Review(Box::new(ReviewLaunch {
            paper_path,
            input_interpretation,
            diff,
            variables,
            extra_inputs,
            snapshot,
        })),
    ))
}

/// Queue one review per document, all bound to one captured snapshot (the
/// queued form of `start_batch`).
#[tauri::command]
pub async fn enqueue_batch(
    app: AppHandle,
    paths: Vec<String>,
    variables: Option<std::collections::HashMap<String, String>>,
    extra_inputs: Option<std::collections::HashMap<String, String>>,
    expected_profile_config_snapshot_id: Option<String>,
    run_parallel_overrides: Option<RunParallelOverrides>,
) -> Result<Vec<QueuedRun>, String> {
    let launch = prepare_batch_launch(
        paths,
        variables,
        extra_inputs,
        expected_profile_config_snapshot_id,
        run_parallel_overrides,
    )
    .await?;
    let bus = crate::emit::from_app(app);
    Ok(launch
        .paths
        .iter()
        .map(|path| {
            enqueue(
                bus.clone(),
                "review",
                label_for(path, &launch.snapshot.profile_name),
                path.clone(),
                launch.snapshot.profile_name.clone(),
                Launch::Review(Box::new(ReviewLaunch {
                    paper_path: path.clone(),
                    input_interpretation: Some("document".into()),
                    diff: false,
                    variables: launch.variables.clone(),
                    extra_inputs: launch.extra_inputs.clone(),
                    snapshot: launch.snapshot.clone(),
                })),
            )
        })
        .collect())
}

/// Queue a re-run of a past run (the queued form of `rerun_run`).
#[tauri::command]
pub async fn enqueue_rerun(
    app: AppHandle,
    run_id: String,
    from_step: Option<String>,
    only_failed: bool,
) -> Result<QueuedRun, String> {
    crate::runs::recover_resumable_runs()?;
    let parent = crate::runs::load_manifest(&run_id)?;
    let profile_name = load_run_snapshot()
        .map(|snapshot| snapshot.profile_name)
        .unwrap_or_default();
    let title = if parent.title.trim().is_empty() {
        label_for(&parent.input_path, &profile_name)
    } else {
        parent.title.trim().to_string()
    };
    Ok(enqueue(
        crate::emit::from_app(app),
        "rerun",
        format!("Re-run · {title}"),
        parent.input_path.clone(),
        profile_name,
        Launch::Rerun {
            run_id,
            from_step,
            only_failed,
        },
    ))
}

#[tauri::command]
pub async fn list_run_queue() -> Result<RunQueueSnapshot, String> {
    let limit = concurrency_limit();
    Ok(snapshot(&lock_queue(), limit))
}

/// The full result (report, markdown, extracted text, run id) of a recently
/// finished review, while the queue still retains it.
#[tauri::command]
pub async fn get_queued_run_result(key: String) -> Result<Option<serde_json::Value>, String> {
    Ok(lock_queue()
        .entries
        .iter()
        .find(|entry| entry.info.key == key)
        .and_then(|entry| entry.result.clone()))
}

/// Remove finished reviews from the list: one by key, or all of them.
#[tauri::command]
pub async fn dismiss_queued_runs(key: Option<String>) -> Result<(), String> {
    {
        let mut queue = lock_queue();
        queue.entries.retain(|entry| {
            !is_finished(&entry.info.status)
                || key.as_deref().is_some_and(|key| key != entry.info.key)
        });
    }
    publish();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(key: &str, at: u64, result: bool) -> Entry {
        Entry {
            info: QueuedRun {
                key: key.into(),
                kind: "review".into(),
                label: key.into(),
                input_path: String::new(),
                profile_name: String::new(),
                status: QueuedRunStatus::Done,
                stage: None,
                stage_label: None,
                waiting: None,
                queued_at: at,
                started_at: Some(at),
                finished_at: Some(at),
                run_id: None,
                error: None,
                partial: false,
                result_available: result,
            },
            launch: None,
            run: None,
            result: result.then(|| serde_json::json!({})),
        }
    }

    #[test]
    fn prune_keeps_recent_finished_entries_and_results() {
        let mut queue = Queue::default();
        for index in 0..(MAX_FINISHED as u64 + 3) {
            queue
                .entries
                .push(finished(&format!("run-{index}"), index, true));
        }
        prune(&mut queue);
        assert_eq!(queue.entries.len(), MAX_FINISHED);
        assert_eq!(queue.entries[0].info.key, "run-3");
        let with_results: Vec<&str> = queue
            .entries
            .iter()
            .filter(|entry| entry.result.is_some())
            .map(|entry| entry.info.key.as_str())
            .collect();
        assert_eq!(with_results.len(), MAX_RESULTS);
        assert_eq!(
            with_results[0],
            format!("run-{}", MAX_FINISHED as u64 + 3 - MAX_RESULTS as u64)
        );
        assert!(queue
            .entries
            .iter()
            .all(|entry| entry.result.is_some() == entry.info.result_available));
    }

    #[test]
    fn queued_run_events_tag_pipeline_events_only() {
        #[derive(Default)]
        struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);
        impl crate::emit::Events for Recorder {
            fn emit_event(
                &self,
                event: &str,
                payload: serde_json::Value,
            ) -> Result<(), crate::emit::EmitError> {
                self.0.lock().unwrap().push((event.into(), payload));
                Ok(())
            }
        }
        let recorder = std::sync::Arc::new(Recorder::default());
        let bus = QueuedRunEvents {
            inner: recorder.clone(),
            key: "run-test-tag".into(),
        };
        use crate::emit::Events as _;
        bus.emit_event("pipeline:log", serde_json::json!({"line": "x"}))
            .unwrap();
        bus.emit_event("batch:progress", serde_json::json!({"line": "y"}))
            .unwrap();
        let events = recorder.0.lock().unwrap();
        assert_eq!(events[0].1["runKey"], "run-test-tag");
        assert!(events[1].1.get("runKey").is_none());
    }

    #[test]
    fn labels_use_the_input_name_or_workflow() {
        assert_eq!(label_for("/papers/draft.pdf", "Review"), "draft.pdf");
        assert_eq!(label_for("/papers/project/", "Review"), "project");
        assert_eq!(label_for("", "Referee report"), "Referee report");
    }
}
