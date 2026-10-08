//! Lazy persistent App Server ownership and qualified thread operations.

use super::process::write_runtime_config;
#[cfg(test)]
use super::transport::AppServerClient;
use super::transport::{RequestError, DEFAULT_REQUEST_TIMEOUT};
#[cfg(test)]
use super::wire::InitializeResult;
use super::wire::NormalizedEvent;
use crate::agent_runtime::codex::invocation::{NativeThreadStart, TextTurnStart};
use crate::agent_runtime::codex::session::{same_existing_path as same_path, NativeSession};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;
use tokio::sync::{broadcast, Mutex};

const MAX_TURN_TEXT_BYTES: usize = 2 * 1024 * 1024;
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
    #[serde(default)]
    pub base_instructions: Option<String>,
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
    native: Arc<NativeSession>,
    account: Option<Arc<crate::agent_runtime::codex::chatgpt::ChatgptAccount>>,
    projection_tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    projection_enabled: bool,
    side_threads: StdMutex<HashSet<String>>,
}

impl AppServerSupervisor {
    pub async fn launch(epoch: u64) -> Result<Self, RequestError> {
        let codex_home = crate::workbench::commands::run_store(|store| Ok(store.codex_home_path()))
            .await
            .map_err(|error| RequestError::unavailable(error.message))?;
        let account = crate::agent_runtime::codex::chatgpt::connect()
            .await
            .map_err(RequestError::unavailable)?;
        let mut supervisor = Self::launch_with_codex_home(epoch, codex_home).await?;
        account
            .attach(&supervisor.native.client)
            .await
            .map_err(RequestError::unavailable)?;
        supervisor.account = Some(account);
        Ok(supervisor)
    }

    async fn launch_with_codex_home(epoch: u64, codex_home: PathBuf) -> Result<Self, RequestError> {
        let resolved = crate::deps::resolve_command("codex").ok_or_else(|| {
            RequestError::unavailable("No launchable Codex CLI was found on PATH")
        })?;
        let launcher = resolved
            .canonical_program()
            .map_err(RequestError::unavailable)?;
        write_runtime_config(&codex_home, &launcher).map_err(RequestError::unavailable)?;
        let cwd = std::env::temp_dir();
        let native = NativeSession::launch_resolved(
            &resolved,
            &codex_home,
            &cwd,
            epoch,
            "pipeline_workbench",
            "Pipeline Workbench",
        )
        .await
        .map_err(RequestError::unavailable)?;
        Ok(Self {
            epoch,
            native: Arc::new(native),
            account: None,
            projection_tasks: Mutex::new(HashMap::new()),
            projection_enabled: true,
            side_threads: StdMutex::new(HashSet::new()),
        })
    }

