use crate::agent_runtime::codex::session::{private_directory, private_write, NativeSession};
use crate::agent_runtime::codex::{AccountState, RateLimits};
use fs2::FileExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::{oneshot, Mutex, RwLock};

pub(super) const PROFILE: &str = "pipeline-workflow-host-tools";
static EPOCH: AtomicU64 = AtomicU64::new(1);
static CONNECTION: OnceLock<Mutex<Option<Arc<Connection>>>> = OnceLock::new();

type PendingLogin = Arc<Mutex<Option<(String, oneshot::Sender<()>)>>>;

pub(super) struct Connection {
    pub native: NativeSession,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub attempts: PathBuf,
    pub activity: Arc<RwLock<()>>,
    pub(super) recovery: Mutex<Vec<super::journal::UnresolvedAttempt>>,
    pending_login: PendingLogin,
    _lock: std::fs::File,
}

pub(super) fn config(launcher: &Path) -> String {
    // JSON string escaping is also valid for these TOML basic strings.
    let launcher = serde_json::to_string(&launcher.to_string_lossy()).unwrap();
    format!(
        r#"cli_auth_credentials_store = "file"
model_provider = "openai"
project_doc_max_bytes = 0
web_search = "disabled"
default_permissions = "{PROFILE}"
[features]
shell_tool = false
multi_agent = false
multi_agent_v2 = false
hooks = false
apps = false
remote_plugin = false
plugins = false
plugin_hooks = false
recommended_plugins = false
view_image = false
image_generation = false
skip_host_skill_discovery = true
[agents]
enabled = false
[permissions.{PROFILE}]
description = "Workflow tools are executed by Pipeline within each invocation's artifact view"
[permissions.{PROFILE}.filesystem]
":minimal" = "read"
{launcher} = "read"
[permissions.{PROFILE}.network]
enabled = false
"#
    )
}

impl Connection {
    #[cfg(test)]
    pub(super) fn simulated(
        root: &Path,
        client: crate::agent_runtime::codex::AppServerClient,
    ) -> Arc<Self> {
        let home = root.join("codex");
        let cwd = root.join("empty");
        let attempts = root.join("attempts");
        for path in [&home, &cwd, &attempts] {
            private_directory(path).unwrap();
        }
        Arc::new(Self {
            native: NativeSession::simulated(client),
            home,
            cwd,
            attempts,
            activity: Arc::new(RwLock::new(())),
            recovery: Mutex::new(Vec::new()),
            pending_login: Arc::new(Mutex::new(None)),
            _lock: std::fs::File::create(root.join("runtime.lock")).unwrap(),
        })
    }

    pub async fn launch(root: PathBuf) -> Result<Arc<Self>, String> {
        let prepared = tokio::task::spawn_blocking(move || {
            private_directory(&root)?;
            let lock_path = root.join("runtime.lock");
            let mut options = std::fs::OpenOptions::new();
            options.create(true).read(true).write(true).truncate(false);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
            }
            let lock = options.open(lock_path).map_err(|e| e.to_string())?;
            lock.try_lock_exclusive().map_err(|_| {
                "Workflow ChatGPT connection is in use by another Pipeline process".to_string()
            })?;
            let home = root.join("codex");
            let cwd = root.join("empty");
            let attempts = root.join("attempts");
            for path in [&home, &cwd, &attempts] {
                private_directory(path)?;
            }
            let launcher = crate::deps::resolve_command("codex")
                .ok_or("Codex is not installed")?
                .canonical_program()?;
            private_write(&home.join("config.toml"), config(&launcher).as_bytes())?;
            let recovery = super::journal::unresolved(&attempts)?;
            Ok::<_, String>((home, cwd, attempts, lock, recovery))
        })
        .await
        .map_err(|e| e.to_string())??;
        let (home, cwd, attempts, lock, recovery) = prepared;
        let native = NativeSession::launch(
            &home,
            &cwd,
            EPOCH.fetch_add(1, Ordering::Relaxed),
            "pipeline_workflows",
        )
        .await?;
        Ok(Arc::new(Self {
            native,
            home,
            cwd,
            attempts,
            activity: Arc::new(RwLock::new(())),
            recovery: Mutex::new(recovery),
            pending_login: Arc::new(Mutex::new(None)),
            _lock: lock,
        }))
    }
}

