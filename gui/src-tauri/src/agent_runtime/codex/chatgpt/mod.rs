//! One application-owned ChatGPT account; execution runtimes retain separate homes.
mod commands;
mod credentials;
mod refresh;
#[cfg(test)]
mod tests;
use super::session::{private_directory, private_write, NativeSession};
use super::{AccountState, AccountStatus, AppServerClient, LoginStart, NormalizedEvent};
pub(crate) use commands::*;
use credentials::ExistingAccount;
use fs2::FileExt;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{oneshot, Mutex, OwnedRwLockReadGuard, RwLock};

static ACCOUNT: OnceLock<Mutex<Option<Arc<ChatgptAccount>>>> = OnceLock::new();
static EPOCH: AtomicU64 = AtomicU64::new(1);
const BUSY: &str = "Wait for active Conversations, Reviews, or the current sign-in to finish before changing the ChatGPT account";

pub(crate) struct ChatgptAccount {
    native: Mutex<NativeSession>,
    root: PathBuf,
    user_home: PathBuf,
    activity: Arc<RwLock<()>>,
    serial: Mutex<()>,
    clients: Mutex<Vec<AppServerClient>>,
    pending: Mutex<Option<PendingLogin>>,
    choices: Mutex<Vec<ExistingAccount>>,
    login_error: Mutex<Option<String>>,
    login_active: std::sync::atomic::AtomicBool,
    events: tokio::sync::broadcast::Sender<()>,
    _lock: std::fs::File,
}
struct PendingLogin {
    id: String,
    epoch: u64,
    cancel: oneshot::Sender<()>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountLogin {
    pub login_id: String,
    pub epoch: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub account: AccountState,
    pub epoch: u64,
    pub version: String,
    pub login_in_progress: bool,
    pub pending_login: Option<AccountLogin>,
    pub existing_accounts: Vec<ExistingAccount>,
    pub error: Option<String>,
}

async fn launch_native(root: &Path, epoch: u64) -> Result<NativeSession, String> {
    let resolved =
        crate::deps::resolve_command("codex").ok_or("No launchable Codex CLI was found on PATH")?;
    NativeSession::launch_resolved(
        &resolved,
        &root.join("codex"),
        &root.join("empty"),
        epoch,
        "pipeline_chatgpt",
        "Pipeline ChatGPT",
    )
    .await
}

pub(crate) async fn connect() -> Result<Arc<ChatgptAccount>, String> {
    let mut slot = ACCOUNT.get_or_init(|| Mutex::new(None)).lock().await;
    if let Some(account) = slot.as_ref() {
        return Ok(account.clone());
    }
    let user_home = dirs::home_dir().ok_or("Cannot determine the user home")?;
    let account =
        ChatgptAccount::launch(user_home.join(".pipeline/providers/chatgpt"), user_home).await?;
    *slot = Some(account.clone());
    Ok(account)
}

impl ChatgptAccount {
    async fn launch(root: PathBuf, user_home: PathBuf) -> Result<Arc<Self>, String> {
        private_directory(&root)?;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).read(true).write(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let lock = options
            .open(root.join("account.lock"))
            .map_err(|_| "Cannot open ChatGPT account lock")?;
        lock.try_lock_exclusive()
            .map_err(|_| "The Pipeline ChatGPT account is in use by another Pipeline process")?;
        for name in ["codex", "empty"] {
            private_directory(&root.join(name))?;
        }
        private_write(
            &root.join("codex/config.toml"),
            b"cli_auth_credentials_store = \"file\"\nmodel_provider = \"openai\"\n",
        )?;
        let choices = credentials::migrate(&root, &user_home, None)?;
        let native = launch_native(&root, EPOCH.fetch_add(1, Ordering::Relaxed)).await?;
        Ok(Arc::new(Self {
            native: Mutex::new(native),
            root,
            user_home,
            activity: Arc::new(RwLock::new(())),
            serial: Mutex::new(()),
            clients: Mutex::new(Vec::new()),
            pending: Mutex::new(None),
            choices: Mutex::new(choices),
            login_error: Mutex::new(None),
            events: tokio::sync::broadcast::channel(16).0,
            login_active: std::sync::atomic::AtomicBool::new(false),
            _lock: lock,
        }))
    }

    pub async fn client(&self) -> Result<AppServerClient, String> {
        let mut native = self.native.lock().await;
        if native.client.is_closed() {
            if self.login_active.load(Ordering::Acquire) {
                return Err("ChatGPT sign-in disconnected; wait for cleanup and try again".into());
            }
            native.shutdown().await;
            *native = launch_native(&self.root, EPOCH.fetch_add(1, Ordering::Relaxed)).await?;
        }
        Ok(native.client.clone())
    }

