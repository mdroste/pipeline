//! Lazy persistent App Server ownership and qualified thread operations.

use super::compatibility::MINIMUM_CODEX_VERSION;
use super::process::{
    capture_stderr, prepare_isolated_command, write_runtime_config, OwnedProcess,
};
use super::transport::{AppServerClient, RequestError, DEFAULT_REQUEST_TIMEOUT};
use super::wire::{InitializeResult, NormalizedEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

const MAX_TURN_TEXT_BYTES: usize = 2 * 1024 * 1024;
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_SIDE_TURN_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorStatus {
    pub connected: bool,
    pub epoch: u64,
    pub pid: Option<u32>,
    pub executable: Option<String>,
    pub user_agent: Option<String>,
    pub platform_family: Option<String>,
    pub platform_os: Option<String>,
    pub codex_home: Option<String>,
}

pub use crate::agent_runtime::codex::account::*;
#[cfg(test)]
use crate::agent_runtime::codex::account::{parse_account_state, MODEL_PAGE_SIZE};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartThreadRequest {
    pub workbench_session_id: String,
    pub cwd: String,
    pub runtime_workspace_roots: Vec<String>,
    pub permissions: String,
    pub developer_instructions: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    #[serde(default)]
    pub dynamic_tools: Vec<Value>,
}

/// One host-owned side call: an ephemeral thread without dynamic tools or
/// a Workspace binding, run to completion and discarded. Its events never
/// reach conversation projections or the frontend event bridge.
#[derive(Debug, Clone)]
pub struct SideTurnRequest {
    pub cwd: String,
    pub permissions: String,
    pub developer_instructions: String,
    pub text: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTurnRequest {
    pub workbench_binding_id: String,
    pub thread_id: String,
    pub text: String,
    pub client_user_message_id: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadConnection {
    pub workbench_binding_id: Option<String>,
    pub thread_id: String,
    pub provider_session_id: String,
    pub model: String,
    pub model_provider: String,
    pub cwd: String,
    pub instruction_sources: Vec<String>,
    pub runtime_workspace_roots: Vec<String>,
    pub active_permission_profile: Option<String>,
    pub approval_policy: Option<String>,
    pub approvals_reviewer: Option<String>,
}

pub struct AppServerSupervisor {
    epoch: u64,
    client: AppServerClient,
    process: Option<OwnedProcess>,
    initialize: InitializeResult,
    executable: Option<String>,
    stderr_task: Mutex<Option<tokio::task::JoinHandle<Vec<String>>>>,
    projection_tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    projection_enabled: bool,
    side_threads: StdMutex<HashSet<String>>,
}

impl AppServerSupervisor {
    pub async fn launch(epoch: u64) -> Result<Self, RequestError> {
        let codex_home = crate::workbench::commands::run_store(|store| Ok(store.codex_home_path()))
            .await
            .map_err(|error| RequestError::unavailable(error.message))?;
        Self::launch_with_codex_home(epoch, codex_home).await
    }

    async fn launch_with_codex_home(epoch: u64, codex_home: PathBuf) -> Result<Self, RequestError> {
        let resolved = crate::deps::resolve_command("codex").ok_or_else(|| {
            RequestError::unavailable("No launchable Codex CLI was found on PATH")
        })?;
        let executable = resolved.discovered_path().display().to_string();
        let version = super::probe::installed_version(&resolved)
            .await
            .map_err(RequestError::unavailable)?;
        if !version.meets_minimum {
            return Err(RequestError::unavailable(format!(
                "Codex CLI {} is too old for Workspace; install {MINIMUM_CODEX_VERSION} or newer",
                version.version
            )));
        }
        let launcher = resolved
            .canonical_program()
            .map_err(RequestError::unavailable)?;
        write_runtime_config(&codex_home, &launcher).map_err(RequestError::unavailable)?;
        let cwd = std::env::temp_dir();
        let command = prepare_isolated_command(&resolved, &codex_home, &cwd)
            .map_err(RequestError::unavailable)?;
        let (process, pipes) = OwnedProcess::spawn(command).map_err(RequestError::unavailable)?;
        let client = AppServerClient::from_io(epoch, pipes.stdout, pipes.stdin);
        let stderr_task = tokio::spawn(capture_stderr(pipes.stderr));
        let initialize = match client.initialize().await {
            Ok(initialize) => initialize,
            Err(error) => {
                process.terminate().await;
                stderr_task.abort();
                return Err(error);
            }
        };
        if !same_path(Path::new(&initialize.codex_home), &codex_home) {
            process.terminate().await;
            stderr_task.abort();
            return Err(RequestError::unavailable(
                "Codex App Server did not use Pipeline's isolated Workbench home",
            ));
        }
        Ok(Self {
            epoch,
            client,
            process: Some(process),
            initialize,
            executable: Some(executable),
            stderr_task: Mutex::new(Some(stderr_task)),
            projection_tasks: Mutex::new(HashMap::new()),
            projection_enabled: true,
            side_threads: StdMutex::new(HashSet::new()),
        })
    }

    #[cfg(test)]
    fn from_test_client(epoch: u64, client: AppServerClient, initialize: InitializeResult) -> Self {
        Self {
            epoch,
            client,
            process: None,
            initialize,
            executable: None,
            stderr_task: Mutex::new(None),
            projection_tasks: Mutex::new(HashMap::new()),
            projection_enabled: false,
            side_threads: StdMutex::new(HashSet::new()),
        }
    }

    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            connected: !self.client.is_closed(),
            epoch: self.epoch,
            pid: self.process.as_ref().map(OwnedProcess::pid),
            executable: self.executable.clone(),
            user_agent: Some(self.initialize.user_agent.clone()),
            platform_family: Some(self.initialize.platform_family.clone()),
            platform_os: Some(self.initialize.platform_os.clone()),
            codex_home: Some(self.initialize.codex_home.clone()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedEvent> {
        self.client.subscribe()
    }

    pub async fn account_state(&self, refresh_token: bool) -> Result<AccountState, RequestError> {
        self.client.account_state(refresh_token).await
    }

    pub async fn login_start(&self) -> Result<LoginStart, RequestError> {
        self.client.login_start().await
    }

    pub async fn login_cancel(&self, login_id: &str) -> Result<bool, RequestError> {
        self.client.login_cancel(login_id).await
    }

    pub async fn logout(&self) -> Result<(), RequestError> {
        self.client.logout().await
    }

    pub async fn model_catalog(&self) -> Result<ModelCatalog, RequestError> {
        self.client.model_catalog().await
    }

    pub async fn validate_model_selection(
        &self,
        model: &str,
        effort: Option<&str>,
    ) -> Result<WorkspaceModel, RequestError> {
        self.client.validate_model_selection(model, effort).await
    }

    pub async fn rate_limits(&self) -> Result<RateLimits, RequestError> {
        self.client.rate_limits().await
    }

    pub async fn start_thread(
        &self,
        request: StartThreadRequest,
    ) -> Result<ThreadConnection, RequestError> {
        validate_identifier("Workbench session id", &request.workbench_session_id)?;
        let workbench_session_id = request.workbench_session_id.clone();
        let mut connection = self.start_thread_unbound(request, false).await?;
        if self.projection_enabled {
            let binding = bind_session_projection(
                workbench_session_id,
                self.runtime_namespace(),
                connection.thread_id.clone(),
            )
            .await?;
            connection.workbench_binding_id = Some(binding.id.clone());
            self.attach_projection(connection.thread_id.clone(), binding.id)
                .await;
        }
        Ok(connection)
    }

    /// `thread/start` with the qualified safety defaults, validated against
    /// the request, but without any Workspace binding or projection.
    async fn start_thread_unbound(
        &self,
        request: StartThreadRequest,
        ephemeral: bool,
    ) -> Result<ThreadConnection, RequestError> {
        validate_thread_request(&request)?;
        if let Some(model) = request.model.as_deref() {
            self.validate_model_selection(model, request.effort.as_deref())
                .await?;
        }
        let expected_cwd = request.cwd.clone();
        let expected_roots = request.runtime_workspace_roots.clone();
        let expected_permissions = request.permissions.clone();
        let expected_model = request.model.clone();
        let result = self
            .client
            .request(
                "thread/start",
                json!({
                    "approvalPolicy": "untrusted",
                    "approvalsReviewer": "user",
                    "cwd": request.cwd,
                    "developerInstructions": request.developer_instructions,
                    "dynamicTools": request.dynamic_tools,
                    "ephemeral": ephemeral,
                    "model": request.model,
                    "modelProvider": "openai",
                    "reasoningEffort": request.effort,
                    "permissions": request.permissions,
                    "runtimeWorkspaceRoots": request.runtime_workspace_roots,
                    "allowProviderModelFallback": false,
                    "serviceName": "pipeline_workbench"
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let connection = parse_thread_connection(result)?;
        validate_thread_contract(
            &connection,
            Some(&expected_cwd),
            &expected_roots,
            &expected_permissions,
            expected_model.as_deref(),
        )?;
        Ok(connection)
    }

    /// Whether a thread belongs to a host-owned side call rather than a
    /// Workspace conversation. The event bridge drops such threads' events.
    pub fn is_side_thread(&self, thread_id: &str) -> bool {
        self.side_threads
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .contains(thread_id)
    }

    /// Runs one bounded, tool-free turn in an ephemeral thread and returns the
    /// final assistant text. Server requests are declined and the turn is
    /// interrupted, so a side call can never wait on the user.
    pub async fn run_side_turn(&self, request: SideTurnRequest) -> Result<String, RequestError> {
        if request.text.is_empty() || request.text.len() > MAX_SIDE_TURN_TEXT_BYTES {
            return Err(RequestError::invalid(format!(
                "Side turn text must contain 1 to {MAX_SIDE_TURN_TEXT_BYTES} bytes"
            )));
        }
        let mut events = self.client.subscribe();
        let connection = self
            .start_thread_unbound(
                StartThreadRequest {
                    workbench_session_id: "side-turn".to_string(),
                    cwd: request.cwd.clone(),
                    runtime_workspace_roots: vec![request.cwd],
                    permissions: request.permissions,
                    developer_instructions: request.developer_instructions,
                    model: request.model.clone(),
                    effort: request.effort.clone(),
                    dynamic_tools: Vec::new(),
                },
                true,
            )
            .await?;
        let thread_id = connection.thread_id;
        let _registration = SideThreadRegistration::register(&self.side_threads, &thread_id);
        let result = self
            .client
            .request(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{"type":"text", "text":request.text}],
                    "model": request.model,
                    "effort": request.effort
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")?;
        let collected = tokio::time::timeout(
            request.timeout,
            self.collect_side_turn(&mut events, &thread_id, &turn_id),
        )
        .await;
        match collected {
            Ok(result) => result,
            Err(_) => {
                let _ = self.interrupt_turn(&thread_id, &turn_id).await;
                Err(RequestError::unavailable(
                    "The side call did not finish within its time budget",
                ))
            }
        }
    }

    async fn collect_side_turn(
        &self,
        events: &mut broadcast::Receiver<NormalizedEvent>,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<String, RequestError> {
        let mut streamed = String::new();
        let mut final_text: Option<String> = None;
        let mut lagged = false;
        loop {
            match events.recv().await {
                Ok(NormalizedEvent::ConnectionClosed { reason, .. }) => {
                    return Err(RequestError::unavailable(format!(
                        "Workspace connection closed during the side call: {reason}"
                    )));
                }
                Ok(NormalizedEvent::ServerRequest {
                    request_id, params, ..
                }) if params.get("threadId").and_then(Value::as_str) == Some(thread_id) => {
                    let _ = self
                        .client
                        .respond_to_server_request(
                            request_id,
                            Err(RequestError::invalid(
                                "Side calls cannot request approvals or input",
                            )),
                        )
                        .await;
                    let _ = self.interrupt_turn(thread_id, turn_id).await;
                    return Err(RequestError::unavailable(
                        "The side call tried to use a tool and was stopped",
                    ));
                }
                Ok(event) => {
                    if event_thread_id(&event) != Some(thread_id) {
                        continue;
                    }
                    match event {
                        NormalizedEvent::AgentMessageDelta { delta, .. } => {
                            streamed.push_str(&delta)
                        }
                        NormalizedEvent::ItemCompleted {
                            item_kind, item, ..
                        } if item_kind == "agentMessage" => {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                final_text = Some(text.to_string());
                            }
                        }
                        NormalizedEvent::TurnCompleted {
                            turn_id: completed,
                            status,
                            turn,
                            ..
                        } if completed == turn_id => {
                            if status != "completed" {
                                return Err(RequestError::unavailable(format!(
                                    "The side call ended with status {status}"
                                )));
                            }
                            if final_text.is_none() {
                                final_text = last_agent_message(&turn);
                            }
                            if final_text.is_none() && lagged {
                                if let Ok(snapshot) = self.read_thread(thread_id).await {
                                    final_text = snapshot
                                        .get("thread")
                                        .and_then(|thread| thread.get("turns"))
                                        .and_then(Value::as_array)
                                        .and_then(|turns| {
                                            turns.iter().rev().find_map(last_agent_message)
                                        });
                                }
                            }
                            let text = final_text.unwrap_or(streamed);
                            if text.trim().is_empty() {
                                return Err(RequestError::unavailable(
                                    "The side call returned no assistant text",
                                ));
                            }
                            return Ok(text);
                        }
                        _ => {}
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => lagged = true,
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(RequestError::unavailable(
                        "Workspace event stream closed during the side call",
                    ));
                }
            }
        }
    }

    pub async fn resume_thread(
        &self,
        workbench_session_id: &str,
        thread_id: &str,
        permissions: &str,
        runtime_workspace_roots: Vec<String>,
    ) -> Result<ThreadConnection, RequestError> {
        validate_identifier("Workbench session id", workbench_session_id)?;
        validate_identifier("thread id", thread_id)?;
        validate_identifier("permissions profile", permissions)?;
        validate_roots(&runtime_workspace_roots)?;
        let expected_roots = runtime_workspace_roots.clone();
        let result = self
            .client
            .request(
                "thread/resume",
                json!({
                    "threadId": thread_id,
                    "permissions": permissions,
                    "runtimeWorkspaceRoots": runtime_workspace_roots,
                    "modelProvider": "openai"
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let mut connection = parse_thread_connection(result)?;
        validate_thread_contract(&connection, None, &expected_roots, permissions, None)?;
        if self.projection_enabled {
            let binding = bind_session_projection(
                workbench_session_id.to_string(),
                self.runtime_namespace(),
                connection.thread_id.clone(),
            )
            .await?;
            connection.workbench_binding_id = Some(binding.id.clone());
            self.attach_projection(connection.thread_id.clone(), binding.id)
                .await;
        }
        Ok(connection)
    }

    pub async fn read_thread(&self, thread_id: &str) -> Result<Value, RequestError> {
        validate_identifier("thread id", thread_id)?;
        self.client
            .request(
                "thread/read",
                json!({"threadId": thread_id, "includeTurns": true}),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await
    }

    pub async fn reconcile_thread(
        &self,
        workbench_binding_id: &str,
        thread_id: &str,
    ) -> Result<(), RequestError> {
        validate_identifier("Workbench binding id", workbench_binding_id)?;
        let snapshot = self.read_thread(thread_id).await?;
        let binding_id = workbench_binding_id.to_string();
        let thread_id = thread_id.to_string();
        let epoch = self.epoch;
        crate::workbench::commands::run_store(move |store| {
            reconcile_snapshot(&store, &binding_id, &thread_id, epoch, snapshot)
        })
        .await
        .map_err(|error| RequestError::unavailable(error.message))
    }

    pub async fn start_turn(&self, request: StartTurnRequest) -> Result<String, RequestError> {
        validate_identifier("Workbench binding id", &request.workbench_binding_id)?;
        validate_identifier("thread id", &request.thread_id)?;
        validate_identifier("client user message id", &request.client_user_message_id)?;
        if request.text.is_empty() || request.text.len() > MAX_TURN_TEXT_BYTES {
            return Err(RequestError::invalid(format!(
                "Turn text must contain 1 to {MAX_TURN_TEXT_BYTES} bytes"
            )));
        }
        if let Some(model) = request.model.as_deref() {
            self.validate_model_selection(model, request.effort.as_deref())
                .await?;
        }
        let result = self
            .client
            .request(
                "turn/start",
                json!({
                    "threadId": request.thread_id,
                    "input": [{"type":"text", "text":request.text}],
                    "clientUserMessageId": request.client_user_message_id,
                    "model": request.model,
                    "effort": request.effort
                }),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")?;
        if self.projection_enabled {
            record_turn_projection(
                request.workbench_binding_id,
                request.client_user_message_id,
                turn_id.clone(),
            )
            .await?;
        }
        Ok(turn_id)
    }

    pub async fn interrupt_turn(&self, thread_id: &str, turn_id: &str) -> Result<(), RequestError> {
        validate_identifier("thread id", thread_id)?;
        validate_identifier("turn id", turn_id)?;
        self.client
            .request(
                "turn/interrupt",
                json!({"threadId":thread_id, "turnId":turn_id}),
                DEFAULT_REQUEST_TIMEOUT,
            )
            .await?;
        Ok(())
    }

    pub async fn respond_to_server_request(
        &self,
        request_id: Value,
        result: Result<Value, RequestError>,
    ) -> Result<(), RequestError> {
        self.client
            .respond_to_server_request(request_id, result)
            .await
    }

    pub async fn shutdown(&self) {
        for (_, task) in self.projection_tasks.lock().await.drain() {
            task.abort();
        }
        let _ = self.client.close_writer().await;
        if let Some(process) = &self.process {
            match tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, process.wait()).await {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(status)) => self.client.fail(
                    format!("Codex App Server exited with status {status}"),
                    true,
                ),
                Ok(Err(error)) => self.client.fail(error, true),
                Err(_) => {
                    process.terminate().await;
                    self.client.fail(
                        "Codex App Server required forced process-tree cleanup",
                        true,
                    );
                }
            }
        }
        if let Some(mut task) = self.stderr_task.lock().await.take() {
            if tokio::time::timeout(Duration::from_secs(1), &mut task)
                .await
                .is_err()
            {
                // The process tree is already stopped. A descendant that kept
                // stderr open cannot retain an unbounded background task.
                task.abort();
            }
        }
    }

    pub(crate) fn runtime_namespace(&self) -> String {
        format!("codex-0-147-0-epoch-{}", self.epoch)
    }

    async fn attach_projection(&self, thread_id: String, binding_id: String) {
        let mut events = self.client.subscribe();
        let client = self.client.clone();
        let projected_thread_id = thread_id.clone();
        let task = tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if event_thread_id(&event) != Some(projected_thread_id.as_str()) {
                            if matches!(event, NormalizedEvent::ConnectionClosed { .. }) {
                                return;
                            }
                            continue;
                        }
                        let event_binding_id = binding_id.clone();
                        let result = crate::workbench::commands::run_store(move |store| {
                            store.project_codex_event(&event_binding_id, &event)
                        })
                        .await;
                        match result {
                            Ok(_) => {}
                            Err(error) => client.warn(format!(
                                "Workbench could not persist a Codex event: {}",
                                error.message
                            )),
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        client.warn(format!(
                            "Workbench projection lagged by {skipped} events; reconciling from thread/read"
                        ));
                        match client
                            .request(
                                "thread/read",
                                json!({"threadId":projected_thread_id,"includeTurns":true}),
                                DEFAULT_REQUEST_TIMEOUT,
                            )
                            .await
                        {
                            Ok(snapshot) => {
                                let snapshot_binding_id = binding_id.clone();
                                let snapshot_thread_id = projected_thread_id.clone();
                                let epoch = client.epoch();
                                let result = crate::workbench::commands::run_store(move |store| {
                                    reconcile_snapshot(
                                        &store,
                                        &snapshot_binding_id,
                                        &snapshot_thread_id,
                                        epoch,
                                        snapshot,
                                    )
                                })
                                .await;
                                if result.is_err() {
                                    client
                                        .warn("Workbench thread projection reconciliation failed");
                                }
                            }
                            Err(error) => client.warn(format!(
                                "Workbench could not reload a lagged thread: {}",
                                error.message
                            )),
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        if let Some(old) = self.projection_tasks.lock().await.insert(thread_id, task) {
            old.abort();
        }
    }
}

pub struct SupervisorManager {
    next_epoch: AtomicU64,
    current: Mutex<Option<Arc<AppServerSupervisor>>>,
}

static SUPERVISOR_MANAGER: OnceLock<SupervisorManager> = OnceLock::new();

pub fn supervisor_manager() -> &'static SupervisorManager {
    SUPERVISOR_MANAGER.get_or_init(SupervisorManager::default)
}

impl Default for SupervisorManager {
    fn default() -> Self {
        Self {
            next_epoch: AtomicU64::new(1),
            current: Mutex::new(None),
        }
    }
}

impl SupervisorManager {
    pub async fn connect(&self) -> Result<Arc<AppServerSupervisor>, RequestError> {
        let mut current = self.current.lock().await;
        if let Some(supervisor) = current.as_ref() {
            if supervisor.status().connected {
                return Ok(supervisor.clone());
            }
        }
        if let Some(stale) = current.take() {
            stale.shutdown().await;
        }
        let epoch = self.next_epoch.fetch_add(1, Ordering::Relaxed);
        let supervisor = Arc::new(AppServerSupervisor::launch(epoch).await?);
        *current = Some(supervisor.clone());
        Ok(supervisor)
    }

    pub async fn current(&self) -> Option<Arc<AppServerSupervisor>> {
        self.current.lock().await.clone()
    }

    pub async fn shutdown(&self) {
        if let Some(supervisor) = self.current.lock().await.take() {
            supervisor.shutdown().await;
        }
    }
}

fn validate_thread_request(request: &StartThreadRequest) -> Result<(), RequestError> {
    if request.developer_instructions.is_empty()
        || request.developer_instructions.len() > MAX_TURN_TEXT_BYTES
    {
        return Err(RequestError::invalid(
            "Developer instructions must contain 1 to 2 MiB",
        ));
    }
    validate_identifier("permissions profile", &request.permissions)?;
    let cwd = Path::new(&request.cwd);
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err(RequestError::invalid(
            "Thread cwd must be an existing absolute directory",
        ));
    }
    validate_roots(&request.runtime_workspace_roots)?;
    if !request
        .runtime_workspace_roots
        .iter()
        .any(|root| same_path(Path::new(root), cwd))
    {
        return Err(RequestError::invalid(
            "Thread cwd must be one of its runtime workspace roots",
        ));
    }
    Ok(())
}

fn validate_roots(roots: &[String]) -> Result<(), RequestError> {
    if roots.is_empty() || roots.len() > 16 {
        return Err(RequestError::invalid(
            "Runtime workspace roots must contain 1 to 16 paths",
        ));
    }
    for root in roots {
        let root = PathBuf::from(root);
        if !root.is_absolute() || !root.is_dir() {
            return Err(RequestError::invalid(
                "Runtime workspace roots must be existing absolute directories",
            ));
        }
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> Result<(), RequestError> {
    if value.is_empty()
        || value.len() > 256
        || value.chars().any(|character| character.is_control())
    {
        return Err(RequestError::invalid(format!("Invalid {label}")));
    }
    Ok(())
}

fn parse_thread_connection(value: Value) -> Result<ThreadConnection, RequestError> {
    let thread_id = required_string(&value, &["thread", "id"], "thread response")?;
    let provider_session_id = required_string(&value, &["thread", "sessionId"], "thread response")?;
    let model = required_string(&value, &["model"], "thread response")?;
    let model_provider = required_string(&value, &["modelProvider"], "thread response")?;
    let cwd = required_string(&value, &["cwd"], "thread response")?;
    let instruction_sources = string_array(&value, "instructionSources")?;
    let runtime_workspace_roots = string_array(&value, "runtimeWorkspaceRoots")?;
    let active_permission_profile = value
        .get("activePermissionProfile")
        .and_then(|profile| profile.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let approval_policy = value
        .get("approvalPolicy")
        .and_then(Value::as_str)
        .map(str::to_string);
    let approvals_reviewer = value
        .get("approvalsReviewer")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(ThreadConnection {
        workbench_binding_id: None,
        thread_id,
        provider_session_id,
        model,
        model_provider,
        cwd,
        instruction_sources,
        runtime_workspace_roots,
        active_permission_profile,
        approval_policy,
        approvals_reviewer,
    })
}

fn validate_thread_contract(
    connection: &ThreadConnection,
    expected_cwd: Option<&str>,
    expected_roots: &[String],
    expected_permissions: &str,
    expected_model: Option<&str>,
) -> Result<(), RequestError> {
    if connection.model_provider != "openai" {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor Workspace's OpenAI-only provider boundary",
        ));
    }
    let cwd_matches = expected_cwd
        .map(|expected| same_path(Path::new(&connection.cwd), Path::new(expected)))
        .unwrap_or_else(|| {
            expected_roots
                .iter()
                .any(|root| same_path(Path::new(&connection.cwd), Path::new(root)))
        });
    if !cwd_matches {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor the requested Workspace working directory",
        ));
    }
    if connection.runtime_workspace_roots.len() != expected_roots.len()
        || !expected_roots.iter().all(|expected| {
            connection
                .runtime_workspace_roots
                .iter()
                .any(|actual| same_path(Path::new(actual), Path::new(expected)))
        })
    {
        return Err(RequestError::unavailable(
            "Codex App Server did not honor the requested Workspace filesystem roots",
        ));
    }
    if connection.active_permission_profile.as_deref() != Some(expected_permissions) {
        return Err(RequestError::unavailable(format!(
            "Codex App Server did not activate Workspace permission profile {expected_permissions}"
        )));
    }
    if connection.approval_policy.as_deref() != Some("untrusted")
        || connection.approvals_reviewer.as_deref() != Some("user")
    {
        return Err(RequestError::unavailable(
            "Codex App Server did not preserve Workspace's approval policy",
        ));
    }
    if let Some(expected_model) = expected_model {
        if connection.model != expected_model {
            return Err(RequestError::unavailable(
                "Codex App Server substituted a different model despite fallback being disabled",
            ));
        }
    }
    Ok(())
}

async fn bind_session_projection(
    session_id: String,
    runtime_namespace: String,
    provider_thread_id: String,
) -> Result<crate::workbench::store::SessionBinding, RequestError> {
    crate::workbench::commands::run_store(move |store| {
        store.bind_session(&session_id, &runtime_namespace, &provider_thread_id)
    })
    .await
    .map_err(|error| RequestError::unavailable(error.message))
}

async fn record_turn_projection(
    binding_id: String,
    client_submission_id: String,
    provider_turn_id: String,
) -> Result<(), RequestError> {
    crate::workbench::commands::run_store(move |store| {
        store.record_turn_submission(&binding_id, &client_submission_id, &provider_turn_id)
    })
    .await
    .map_err(|error| RequestError::unavailable(error.message))
}

/// Removes a side thread from the registry when its call ends on any path.
struct SideThreadRegistration<'a> {
    registry: &'a StdMutex<HashSet<String>>,
    thread_id: String,
}

impl<'a> SideThreadRegistration<'a> {
    fn register(registry: &'a StdMutex<HashSet<String>>, thread_id: &str) -> Self {
        registry
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(thread_id.to_string());
        Self {
            registry,
            thread_id: thread_id.to_string(),
        }
    }
}

impl Drop for SideThreadRegistration<'_> {
    fn drop(&mut self) {
        self.registry
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.thread_id);
    }
}

fn last_agent_message(turn: &Value) -> Option<String> {
    turn.get("items")
        .and_then(Value::as_array)?
        .iter()
        .rev()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("agentMessage"))
        .and_then(|item| item.get("text").and_then(Value::as_str))
        .map(str::to_string)
}

/// The thread an event belongs to, when it carries one. Server requests carry
/// theirs inside `params.threadId`.
pub(crate) fn event_thread_id(event: &NormalizedEvent) -> Option<&str> {
    match event {
        NormalizedEvent::ThreadStarted { thread_id, .. }
        | NormalizedEvent::ThreadStatusChanged { thread_id, .. }
        | NormalizedEvent::TurnStarted { thread_id, .. }
        | NormalizedEvent::TurnCompleted { thread_id, .. }
        | NormalizedEvent::ItemStarted { thread_id, .. }
        | NormalizedEvent::ItemCompleted { thread_id, .. }
        | NormalizedEvent::AgentMessageDelta { thread_id, .. }
        | NormalizedEvent::ServerRequestResolved { thread_id, .. } => Some(thread_id),
        NormalizedEvent::ServerRequest { params, .. } => {
            params.get("threadId").and_then(Value::as_str)
        }
        _ => None,
    }
}

fn reconcile_snapshot(
    store: &crate::workbench::store::Store,
    binding_id: &str,
    thread_id: &str,
    epoch: u64,
    snapshot: Value,
) -> crate::workbench::store::WorkbenchResult<()> {
    let turns = snapshot
        .get("thread")
        .and_then(|thread| thread.get("turns"))
        .and_then(Value::as_array)
        .ok_or_else(|| crate::workbench::store::WorkbenchError {
            code: "storage_error".to_string(),
            message: "thread/read reconciliation response omitted thread.turns".to_string(),
            retryable: true,
            recovery: Some("Retry the Workbench session reload.".to_string()),
        })?;
    for turn in turns {
        let Some(turn_id) = turn.get("id").and_then(Value::as_str) else {
            continue;
        };
        let status = turn
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("inProgress");
        let event = if matches!(status, "completed" | "failed" | "interrupted") {
            NormalizedEvent::TurnCompleted {
                epoch,
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                status: status.to_string(),
                turn: turn.clone(),
            }
        } else {
            NormalizedEvent::TurnStarted {
                epoch,
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                turn: turn.clone(),
            }
        };
        store.project_codex_event(binding_id, &event)?;
    }
    Ok(())
}

fn required_string(value: &Value, path: &[&str], context: &str) -> Result<String, RequestError> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(key).ok_or_else(|| {
            RequestError::unavailable(format!("{context} omitted {}", path.join(".")))
        })?;
    }
    cursor.as_str().map(str::to_string).ok_or_else(|| {
        RequestError::unavailable(format!(
            "{context} field {} was not a string",
            path.join(".")
        ))
    })
}

fn string_array(value: &Value, key: &str) -> Result<Vec<String>, RequestError> {
    let values = value
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    values
        .into_iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                RequestError::unavailable(format!("thread response {key} contained a non-string"))
            })
        })
        .collect()
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn newer_servers_must_echo_the_security_contract() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let expected_roots = vec![root.clone()];
        let connection = ThreadConnection {
            workbench_binding_id: None,
            thread_id: "thread-1".to_string(),
            provider_session_id: "session-1".to_string(),
            model: "gpt-test".to_string(),
            model_provider: "openai".to_string(),
            cwd: root.clone(),
            instruction_sources: Vec::new(),
            runtime_workspace_roots: expected_roots.clone(),
            active_permission_profile: Some("pipeline-workbench".to_string()),
            approval_policy: Some("untrusted".to_string()),
            approvals_reviewer: Some("user".to_string()),
        };
        assert!(validate_thread_contract(
            &connection,
            Some(&root),
            &expected_roots,
            "pipeline-workbench",
            Some("gpt-test"),
        )
        .is_ok());

        for incompatible in [
            ThreadConnection {
                model_provider: "other".to_string(),
                ..connection.clone()
            },
            ThreadConnection {
                runtime_workspace_roots: Vec::new(),
                ..connection.clone()
            },
            ThreadConnection {
                active_permission_profile: None,
                ..connection.clone()
            },
            ThreadConnection {
                approval_policy: Some("never".to_string()),
                ..connection.clone()
            },
            ThreadConnection {
                model: "substituted".to_string(),
                ..connection.clone()
            },
        ] {
            assert!(validate_thread_contract(
                &incompatible,
                Some(&root),
                &expected_roots,
                "pipeline-workbench",
                Some("gpt-test"),
            )
            .is_err());
        }
    }

    #[test]
    fn thread_lifecycle_uses_qualified_methods_and_exact_safe_defaults() {
        runtime().block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().canonicalize().unwrap();
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, mut server_writer) = tokio::io::split(server_io);
            let mut server_reader = BufReader::new(server_reader);
            let workspace_for_server = workspace.clone();
            let server = tokio::spawn(async move {
                for expected in [
                    "thread/start",
                    "turn/start",
                    "turn/interrupt",
                    "thread/read",
                    "thread/resume",
                ] {
                    let mut line = String::new();
                    server_reader.read_line(&mut line).await.unwrap();
                    let request: Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(request["method"], expected);
                    let result = match expected {
                        "thread/start" => {
                            assert_eq!(request["params"]["modelProvider"], "openai");
                            assert_eq!(request["params"]["allowProviderModelFallback"], false);
                            assert_eq!(request["params"]["approvalPolicy"], "untrusted");
                            assert_eq!(request["params"]["dynamicTools"][0]["type"], "function");
                            assert_eq!(request["params"]["dynamicTools"][0]["name"], "fixture_read");
                            json!({
                                "thread":{"id":"thread-1","sessionId":"provider-session-1"},
                                "model":"gpt-test","modelProvider":"openai",
                                "activePermissionProfile":{"id":"pipeline-workbench"},
                                "approvalPolicy":"untrusted","approvalsReviewer":"user",
                                "cwd":workspace_for_server,
                                "instructionSources":[],
                                "runtimeWorkspaceRoots":[workspace_for_server]
                            })
                        }
                        "turn/start" => json!({"turn":{"id":"turn-1"}}),
                        "thread/resume" => json!({
                            "thread":{"id":"thread-1","sessionId":"provider-session-1"},
                            "model":"gpt-test","modelProvider":"openai",
                            "activePermissionProfile":{"id":"pipeline-workbench"},
                            "approvalPolicy":"untrusted","approvalsReviewer":"user",
                            "cwd":workspace_for_server,
                            "instructionSources":[],
                            "runtimeWorkspaceRoots":[workspace_for_server]
                        }),
                        _ => json!({}),
                    };
                    server_writer
                        .write_all(
                            format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes(),
                        )
                        .await
                        .unwrap();
                }
            });
            let initialize = InitializeResult {
                user_agent: "codex_cli_rs/0.147.0".to_string(),
                platform_family: "unix".to_string(),
                platform_os: "macos".to_string(),
                codex_home: temporary.path().display().to_string(),
            };
            let client = AppServerClient::from_io(8, client_reader, client_writer);
            let supervisor = AppServerSupervisor::from_test_client(8, client, initialize);
            let root = workspace.display().to_string();
            let thread = supervisor
                .start_thread(StartThreadRequest {
                    workbench_session_id: "session-test".to_string(),
                    cwd: root.clone(),
                    runtime_workspace_roots: vec![root.clone()],
                    permissions: "pipeline-workbench".to_string(),
                    developer_instructions: "Use only selected research context.".to_string(),
                    model: None,
                    effort: Some("high".to_string()),
                    dynamic_tools: vec![json!({"type":"function","name":"fixture_read","description":"Read a fixture.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})],
                })
                .await
                .unwrap();
            assert_eq!(thread.provider_session_id, "provider-session-1");
            assert_eq!(
                supervisor
                    .start_turn(StartTurnRequest {
                        workbench_binding_id: "binding-test".to_string(),
                        thread_id: thread.thread_id.clone(),
                        text: "Analyze the result.".to_string(),
                        client_user_message_id: "submission-1".to_string(),
                        model: None,
                        effort: None,
                    })
                    .await
                    .unwrap(),
                "turn-1"
            );
            supervisor
                .interrupt_turn(&thread.thread_id, "turn-1")
                .await
                .unwrap();
            supervisor.read_thread(&thread.thread_id).await.unwrap();
            supervisor
                .resume_thread(
                    "session-test",
                    &thread.thread_id,
                    "pipeline-workbench",
                    vec![root],
                )
                .await
                .unwrap();
            server.await.unwrap();
        });
    }

    /// A scripted server for side-turn tests: answers thread/start and
    /// turn/start, then plays the given notification frames.
    async fn side_turn_server(
        reader: tokio::io::ReadHalf<tokio::io::DuplexStream>,
        mut writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
        workspace: PathBuf,
        after_turn_start: Vec<Value>,
        expect_interrupt: bool,
    ) -> Vec<Value> {
        let mut reader = BufReader::new(reader);
        let mut observed = Vec::new();
        for expected in ["thread/start", "turn/start"] {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], expected);
            observed.push(request.clone());
            let result = if expected == "thread/start" {
                json!({
                    "thread":{"id":"thread-side","sessionId":"provider-side"},
                    "model":"gpt-mini","modelProvider":"openai",
                    "activePermissionProfile":{"id":"workbench-inspect"},
                    "approvalPolicy":"untrusted","approvalsReviewer":"user",
                    "cwd":workspace,
                    "instructionSources":[],
                    "runtimeWorkspaceRoots":[workspace]
                })
            } else {
                json!({"turn":{"id":"turn-side"}})
            };
            writer
                .write_all(format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes())
                .await
                .unwrap();
        }
        for frame in after_turn_start {
            writer
                .write_all(format!("{frame}\n").as_bytes())
                .await
                .unwrap();
        }
        if expect_interrupt {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            assert!(
                response.get("error").is_some(),
                "side call must decline the request"
            );
            observed.push(response);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "turn/interrupt");
            observed.push(request.clone());
            writer
                .write_all(format!("{}\n", json!({"id":request["id"],"result":{}})).as_bytes())
                .await
                .unwrap();
        }
        observed
    }

    fn side_turn_request(root: &Path) -> SideTurnRequest {
        SideTurnRequest {
            cwd: root.display().to_string(),
            permissions: "workbench-inspect".to_string(),
            developer_instructions: "Reply with only the title.".to_string(),
            text: "Title this.".to_string(),
            model: None,
            effort: Some("low".to_string()),
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn side_turns_run_ephemeral_tool_free_threads_and_return_the_final_message() {
        runtime().block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().canonicalize().unwrap();
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, server_writer) = tokio::io::split(server_io);
            let frames = vec![
                json!({"method":"item/agentMessage/delta","params":{"threadId":"thread-side","turnId":"turn-side","itemId":"item-side","delta":"Ident"}}),
                json!({"method":"item/completed","params":{"threadId":"thread-side","turnId":"turn-side","item":{"id":"item-side","type":"agentMessage","text":"Identification in panels"}}}),
                json!({"method":"turn/completed","params":{"threadId":"thread-side","turn":{"id":"turn-side","status":"completed","items":[]}}}),
            ];
            let server = tokio::spawn(side_turn_server(
                server_reader,
                server_writer,
                workspace.clone(),
                frames,
                false,
            ));
            let client = AppServerClient::from_io(11, client_reader, client_writer);
            let supervisor = AppServerSupervisor::from_test_client(
                11,
                client,
                InitializeResult {
                    user_agent: "codex_cli_rs/0.147.0".to_string(),
                    platform_family: "unix".to_string(),
                    platform_os: "macos".to_string(),
                    codex_home: temporary.path().display().to_string(),
                },
            );
            let text = supervisor
                .run_side_turn(side_turn_request(&workspace))
                .await
                .unwrap();
            assert_eq!(text, "Identification in panels");
            assert!(!supervisor.is_side_thread("thread-side"));
            let observed = server.await.unwrap();
            assert_eq!(observed[0]["params"]["ephemeral"], true);
            assert_eq!(observed[0]["params"]["dynamicTools"], json!([]));
            assert_eq!(observed[0]["params"]["permissions"], "workbench-inspect");
            assert_eq!(observed[0]["params"]["allowProviderModelFallback"], false);
            assert_eq!(observed[1]["params"]["threadId"], "thread-side");
            assert_eq!(observed[1]["params"]["effort"], "low");
            assert!(observed[1]["params"].get("clientUserMessageId").is_none());
        });
    }

    #[test]
    fn side_turns_decline_server_requests_and_stop() {
        runtime().block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let workspace = temporary.path().canonicalize().unwrap();
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, server_writer) = tokio::io::split(server_io);
            let frames = vec![json!({
                "id":"approval-side",
                "method":"item/commandExecution/requestApproval",
                "params":{"threadId":"thread-side","turnId":"turn-side","command":"ls"}
            })];
            let server = tokio::spawn(side_turn_server(
                server_reader,
                server_writer,
                workspace.clone(),
                frames,
                true,
            ));
            let client = AppServerClient::from_io(12, client_reader, client_writer);
            let supervisor = AppServerSupervisor::from_test_client(
                12,
                client,
                InitializeResult {
                    user_agent: "codex_cli_rs/0.147.0".to_string(),
                    platform_family: "unix".to_string(),
                    platform_os: "macos".to_string(),
                    codex_home: temporary.path().display().to_string(),
                },
            );
            let error = supervisor
                .run_side_turn(side_turn_request(&workspace))
                .await
                .unwrap_err();
            assert!(
                error.message.contains("tried to use a tool"),
                "{}",
                error.message
            );
            assert!(!supervisor.is_side_thread("thread-side"));
            let observed = server.await.unwrap();
            assert_eq!(observed[2]["id"], "approval-side");
            assert_eq!(observed[3]["params"]["turnId"], "turn-side");
        });
    }

    #[test]
    fn account_login_catalog_and_quota_use_the_pinned_protocol_contract() {
        runtime().block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client_io);
            let (server_reader, mut server_writer) = tokio::io::split(server_io);
            let mut server_reader = BufReader::new(server_reader);
            let server = tokio::spawn(async move {
                for (index, expected) in [
                    "account/read",
                    "account/login/start",
                    "account/login/cancel",
                    "account/logout",
                    "model/list",
                    "model/list",
                    "account/rateLimits/read",
                ]
                .into_iter()
                .enumerate()
                {
                    let mut line = String::new();
                    server_reader.read_line(&mut line).await.unwrap();
                    let request: Value = serde_json::from_str(&line).unwrap();
                    assert_eq!(request["method"], expected);
                    let result = match index {
                        0 => {
                            assert_eq!(request["params"], json!({"refreshToken":false}));
                            json!({
                                "account":{"type":"chatgpt","email":"researcher@example.test","planType":"plus"},
                                "requiresOpenaiAuth":true
                            })
                        }
                        1 => {
                            assert_eq!(request["params"]["type"], "chatgpt");
                            assert_eq!(request["params"]["appBrand"], "chatgpt");
                            assert_eq!(request["params"]["useHostedLoginSuccessPage"], true);
                            assert!(request["params"].get("apiKey").is_none());
                            json!({"type":"chatgpt","loginId":"login-1","authUrl":"https://auth.openai.test/login"})
                        }
                        2 => {
                            assert_eq!(request["params"], json!({"loginId":"login-1"}));
                            json!({"status":"canceled"})
                        }
                        3 => {
                            assert!(request["params"].is_null());
                            json!({})
                        }
                        4 => {
                            assert_eq!(request["params"]["cursor"], Value::Null);
                            assert_eq!(request["params"]["includeHidden"], false);
                            assert_eq!(request["params"]["limit"], MODEL_PAGE_SIZE);
                            json!({
                                "data":[model_fixture("model-a", true)],
                                "nextCursor":"page-2"
                            })
                        }
                        5 => {
                            assert_eq!(request["params"]["cursor"], "page-2");
                            json!({"data":[model_fixture("model-b", false)],"nextCursor":null})
                        }
                        6 => {
                            assert!(request["params"].is_null());
                            json!({
                                "rateLimits": {"primary":{"usedPercent":91}},
                                "rateLimitsByLimitId": {
                                    "codex": {
                                        "limitName":"Codex",
                                        "planType":"plus",
                                        "primary":{"usedPercent":42,"windowDurationMins":300,"resetsAt":1234}
                                    }
                                }
                            })
                        }
                        _ => unreachable!(),
                    };
                    server_writer
                        .write_all(
                            format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes(),
                        )
                        .await
                        .unwrap();
                }
            });
            let client = AppServerClient::from_io(9, client_reader, client_writer);
            let supervisor = AppServerSupervisor::from_test_client(
                9,
                client,
                InitializeResult {
                    user_agent: "codex_cli_rs/0.147.0".to_string(),
                    platform_family: "unix".to_string(),
                    platform_os: "macos".to_string(),
                    codex_home: temporary.path().display().to_string(),
                },
            );

            let account = supervisor.account_state(false).await.unwrap();
            assert_eq!(account.status, AccountStatus::Chatgpt);
            assert_eq!(account.plan_type.as_deref(), Some("plus"));
            let login = supervisor.login_start().await.unwrap();
            assert_eq!(login.login_id, "login-1");
            assert!(supervisor.login_cancel(&login.login_id).await.unwrap());
            supervisor.logout().await.unwrap();
            let catalog = supervisor.model_catalog().await.unwrap();
            assert_eq!(catalog.models.len(), 2);
            assert_eq!(catalog.models[1].model, "model-b");
            assert_eq!(
                catalog
                    .validate_exact("model-a", Some("medium"))
                    .unwrap()
                    .model,
                "model-a"
            );
            let unavailable = catalog.validate_exact("model-missing", None).unwrap_err();
            assert_eq!(unavailable.code, "invalid_input");
            assert!(catalog.validate_exact("model-a", Some("high")).is_err());
            let limits = supervisor.rate_limits().await.unwrap();
            assert_eq!(limits.source, RateLimitSource::PerBucket);
            assert_eq!(limits.buckets[0].limit_id.as_deref(), Some("codex"));
            assert_eq!(limits.buckets[0].primary.as_ref().unwrap().used_percent, 42);
            assert_eq!(
                limits.buckets[0].primary.as_ref().unwrap().remaining_percent,
                58
            );
            server.await.unwrap();
        });
    }

    #[test]
    fn account_parser_distinguishes_signed_out_and_unsupported_modes() {
        assert_eq!(
            parse_account_state(json!({"account":null,"requiresOpenaiAuth":true}))
                .unwrap()
                .status,
            AccountStatus::SignedOut
        );
        let unsupported = parse_account_state(json!({
            "account":{"type":"apiKey"},
            "requiresOpenaiAuth":true
        }))
        .unwrap();
        assert_eq!(unsupported.status, AccountStatus::Unsupported);
        assert_eq!(
            unsupported.unsupported_account_type.as_deref(),
            Some("apiKey")
        );
    }

    fn model_fixture(id: &str, is_default: bool) -> Value {
        json!({
            "id":id,
            "model":id,
            "displayName":id,
            "description":"Test model",
            "isDefault":is_default,
            "defaultReasoningEffort":"medium",
            "supportedReasoningEfforts":[{
                "reasoningEffort":"medium",
                "description":"Balanced"
            }]
        })
    }

    #[test]
    #[ignore = "requires a locally installed capability-compatible Codex CLI"]
    fn live_supervisor_starts_in_an_isolated_temporary_home_and_shuts_down_cleanly() {
        runtime().block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let codex_home = temporary.path().join("codex");
            let supervisor = AppServerSupervisor::launch_with_codex_home(73, codex_home.clone())
                .await
                .unwrap();
            let status = supervisor.status();
            assert!(status.connected);
            assert_eq!(status.epoch, 73);
            assert!(same_path(
                Path::new(status.codex_home.as_deref().unwrap()),
                &codex_home
            ));
            supervisor.shutdown().await;
            assert!(!supervisor.status().connected);
        });
    }
}
