//! Workspace-owned Playwright process with managed first-use setup;
//! conversations never download or execute model-selected packages or scripts.
mod setup;
use super::store::{WorkbenchError, WorkbenchResult};
use serde::Serialize;
use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio, sync::OnceLock, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Mutex,
};

const VERSION: &str = "1.64.1";
static PROCESS: OnceLock<Mutex<Option<Process>>> = OnceLock::new();
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

#[tauri::command]
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
    workbench_browser_install().await.map_err(err)?;
    let _installation = INSTALL
        .get_or_init(|| Mutex::new(()))
        .try_lock()
        .map_err(|_| err("Browser setup is in progress; wait for it to finish"))?;
    let mut owner = process().lock().await;
    if owner.as_ref().is_some_and(|p| p.session != session) {
        owner.take();
    }
    if owner.is_none() {
        *owner = Some(Process::start(session).await?);
    }
    let result = tokio::time::timeout(
        Duration::from_secs(45),
        owner.as_mut().unwrap().request(args),
    )
    .await;
    let value = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            owner.take();
            return Err(error);
        }
        Err(_) => {
            owner.take();
            return Err(err("Browser action timed out"));
        }
    };
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
        vec![json!({"type":"inputText","text":result.to_string()})]
    };
    Ok(json!({"success":true,"contentItems":content}))
}

pub(crate) async fn stop() {
    if let Some(mut process) = process().lock().await.take() {
        let _ = process.stdin.shutdown().await;
        crate::commands::lifecycle::kill_independent_process(process.pid);
        let _ = process.child.kill().await;
        let _ = tokio::time::timeout(Duration::from_secs(5), process.child.wait()).await;
    }
}
