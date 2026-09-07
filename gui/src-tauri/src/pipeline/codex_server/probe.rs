//! No-login, no-model qualification of the installed Workflow runtime.
use super::{
    connection::{Connection, PROFILE},
    invocation::validate_thread,
    tools::HostTools,
};
use crate::agent_runtime::codex::{AccountStatus, DEFAULT_REQUEST_TIMEOUT};
use serde_json::{json, Value};

pub async fn run() -> Result<Value, String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let c = Connection::launch(root.path().join("runtime")).await?;
    let result = inspect(&c, root.path()).await;
    c.native.shutdown().await;
    result
}

async fn inspect(c: &Connection, root: &std::path::Path) -> Result<Value, String> {
    if c.native
        .client
        .account_state(false)
        .await
        .map_err(|e| e.to_string())?
        .status
        != AccountStatus::SignedOut
    {
        return Err("Temporary Workflow runtime inherited an account".into());
    }
    let tools = HostTools::new(&["Read"], &[], None)?;
    let thread = c
        .native
        .client
        .request(
            "thread/start",
            json!({
                "cwd":c.cwd,"runtimeWorkspaceRoots":[],"permissions":PROFILE,
                "approvalPolicy":"never","approvalsReviewer":"user","modelProvider":"openai",
                "allowProviderModelFallback":false,"ephemeral":false,
                "developerInstructions":"Qualification fixture. No model response requested.",
                "dynamicTools":tools.declarations(),"config":{"web_search":"disabled"},
                "serviceName":"pipeline_workflows"
            }),
            DEFAULT_REQUEST_TIMEOUT,
        )
        .await
        .map_err(|e| e.to_string())?;
    validate_thread(&thread, &c.cwd, None).map_err(|e| format!("{e}: {thread}"))?;
    let id = thread["thread"]["id"].as_str().ok_or("No thread id")?;
    c.native.client.request("thread/inject_items", json!({"threadId":id,"items":[{
        "type":"message","role":"user","content":[{"type":"input_text","text":"Qualification fixture; no response requested."}]
    }]}), DEFAULT_REQUEST_TIMEOUT).await.map_err(|e| e.to_string())?;
    let read = c
        .native
        .client
        .request(
            "thread/read",
            json!({"threadId":id,"includeTurns":true}),
            DEFAULT_REQUEST_TIMEOUT,
        )
        .await
        .map_err(|e| e.to_string())?;
    if read["thread"]["id"] != id {
        return Err("Thread read returned the wrong identity".into());
    }
    let sentinel = root.join("sibling.txt");
    std::fs::write(&sentinel, "workflow-probe-private-sibling").map_err(|e| e.to_string())?;
    // A harmless command must launch successfully before denial results count.
    let baseline = exec(c, baseline_command()).await?;
    if baseline["exitCode"] != 0 {
        return Err(format!("Sandbox baseline failed: {baseline}"));
    }
    for path in [&sentinel, &c.home.join("config.toml")] {
        let denied = exec(c, read_command(path)).await?;
        if denied["exitCode"] == 0 {
            return Err(format!(
                "Native sandbox could read a private fixture: {}",
                path.display()
            ));
        }
    }
    let destination = c.cwd.join("forbidden.txt");
    let denied = exec(c, write_command(&destination)).await?;
    if denied["exitCode"] == 0 || destination.exists() {
        return Err("Native sandbox could write the empty working directory".into());
    }
    Ok(json!({
        "schema_version":1,"runtime_version":c.native.version,
        "platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "isolated_home":true,"ambient_account_absent":true,
        "thread_contract":true,"dynamic_tools_accepted":true,"thread_read":true,
        "sandbox_command_baseline":true,"private_reads_denied":true,"native_writes_denied":true,
        "authenticated_model_turn":false,"browser_login":false,
        "model_reported":thread["model"],"permission_profile":PROFILE
    }))
}

async fn exec(c: &Connection, command: Vec<String>) -> Result<Value, String> {
    c.native.client.request("command/exec", json!({"command":command,"cwd":c.cwd,"permissionProfile":PROFILE,"timeoutMs":5000,"outputBytesCap":4096}), DEFAULT_REQUEST_TIMEOUT).await.map_err(|e| e.to_string())
}
#[cfg(unix)]
fn baseline_command() -> Vec<String> {
    vec!["/usr/bin/true".into()]
}
#[cfg(unix)]
fn read_command(path: &std::path::Path) -> Vec<String> {
    vec!["/bin/cat".into(), path.display().to_string()]
}
#[cfg(unix)]
fn write_command(path: &std::path::Path) -> Vec<String> {
    vec!["/usr/bin/touch".into(), path.display().to_string()]
}
#[cfg(windows)]
fn baseline_command() -> Vec<String> {
    vec!["cmd.exe".into(), "/d".into(), "/c".into(), "exit 0".into()]
}
#[cfg(windows)]
fn read_command(path: &std::path::Path) -> Vec<String> {
    vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-Command".into(),
        format!(
            "$ErrorActionPreference='Stop'; Get-Content -LiteralPath '{}'",
            path.display().to_string().replace('\'', "''")
        ),
    ]
}
#[cfg(windows)]
fn write_command(path: &std::path::Path) -> Vec<String> {
    vec![
        "powershell.exe".into(),
        "-NoProfile".into(),
        "-Command".into(),
        format!(
            "$ErrorActionPreference='Stop'; Set-Content -LiteralPath '{}' -Value probe",
            path.display().to_string().replace('\'', "''")
        ),
    ]
}