pub(super) async fn connect() -> Result<Arc<Connection>, String> {
    let mut slot = CONNECTION.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some(connection) = slot.as_ref() {
        if !connection.native.client.is_closed() {
            return Ok(connection.clone());
        }
        if Arc::strong_count(connection) > 1 {
            return Err("[codex-outcome-unknown] Previous Workflow connection is still reconciling; try again after its calls stop".into());
        }
        connection.native.shutdown().await;
    }
    slot.take();
    let root = dirs::home_dir()
        .ok_or("Cannot locate Pipeline home")?
        .join(".pipeline/providers/workflows");
    let connection = Connection::launch(root).await?;
    *slot = Some(connection.clone());
    Ok(connection)
}

pub async fn shutdown() {
    if let Some(slot) = CONNECTION.get() {
        if let Some(connection) = slot.lock().await.take() {
            connection.native.shutdown().await;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub account: AccountState,
    pub version: String,
    pub epoch: u64,
    pub home: String,
    pub login_in_progress: bool,
    pub unresolved_attempts: Vec<super::journal::UnresolvedAttempt>,
}

#[tauri::command]
pub async fn workflow_codex_status() -> Result<ConnectionStatus, String> {
    let c = connect().await?;
    let account = c
        .native
        .client
        .account_state(false)
        .await
        .map_err(|e| e.to_string())?;
    let login_in_progress = c.pending_login.lock().await.is_some();
    let unresolved_attempts = c.recovery.lock().await.clone();
    Ok(ConnectionStatus {
        unresolved_attempts,
        login_in_progress,
        account,
        version: c.native.version.clone(),
        epoch: c.native.client.epoch(),
        home: c.home.display().to_string(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLoginStart {
    pub login_id: String,
    pub epoch: u64,
}

#[tauri::command]
pub async fn workflow_codex_login_start(
    app: tauri::AppHandle,
) -> Result<WorkflowLoginStart, String> {
    let c = connect().await?;
    let permit = c.activity.clone().try_write_owned().map_err(|_| {
        "Wait for active Workflow calls or the current sign-in to finish before changing accounts"
    })?;
    let mut events = c.native.client.subscribe();
    let login = c
        .native
        .client
        .login_start()
        .await
        .map_err(|e| e.to_string())?;
    if !crate::agent_runtime::codex::account::is_safe_auth_url(&login.auth_url) {
        if c.native.client.login_cancel(&login.login_id).await.is_err() {
            c.native.terminate().await;
        }
        return Err("Codex returned an unsafe authentication URL".into());
    }
    use tauri_plugin_shell::ShellExt;
    #[allow(deprecated)]
    if let Err(error) = app.shell().open(&login.auth_url, None) {
        if c.native.client.login_cancel(&login.login_id).await.is_err() {
            c.native.terminate().await;
        }
        return Err(format!("Could not open ChatGPT sign-in: {error}"));
    }
    let (cancel, mut canceled) = oneshot::channel();
    let login_id = login.login_id.clone();
    let epoch = c.native.client.epoch();
    *c.pending_login.lock().await = Some((login_id.clone(), cancel));
    tokio::spawn(async move {
        // Hold the account mutation lease through browser completion, not only
        // through the login/start RPC. Calls cannot start in a changing account.
        let _permit = permit;
        let completed = tokio::time::timeout(std::time::Duration::from_secs(300), async {
            loop {
                tokio::select! {
                    _ = &mut canceled => return true,
                    event = events.recv() => match event {
                        Ok(crate::agent_runtime::codex::NormalizedEvent::AccountLoginCompleted { login_id: Some(id), .. }) if id == login_id => return true,
                        Ok(crate::agent_runtime::codex::NormalizedEvent::ConnectionClosed { .. }) | Err(_) => return false,
                        _ => {}
                    }
                }
            }
        }).await.unwrap_or(false);
        if !completed && c.native.client.login_cancel(&login_id).await.is_err() {
            c.native.terminate().await;
        }
        c.pending_login.lock().await.take();
    });
    Ok(WorkflowLoginStart {
        login_id: login.login_id,
        epoch,
    })
}

#[tauri::command]
pub async fn workflow_codex_login_cancel(login_id: String, epoch: u64) -> Result<bool, String> {
    let c = connect().await?;
    if c.native.client.epoch() != epoch {
        return Err("The sign-in connection changed; refresh its status".into());
    }
    let mut pending = c.pending_login.lock().await;
    if !pending.as_ref().is_some_and(|(id, _)| id == &login_id) {
        return Err("This sign-in is no longer active".into());
    }
    let result = c
        .native
        .client
        .login_cancel(&login_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some((_, cancel)) = pending.take() {
        let _ = cancel.send(());
    }
    Ok(result)
}

#[tauri::command]
pub async fn workflow_codex_logout() -> Result<(), String> {
    let c = connect().await?;
    let _permit = c
        .activity
        .try_write()
        .map_err(|_| "Wait for active Workflow ChatGPT calls to finish before signing out")?;
    match c.native.client.logout().await {
        Ok(()) => Ok(()),
        Err(error) => {
            // Do not release the account lease while an uncertain logout
            // could still mutate credentials beneath a newly starting call.
            c.native.terminate().await;
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn workflow_codex_rate_limits() -> Result<RateLimits, String> {
    connect()
        .await?
        .native
        .client
        .rate_limits()
        .await
        .map_err(|e| e.to_string())
}

/// Explicit user acknowledgement; never called from an automatic retry path.
#[tauri::command]
pub async fn workflow_codex_acknowledge_attempt(id: String, epoch: u64) -> Result<(), String> {
    let c = connect().await?;
    if c.native.client.epoch() != epoch {
        return Err("The Workflow connection changed; refresh status".into());
    }
    let _permit = c
        .activity
        .try_write()
        .map_err(|_| "Wait for active Workflow calls and sign-in to finish")?;
    let mut recovery = c.recovery.lock().await;
    if !recovery.iter().any(|attempt| attempt.id == id) {
        return Err("This attempt does not require acknowledgement".into());
    }
    super::journal::acknowledge(&c.attempts, &id)?;
    recovery.retain(|attempt| attempt.id != id);
    Ok(())
}

#[tauri::command]
pub async fn workflow_codex_reconcile_attempt(
    id: String,
    epoch: u64,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let c = connect().await?;
    if c.native.client.epoch() != epoch {
        return Err("The Workflow connection changed; refresh status".into());
    }
    let _permit = c.activity.read().await;
    if !c
        .recovery
        .lock()
        .await
        .iter()
        .any(|attempt| attempt.id == id)
    {
        return Err("This attempt does not require reconciliation".into());
    }
    super::journal::validate_id(&id)?;
    let path = c.attempts.join(format!("{id}.json"));
    let mut record = super::journal::read(&path)?;
    let thread = record["thread_id"]
        .as_str()
        .ok_or("Submission identity was not received; its result cannot be confirmed")?;
    let snapshot = c
        .native
        .client
        .request(
            "thread/read",
            json!({"threadId":thread,"includeTurns":true}),
            crate::agent_runtime::codex::DEFAULT_REQUEST_TIMEOUT,
        )
        .await
        .map_err(|e| e.to_string())?;
    // A missing turn acknowledgement must not be guessed from a different turn.
    let turn = record["turn_id"]
        .as_str()
        .ok_or("Turn identity was not received; its result cannot be confirmed")?;
    let found = snapshot["thread"]["turns"]
        .as_array()
        .and_then(|turns| turns.iter().find(|t| t["id"] == turn))
        .ok_or("The recorded turn is not available")?;
    record["reconciliation"] = found.clone();
    private_write(
        &path,
        &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
    )?;
    let output = found["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer")
                .filter_map(|item| item["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    Ok(
        json!({"status":found["status"],"preview":output.chars().take(32000).collect::<String>(),"recordPath":path}),
    )
}
