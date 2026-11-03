//! Workspace-owned Playwright process with managed first-use setup;
//! conversations never download or execute model-selected packages or scripts.
//! The window outlives its turn briefly, so the researcher can finish a sign-in
//! the assistant handed over; app exit closes it at once, and so does a lost
//! ChatGPT connection unless a Claude or API turn is running.
mod setup;
use super::store::{WorkbenchError, WorkbenchResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Mutex,
};

const VERSION: &str = "1.64.1";
/// How long the window stays open after a turn ends, so the researcher can
/// finish a sign-in the assistant asked for and send their next message.
const OPEN_AFTER_TURN: Duration = Duration::from_secs(10 * 60);
/// Tells the driver the turn is over, so it stops counting against the turn.
const RELEASE: &str = "release";
const RELEASE_LIMIT: Duration = Duration::from_secs(5);
const COMMAND_LIMIT: Duration = Duration::from_secs(45);
const SIGN_IN_NOTE: &str = "Sign-in and CAPTCHA belong to the researcher. To hand one over, ask them to finish it in this window and to message you when they are done, then end your reply. The window stays open for 10 minutes after your reply, and your next turn continues in it.";
static PROCESS: OnceLock<Mutex<Option<Process>>> = OnceLock::new();
/// Each new use or end of a turn replaces the close already scheduled.
static CLOSE_GENERATION: AtomicU64 = AtomicU64::new(0);
static INSTALL: OnceLock<Mutex<()>> = OnceLock::new();
fn process() -> &'static Mutex<Option<Process>> {
    PROCESS.get_or_init(|| Mutex::new(None))
}
fn root() -> Result<PathBuf, String> {
    Ok(crate::storage::local_root()?
        .join("native")
        .join("conversation-browser"))
}
fn err(e: impl std::fmt::Display) -> WorkbenchError {
    WorkbenchError::invalid(e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub installed: bool,
    pub node_available: bool,
    pub version: &'static str,
    pub platform_available: bool,
    pub setup_phase: Option<String>,
}

#[tauri::command(async)]
pub fn workbench_browser_status() -> Result<BrowserStatus, String> {
    let root = root()?;
    let installed = super::tool_setup::ready() && executable(&root).is_some();
    Ok(BrowserStatus {
        installed,
        node_available: super::tool_setup::ready(),
        version: VERSION,
        platform_available: super::tool_setup::supported(),
        setup_phase: super::tool_setup::phase().or_else(|| {
            BROWSER_PHASE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        }),
    })
}

static BROWSER_PHASE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
struct SetupProgress;
impl Drop for SetupProgress {
    fn drop(&mut self) {
        *BROWSER_PHASE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
fn browser_root(root: &std::path::Path) -> PathBuf {
    root.join(format!("browsers-{VERSION}"))
}
fn executable(root: &std::path::Path) -> Option<PathBuf> {
    use std::io::Read;
    let directory = browser_root(root);
    let file = crate::safety::open_regular_file(&directory.join("complete")).ok()?;
    let mut value = String::new();
    file.take(8192).read_to_string(&mut value).ok()?;
    let relative = std::path::Path::new(&value);
    if !relative
        .components()
        .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let path = directory.join(relative);
    crate::safety::open_regular_file(&path).ok()?;
    Some(path)
}

#[tauri::command]
pub async fn workbench_browser_install() -> Result<BrowserStatus, String> {
    super::tool_setup::cancellable(async {
        let _guard = INSTALL.get_or_init(|| Mutex::new(())).lock().await;
        if workbench_browser_status()?.installed {
            return workbench_browser_status();
        }
        super::tool_setup::ensure().await?;
        if executable(&root()?).is_some() {
            return workbench_browser_status();
        }
        setup::install(root()?, &super::tool_setup::root()?).await?;
        workbench_browser_status()
    })
    .await
}

struct Process {
    session: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
}
impl Drop for Process {
    fn drop(&mut self) {
        crate::commands::lifecycle::kill_independent_process(self.pid);
        let _ = self.child.start_kill();
        crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
    }
}
impl Process {
    async fn start(session: &str) -> WorkbenchResult<Self> {
        if !workbench_browser_status().map_err(err)?.installed {
            return Err(err(
                "Browser setup is incomplete. Retry in Assistant settings",
            ));
        }
        let root = root().map_err(err)?;
        use sha2::{Digest, Sha256};
        let profile = root
            .join("profiles")
            .join(format!("{:x}", Sha256::digest(session)));
        crate::agent_runtime::codex::session::private_directory(&profile).map_err(err)?;
        let mut command = std::process::Command::new(super::tool_setup::node().map_err(err)?);
        command
            .arg(super::tool_setup::root().map_err(err)?.join("driver.mjs"))
            .arg(profile);
        // Keep display/runtime variables, while removing model/provider and cloud secrets.
        for (key, _) in std::env::vars_os() {
            let upper = key.to_string_lossy().to_ascii_uppercase();
            if upper.contains("TOKEN")
                || upper.contains("SECRET")
                || upper.contains("API_KEY")
                || upper.starts_with("AWS_")
                || upper.starts_with("ANTHROPIC_")
                || upper.starts_with("OPENAI_")
                || upper.starts_with("CLAUDE_")
                || upper.ends_with("_PROXY")
                || upper.starts_with("NODE_")
            {
                command.env_remove(key);
            }
        }
        command
            .env("PLAYWRIGHT_BROWSERS_PATH", browser_root(&root))
            .env("PATH", crate::env::full_path())
            .current_dir(&root);
        crate::process::configure_isolation(&mut command);
        let mut command = tokio::process::Command::from(command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(err)?;
        let pid = child
            .id()
            .ok_or_else(|| err("Browser did not report its process ID"))?;
        crate::commands::lifecycle::register_independent_child_pid(pid);
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| err("Browser input is unavailable"))?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| err("Browser output is unavailable"))?,
        );
        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut bytes = [0; 4096];
                while matches!(stderr.read(&mut bytes).await,Ok(n) if n>0) {}
            });
        }
        let mut process = Self {
            session: session.into(),
            child,
            stdin,
            stdout,
            pid,
        };
        let ready = tokio::time::timeout(Duration::from_secs(30), process.read())
            .await
            .map_err(|_| err("Browser startup timed out"))??;
        if ready["ready"] != true {
            return Err(err("Browser failed to initialize"));
        }
        Ok(process)
    }
    async fn read(&mut self) -> WorkbenchResult<Value> {
        let line = crate::pipeline::logging::next_bounded_line(&mut self.stdout, 2 * 1024 * 1024)
            .await
            .map_err(err)?
            .ok_or_else(|| err("Browser closed; check its system dependencies"))?;
        if line.truncated {
            return Err(err("Browser response exceeded 2 MiB"));
        }
        serde_json::from_str(&line.text).map_err(err)
    }
    async fn request(&mut self, args: Value) -> WorkbenchResult<Value> {
        let mut bytes = serde_json::to_vec(&args).map_err(err)?;
        bytes.push(b'\n');
        self.stdin.write_all(&bytes).await.map_err(err)?;
        self.stdin.flush().await.map_err(err)?;
        self.read().await
    }
}

