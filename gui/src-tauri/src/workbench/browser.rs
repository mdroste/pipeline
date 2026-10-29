//! Optional Workspace-owned Playwright process. Installation is a user action;
//! conversations never download or execute model-selected packages or scripts.
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
}

#[tauri::command]
pub fn workbench_browser_status() -> Result<BrowserStatus, String> {
    let root = root()?;
    let read_json = |path: PathBuf| {
        let metadata = std::fs::symlink_metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 {
            return None;
        }
        std::fs::read(path)
            .ok()
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
    };
    let package = read_json(root.join("node_modules/playwright/package.json"));
    let marker = read_json(root.join("installed-version"));
    let ready = marker.is_some_and(|m| {
        m["version"] == VERSION
            && m["executable"].as_str().is_some_and(|path| {
                std::path::Path::new(path).starts_with(root.join("browsers"))
                    && std::path::Path::new(path).is_file()
            })
    });
    Ok(BrowserStatus {
        installed: package.is_some_and(|v| v["version"] == VERSION) && ready,
        node_available: crate::deps::resolve_command("node").is_some(),
        version: VERSION,
    })
}

#[tauri::command]
pub async fn workbench_browser_install() -> Result<BrowserStatus, String> {
    let _guard = INSTALL.get_or_init(|| Mutex::new(())).lock().await;
    stop().await;
    tokio::task::spawn_blocking(|| {
        let root=root()?;
        crate::agent_runtime::codex::session::private_directory(&root)?;
        let _=std::fs::remove_file(root.join("installed-version"));
        if std::fs::symlink_metadata(&root).map_err(|e|e.to_string())?.file_type().is_symlink() { return Err("Browser installation folder must not be a symlink".into()); }
        let npm=crate::deps::resolve_command("npm").ok_or("Install Node.js 22 or later to set up the browser")?;
        let node=crate::deps::resolve_command("node").ok_or("Install Node.js 22 or later to set up the browser")?;
        let mut command=npm.command(["install","--prefix",root.to_str().ok_or("Browser path is not UTF-8")?,"--no-audit","--no-fund","--ignore-scripts","--save-exact",&format!("playwright@{VERSION}")]);
        command.env("PATH",crate::env::full_path()).current_dir(&root);
        let result=crate::process::run_bounded_independent(&mut command,Duration::from_secs(300),64*1024)?;
        if !result.status.success() { return Err("Playwright installation failed; check internet access and your Node.js installation".into()); }
        let mut command=node.command([root.join("node_modules/playwright/cli.js").as_os_str(),std::ffi::OsStr::new("install"),std::ffi::OsStr::new("chromium")]);
        command.env("PLAYWRIGHT_BROWSERS_PATH",root.join("browsers")).env("PATH",crate::env::full_path()).current_dir(&root);
        let result=crate::process::run_bounded_independent(&mut command,Duration::from_secs(300),64*1024)?;
        if !result.status.success() { return Err("Chromium installation failed. On Linux, install Playwright's system dependencies and retry".into()); }
        let mut verify=node.command(["-e","const p=require('playwright').chromium.executablePath(); if(!require('node:fs').existsSync(p)) process.exit(1); process.stdout.write(p)"]);
        verify.env("PLAYWRIGHT_BROWSERS_PATH",root.join("browsers")).current_dir(&root);
        let result=crate::process::run_bounded_independent(&mut verify,Duration::from_secs(15),8192)?;
        if !result.status.success() || result.stdout_truncated { return Err("Chromium is missing after installation".into()); }
        let executable=String::from_utf8(result.stdout).map_err(|e|e.to_string())?;
        crate::agent_runtime::codex::session::private_write(&root.join("installed-version"),&serde_json::to_vec(&json!({"version":VERSION,"executable":executable})).map_err(|e|e.to_string())?)?;
        workbench_browser_status()
    }).await.map_err(|e|e.to_string())?
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
                "Install the browser in Assistant settings before using browser tools",
            ));
        }
        let root = root().map_err(err)?;
        // Generated from the installed application's source, never conversation text.
        crate::agent_runtime::codex::session::private_write(
            &root.join("driver.mjs"),
            include_str!("browser/driver.mjs").as_bytes(),
        )
        .map_err(err)?;
        crate::agent_runtime::codex::session::private_write(
            &root.join("public-request.mjs"),
            include_str!("browser/public-request.mjs").as_bytes(),
        )
        .map_err(err)?;
        use sha2::{Digest, Sha256};
        let profile = root
            .join("profiles")
            .join(format!("{:x}", Sha256::digest(session)));
        crate::agent_runtime::codex::session::private_directory(&profile).map_err(err)?;
        let node =
            crate::deps::resolve_command("node").ok_or_else(|| err("Node.js is unavailable"))?;
        let mut command = node.command([root.join("driver.mjs"), profile]);
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
            .env("PLAYWRIGHT_BROWSERS_PATH", root.join("browsers"))
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