    pub async fn status(&self, refresh: bool) -> Result<Status, String> {
        let _lease = if refresh {
            Some(self.activity.try_read().map_err(|_| BUSY)?)
        } else {
            None
        };
        let client = self.client().await?;
        // Managed refreshes are serialized with external-token refresh requests.
        let _serial = self.serial.lock().await;
        let account = match client.account_state(refresh).await {
            Ok(account) => account,
            Err(_) => {
                if refresh {
                    self.native.lock().await.terminate().await;
                }
                return Err("Cannot check the shared ChatGPT account".into());
            }
        };
        let pending = self.pending.lock().await;
        Ok(Status {
            account,
            epoch: client.epoch(),
            version: self.native.lock().await.version.clone(),
            login_in_progress: pending.is_some(),
            pending_login: pending.as_ref().map(|p| AccountLogin {
                login_id: p.id.clone(),
                epoch: p.epoch,
            }),
            existing_accounts: self.choices.lock().await.clone(),
            error: self.login_error.lock().await.clone(),
        })
    }

    /// Read leases span complete native work, including owner-specific cleanup.
    pub async fn lease(
        self: &Arc<Self>,
        client: &AppServerClient,
    ) -> Result<OwnedRwLockReadGuard<()>, String> {
        let permit = self.activity.clone().try_read_owned().map_err(|_| BUSY)?;
        self.install(client, true).await?;
        Ok(permit)
    }

    /// Register an idle execution runtime. Signed-out startup is valid.
    pub async fn attach(self: &Arc<Self>, client: &AppServerClient) -> Result<(), String> {
        let Ok(_permit) = self.activity.clone().try_read_owned() else {
            return Ok(()); // Idle ephemeral startup is valid during browser login.
        };
        self.install(client, false).await
    }

    async fn install(
        self: &Arc<Self>,
        runtime: &AppServerClient,
        required: bool,
    ) -> Result<(), String> {
        let _serial = self.serial.lock().await;
        let owner = self.client().await?;
        let state = owner
            .account_state(false)
            .await
            .map_err(|_| "Cannot check ChatGPT sign-in")?;
        if state.status != AccountStatus::Chatgpt {
            if required {
                return Err(
                    "Sign in to ChatGPT in Settings to use Conversations and Reviews".into(),
                );
            }
            return Ok(());
        }
        let auth = credentials::read(&self.root.join("codex/auth.json"))?
            .ok_or("ChatGPT sign-in is unavailable; sign in again")?;
        let mut clients = self.clients.lock().await;
        clients.retain(|client| !client.is_closed());
        if !clients.iter().any(|client| client.same_connection(runtime)) {
            clients.push(runtime.clone());
        }
        drop(clients);
        runtime.set_auth_refresh(Some(Arc::new(refresh::Refresh {
            owner: Arc::downgrade(self),
            identity: auth.identity.clone(),
            account_id: auth.account_id.clone(),
        })));
        let mut params = auth.external_tokens();
        params["type"] = "chatgptAuthTokens".into();
        // Never propagate native error data from a credential-bearing RPC.
        let result = runtime
            .request(
                "account/login/start",
                params,
                super::DEFAULT_REQUEST_TIMEOUT,
            )
            .await;
        if !matches!(result, Ok(ref value) if value["type"] == "chatgptAuthTokens") {
            let _ = runtime.close_writer().await;
            runtime.fail(
                "Shared ChatGPT authentication could not be confirmed; reconnect this runtime",
                false,
            );
            return Err("Cannot connect this runtime to the shared ChatGPT account; a compatible Codex installation is required".into());
        }
        Ok(())
    }

    async fn clear_runtimes(&self) -> Result<(), String> {
        let mut clients = self.clients.lock().await;
        clients.retain(|client| !client.is_closed());
        for client in clients.iter() {
            client.set_auth_refresh(None);
            if client.logout().await.is_err() {
                let _ = client.close_writer().await;
                client.fail("ChatGPT account changed; reconnect this runtime", false);
            }
        }
        Ok(())
    }

