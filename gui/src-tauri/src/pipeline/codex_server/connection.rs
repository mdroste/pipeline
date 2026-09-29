use crate::agent_runtime::codex::chatgpt;
use crate::agent_runtime::codex::session::{private_directory, private_write, NativeSession};
use crate::agent_runtime::codex::{AccountState, RateLimits};
use fs2::FileExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, RwLock};

pub(super) const PROFILE: &str = "pipeline-workflow-host-tools";
static EPOCH: AtomicU64 = AtomicU64::new(1);
static CONNECTION: OnceLock<Mutex<Option<Arc<Connection>>>> = OnceLock::new();

pub(super) struct Connection {
    pub native: NativeSession,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub attempts: PathBuf,
    pub activity: Arc<RwLock<()>>,
    pub(super) recovery: Mutex<Vec<super::journal::UnresolvedAttempt>>,
    pub account: Option<Arc<chatgpt::ChatgptAccount>>,
    _lock: std::fs::File,
}

pub(super) fn config(launcher: &Path) -> String {
    // JSON string escaping is also valid for these TOML basic strings.
    let launcher = serde_json::to_string(&launcher.to_string_lossy()).unwrap();
    format!(
        r#"cli_auth_credentials_store = "ephemeral"
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
            account: None,
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
            let resolved = crate::deps::resolve_command("codex").ok_or("Codex is not installed")?;
            let launcher = resolved.canonical_program()?;
            private_write(&home.join("config.toml"), config(&launcher).as_bytes())?;
            let recovery = super::journal::unresolved(&attempts)?;
            Ok::<_, String>((home, cwd, attempts, lock, recovery, resolved))
        })
        .await
        .map_err(|e| e.to_string())??;
        let (home, cwd, attempts, lock, recovery, resolved) = prepared;
        let native = NativeSession::launch_resolved(
            &resolved,
            &home,
            &cwd,
            EPOCH.fetch_add(1, Ordering::Relaxed),
            "pipeline_workflows",
            "Pipeline",
        )
        .await?;
        Ok(Arc::new(Self {
            native,
            home,
            cwd,
            attempts,
            activity: Arc::new(RwLock::new(())),
            recovery: Mutex::new(recovery),
            account: None,
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
    let account = chatgpt::connect().await?;
    let mut connection = Connection::launch(root).await?;
    account.attach(&connection.native.client).await?;
    Arc::get_mut(&mut connection)
        .ok_or("New Review connection has unexpected owners")?
        .account = Some(account);
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
    let shared = chatgpt::connect().await?.status(false).await?;
    let account = shared.account;
    let login_in_progress = shared.login_in_progress;
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
    let login = chatgpt::chatgpt_login_start(app).await?;
    Ok(WorkflowLoginStart {
        login_id: login.login_id,
        epoch: login.epoch,
    })
}

#[tauri::command]
pub async fn workflow_codex_login_cancel(login_id: String, epoch: u64) -> Result<bool, String> {
    chatgpt::chatgpt_login_cancel(login_id, epoch).await
}

#[tauri::command]
pub async fn workflow_codex_logout() -> Result<(), String> {
    chatgpt::chatgpt_logout().await
}

#[tauri::command]
pub async fn workflow_codex_rate_limits() -> Result<RateLimits, String> {
    chatgpt::chatgpt_rate_limits().await
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