    #[cfg(test)]
    fn from_test_client(epoch: u64, client: AppServerClient, initialize: InitializeResult) -> Self {
        let mut native = NativeSession::simulated(client);
        native.initialize = initialize;
        Self {
            epoch,
            native: Arc::new(native),
            account: None,
            projection_tasks: Mutex::new(HashMap::new()),
            projection_enabled: false,
            side_threads: StdMutex::new(HashSet::new()),
        }
    }

    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            connected: !self.native.client.is_closed(),
            epoch: self.epoch,
            pid: self.native.pid(),
            executable: self.native.executable.clone(),
            user_agent: Some(self.native.initialize.user_agent.clone()),
            platform_family: Some(self.native.initialize.platform_family.clone()),
            platform_os: Some(self.native.initialize.platform_os.clone()),
            codex_home: Some(self.native.initialize.codex_home.clone()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NormalizedEvent> {
        self.native.client.subscribe()
    }

    pub async fn account_state(&self, refresh_token: bool) -> Result<AccountState, RequestError> {
        if let Some(account) = &self.account {
            return account
                .status(refresh_token)
                .await
                .map(|status| status.account)
                .map_err(RequestError::unavailable);
        }
        self.native.client.account_state(refresh_token).await
    }

    #[cfg(test)]
    pub async fn login_start(&self) -> Result<LoginStart, RequestError> {
        self.native.client.login_start().await
    }

    #[cfg(test)]
    pub async fn login_cancel(&self, login_id: &str) -> Result<bool, RequestError> {
        self.native.client.login_cancel(login_id).await
    }

    #[cfg(test)]
    pub async fn logout(&self) -> Result<(), RequestError> {
        self.native.client.logout().await
    }

    pub async fn model_catalog(&self) -> Result<ModelCatalog, RequestError> {
        if self.account.is_some() {
            return crate::agent_runtime::codex::chatgpt::chatgpt_model_catalog()
                .await
                .map_err(RequestError::unavailable);
        }
        self.native.client.model_catalog().await
    }

    pub async fn validate_model_selection(
        &self,
        model: &str,
        effort: Option<&str>,
    ) -> Result<WorkspaceModel, RequestError> {
        self.model_catalog().await?.validate_exact(model, effort)
    }

    pub async fn rate_limits(&self) -> Result<RateLimits, RequestError> {
        if self.account.is_some() {
            return crate::agent_runtime::codex::chatgpt::chatgpt_rate_limits()
                .await
                .map_err(RequestError::unavailable);
        }
        self.native.client.rate_limits().await
    }

    pub(crate) async fn account_lease(
        &self,
    ) -> Result<Option<tokio::sync::OwnedRwLockReadGuard<()>>, RequestError> {
        match &self.account {
            Some(account) => account
                .lease(&self.native.client)
                .await
                .map(Some)
                .map_err(RequestError::unavailable),
            None => Ok(None),
        }
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
            .native
            .client
            .start_native_thread(NativeThreadStart {
                cwd: Path::new(&request.cwd),
                runtime_workspace_roots: &request.runtime_workspace_roots,
                permissions: &request.permissions,
                approval_policy: approval_policy(&expected_permissions),
                developer_instructions: Some(&request.developer_instructions),
                base_instructions: request.base_instructions.as_deref(),
                dynamic_tools: &request.dynamic_tools,
                model: request.model.as_deref(),
                reasoning_effort: request.effort.as_deref(),
                ephemeral,
                service_name: "pipeline_workbench",
                config: None,
            })
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
        // Side calls own a separate process. Their teardown must never interrupt
        // the foreground conversation, even on an ambiguous acknowledgement.
        let account = self.account.as_ref().ok_or_else(|| {
            RequestError::unavailable("A managed account is required for a side turn")
        })?;
        let home = tempfile::tempdir().map_err(|e| RequestError::unavailable(e.to_string()))?;
        let mut side = Self::launch_with_codex_home(self.epoch, home.path().join("codex")).await?;
        account
            .attach(&side.native.client)
            .await
            .map_err(RequestError::unavailable)?;
        side.account = Some(account.clone());
        side.projection_enabled = false;
        side.run_owned_side_turn(request, Some(home)).await
    }

    async fn run_owned_side_turn(
        &self,
        request: SideTurnRequest,
        home: Option<tempfile::TempDir>,
    ) -> Result<String, RequestError> {
        if request.text.is_empty() || request.text.len() > MAX_SIDE_TURN_TEXT_BYTES {
            return Err(RequestError::invalid(format!(
                "Side turn text must contain 1 to {MAX_SIDE_TURN_TEXT_BYTES} bytes"
            )));
        }
        let mut account_use = SideAccountUse {
            permit: self.account_lease().await?,
            native: self.native.clone(),
            completed: false,
            submitted: false,
            home,
        };
        let mut events = self.native.client.subscribe();
        let connection = self
            .start_thread_unbound(
                StartThreadRequest {
                    workbench_session_id: "side-turn".to_string(),
                    base_instructions: None,
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
        account_use.submitted = true;
        let result = self
            .native
            .client
            .start_text_turn(TextTurnStart {
                thread_id: &thread_id,
                text: &request.text,
                client_user_message_id: None,
                model: request.model.as_deref(),
                effort: request.effort.as_deref(),
                output_schema: None,
            })
            .await?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")?;
        let collected = tokio::time::timeout(
            request.timeout,
            self.collect_side_turn(&mut events, &thread_id, &turn_id),
        )
        .await;
        match collected {
            Ok(Ok(text)) => {
                account_use.completed = true;
                Ok(text)
            }
            Ok(Err(error)) => Err(error),
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
                        .native
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
            .native
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
        self.native
            .client
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
        self.start_turn_with_acceptance(request, |_| {}).await
    }

    pub(crate) async fn start_turn_with_acceptance(
        &self,
        request: StartTurnRequest,
        accepted: impl FnOnce(&str),
    ) -> Result<String, RequestError> {
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
            .native
            .client
            .start_text_turn(TextTurnStart {
                thread_id: &request.thread_id,
                text: &request.text,
                client_user_message_id: Some(&request.client_user_message_id),
                model: request.model.as_deref(),
                effort: request.effort.as_deref(),
                output_schema: None,
            })
            .await?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")?;
        // Publish ownership before a fallible projection write.
        accepted(&turn_id);
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
        self.native
            .client
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
        self.native
            .client
            .respond_to_server_request(request_id, result)
            .await
    }

    pub async fn shutdown(&self) {
        for (_, task) in self.projection_tasks.lock().await.drain() {
            task.abort();
        }
        self.native.shutdown().await;
    }

    pub(crate) fn runtime_namespace(&self) -> String {
        format!("codex-0-147-0-epoch-{}", self.epoch)
    }

    async fn attach_projection(&self, thread_id: String, binding_id: String) {
        let mut events = self.native.client.subscribe();
        let client = self.native.client.clone();
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
    if let Some(text) = &request.base_instructions {
        if text.trim().is_empty() || text.len() > 256 * 1024 || text.contains('\0') {
            return Err(RequestError::invalid(
                "Replacement base prompt must contain 1 to 256 KiB and no NUL characters",
            ));
        }
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

fn approval_policy(permissions: &str) -> &'static str {
    if matches!(permissions, "discovery-inspect" | "discovery-edit") {
        "never"
    } else {
        "untrusted"
    }
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
    if connection.approval_policy.as_deref() != Some(approval_policy(expected_permissions))
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

#[cfg(test)]
mod tests;

/// On dropped/failed background calls, stop the native connection before releasing
/// the shared account lease. An unconfirmed turn must not outlive its account.
struct SideAccountUse {
    permit: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
    native: Arc<NativeSession>,
    completed: bool,
    submitted: bool,
    home: Option<tempfile::TempDir>,
}
impl Drop for SideAccountUse {
    fn drop(&mut self) {
        if self.home.is_some() || (!self.completed && self.submitted) {
            let permit = self.permit.take();
            let home = self.home.take();
            let native = self.native.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let _home = home;
                native.terminate().await;
            });
        }
    }
}

#[cfg(test)]
mod side_ownership_tests;