pub(crate) async fn call(session: &str, args: Value) -> WorkbenchResult<Value> {
    if args["action"] == RELEASE {
        return Err(err("Unsupported browser action"));
    }
    workbench_browser_install().await.map_err(err)?;
    let _installation = INSTALL
        .get_or_init(|| Mutex::new(()))
        .try_lock()
        .map_err(|_| err("Browser setup is in progress; wait for it to finish"))?;
    let mut owner = process().lock().await;
    CLOSE_GENERATION.fetch_add(1, Ordering::AcqRel);
    if owner.as_ref().is_some_and(|p| p.session != session) {
        owner.take();
    }
    let mut value = request(&mut owner, session, args.clone()).await?;
    // The researcher can quit the browser while its driver lives on. A refused
    // command did nothing, so a new window can run it.
    if refused_by_closed_window(&value) {
        owner.take();
        value = request(&mut owner, session, args).await?;
    }
    if value["ok"] != true {
        return Err(err(value["error"]
            .as_str()
            .unwrap_or("Browser action failed")));
    }
    let result = &value["result"];
    let content = if let Some(image) = result["image"].as_str() {
        vec![
            json!({"type":"inputText","text":json!({"url":result["url"]}).to_string()}),
            json!({"type":"inputImage","imageUrl":image}),
        ]
    } else {
        vec![json!({"type":"inputText","text":with_sign_in_note(result.clone()).to_string()})]
    };
    Ok(json!({"success":true,"contentItems":content}))
}

/// One command and its reply. The process leaves the slot for the exchange and
/// returns only with a complete reply: a line the driver answers after its
/// caller is gone would otherwise be read as the answer to the next command. A
/// call dropped by Stop or the end of a turn drops the process here, which
/// closes the window, rather than leaving it a reply behind.
async fn request(
    owner: &mut Option<Process>,
    session: &str,
    args: Value,
) -> WorkbenchResult<Value> {
    let mut process = match owner.take() {
        Some(process) => process,
        None => Process::start(session).await?,
    };
    let reply = tokio::time::timeout(COMMAND_LIMIT, process.request(args))
        .await
        .map_err(|_| err("Browser action timed out"))??;
    *owner = Some(process);
    Ok(reply)
}

/// The driver's own verdict. One closed popup is not a closed browser, and
/// starting another would close the page the researcher signed in on.
fn refused_by_closed_window(value: &Value) -> bool {
    value["ok"] != true && value["windowClosed"] == true
}

fn with_sign_in_note(mut snapshot: Value) -> Value {
    if snapshot.is_object() {
        snapshot["signIn"] = json!(SIGN_IN_NOTE);
    }
    snapshot
}

/// Leaves the window for the researcher when a turn ends. The assistant's
/// request to sign in is the last thing a turn says, so closing here would take
/// the window away before they read it. Nothing can drive it until the next
/// turn, and the page's network rules stay in force.
pub(crate) async fn release_after_turn() {
    release_window(RELEASE_LIMIT).await;
    close_after(OPEN_AFTER_TURN).await;
}

/// A driver that won't answer can't be kept: it may be a reply behind.
async fn release_window(limit: Duration) {
    let mut owner = process().lock().await;
    if let Some(mut process) = owner.take() {
        let told = process.request(json!({ "action": RELEASE }));
        if matches!(tokio::time::timeout(limit, told).await, Ok(Ok(_))) {
            *owner = Some(process);
        }
    }
}

async fn close_after(delay: Duration) {
    let generation = CLOSE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    if process().lock().await.is_none() {
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if CLOSE_GENERATION.load(Ordering::Acquire) == generation {
            stop().await;
        }
    });
}

pub(crate) async fn stop() {
    CLOSE_GENERATION.fetch_add(1, Ordering::AcqRel);
    if let Some(mut process) = process().lock().await.take() {
        let _ = process.stdin.shutdown().await;
        crate::commands::lifecycle::kill_independent_process(process.pid);
        let _ = process.child.kill().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), process.child.wait()).await;
    }
}

#[cfg(all(test, unix))]
mod tests;