    pub async fn begin_login(self: &Arc<Self>) -> Result<(AccountLogin, LoginStart), String> {
        let permit = self.activity.clone().try_write_owned().map_err(|_| BUSY)?;
        self.clear_runtimes().await?;
        let client = self.client().await?;
        let mut events = client.subscribe();
        *self.login_error.lock().await = None;
        // Choosing a new browser login deliberately resolves any migration choice.
        private_write(&self.root.join("account-migration-v1"), b"complete")?;
        self.choices.lock().await.clear();
        let login = match client.login_start().await {
            Ok(login) => login,
            Err(_) => {
                self.native.lock().await.terminate().await;
                return Err("Could not start ChatGPT sign-in".into());
            }
        };
        if !super::account::is_safe_auth_url(&login.auth_url) {
            if client.login_cancel(&login.login_id).await.is_err() {
                self.native.lock().await.terminate().await;
            }
            return Err("Codex returned an unsafe authentication URL".into());
        }
        let (cancel, mut canceled) = oneshot::channel();
        let result = AccountLogin {
            login_id: login.login_id.clone(),
            epoch: client.epoch(),
        };
        self.login_active.store(true, Ordering::Release);
        *self.pending.lock().await = Some(PendingLogin {
            id: result.login_id.clone(),
            epoch: result.epoch,
            cancel,
        });
        let _ = self.events.send(());
        let owner = self.clone();
        let id = result.login_id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let completed = tokio::time::timeout(Duration::from_secs(300), async {
                loop {
                    tokio::select! {
                        _ = &mut canceled => return true,
                        event = events.recv() => match event {
                            Ok(NormalizedEvent::AccountLoginCompleted { login_id:Some(login_id), success, .. }) if login_id == id => {
                                if !success { *owner.login_error.lock().await = Some("ChatGPT sign-in did not complete. Try again.".into()); }
                                return true;
                            }
                            Ok(NormalizedEvent::ConnectionClosed { .. }) | Err(_) => return false,
                            _ => {}
                        }
                    }
                }
            }).await.unwrap_or(false);
            if !completed {
                *owner.login_error.lock().await =
                    Some("ChatGPT sign-in timed out or disconnected. Try again.".into());
                if client.login_cancel(&id).await.is_err() {
                    owner.native.lock().await.terminate().await;
                }
            }
            owner.pending.lock().await.take();
            owner.login_active.store(false, Ordering::Release);
            drop(_permit);
            let _ = owner.events.send(());
        });
        Ok((result, login))
    }

    pub async fn cancel_login(&self, id: &str, epoch: Option<u64>) -> Result<bool, String> {
        let mut pending = self.pending.lock().await;
        let p = pending
            .as_ref()
            .filter(|p| p.id == id && epoch.is_none_or(|epoch| epoch == p.epoch))
            .ok_or("This sign-in is no longer active; refresh the ChatGPT account")?;
        let client = self.client().await?;
        if client.epoch() != p.epoch {
            return Err("The sign-in connection changed; refresh the ChatGPT account".into());
        }
        let result = match client.login_cancel(id).await {
            Ok(value) => value,
            Err(_) => {
                self.native.lock().await.terminate().await;
                return Err("Could not confirm sign-in cancellation; reconnect ChatGPT".into());
            }
        };
        if let Some(p) = pending.take() {
            let _ = p.cancel.send(());
        }
        Ok(result)
    }

    pub async fn logout(&self) -> Result<(), String> {
        let _permit = self.activity.try_write().map_err(|_| BUSY)?;
        let _serial = self.serial.lock().await;
        self.clear_runtimes().await?;
        let client = self.client().await?;
        match client.logout().await {
            Ok(()) => {
                let _ = self.events.send(());
                Ok(())
            }
            Err(_) => {
                self.native.lock().await.terminate().await;
                Err("Could not confirm ChatGPT sign-out; check the account again".into())
            }
        }
    }

    pub async fn select_existing(&self, source: &str) -> Result<(), String> {
        let _permit = self.activity.try_write().map_err(|_| BUSY)?;
        let _serial = self.serial.lock().await;
        if !self
            .choices
            .lock()
            .await
            .iter()
            .any(|account| account.source == source)
        {
            return Err("That saved sign-in is no longer available".into());
        }
        self.clear_runtimes().await?;
        let mut native = self.native.lock().await;
        native.shutdown().await;
        credentials::migrate(&self.root, &self.user_home, Some(source))?;
        *native = launch_native(&self.root, EPOCH.fetch_add(1, Ordering::Relaxed)).await?;
        self.choices.lock().await.clear();
        let _ = self.events.send(());
        Ok(())
    }
}

pub(crate) async fn shutdown() {
    if let Some(slot) = ACCOUNT.get() {
        if let Some(account) = slot.lock().await.take() {
            account.native.lock().await.shutdown().await;
        }
    }
}
