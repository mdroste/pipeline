//! Legacy CLI context forks and their returned thread contract.

use crate::pipeline::claude::normalize_cli_root;

#[derive(Debug)]
pub(super) struct CodexForkSpec<'a> {
    pub(super) cwd: &'a str,
    pub(super) sandbox: &'static str,
    pub(super) model: Option<&'a str>,
}

pub(super) fn validate_codex_fork(
    result: &serde_json::Value,
    spec: &CodexForkSpec<'_>,
) -> Result<String, String> {
    let returned_cwd = result
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .and_then(normalize_cli_root)
        .ok_or_else(|| "Codex thread/fork returned no valid working directory".to_string())?;
    if returned_cwd != spec.cwd {
        return Err(format!(
            "Codex thread/fork did not preserve the requested working directory (requested {}, received {returned_cwd})",
            spec.cwd
        ));
    }

    let expected_policy = if spec.sandbox == "workspace-write" {
        "workspaceWrite"
    } else {
        "readOnly"
    };
    let sandbox = result
        .get("sandbox")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Codex thread/fork returned no sandbox policy".to_string())?;
    let returned_policy = sandbox
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if returned_policy != expected_policy {
        return Err(format!(
            "Codex thread/fork returned sandbox '{returned_policy}', expected '{expected_policy}'"
        ));
    }
    if sandbox.get("networkAccess").and_then(|v| v.as_bool()) == Some(true) {
        return Err("Codex thread/fork unexpectedly granted network access".to_string());
    }
    if spec.sandbox == "workspace-write" {
        if sandbox.get("excludeSlashTmp").and_then(|v| v.as_bool()) != Some(true)
            || sandbox.get("excludeTmpdirEnvVar").and_then(|v| v.as_bool()) != Some(true)
        {
            return Err(
                "Codex thread/fork did not preserve the requested workspace-write restrictions"
                    .to_string(),
            );
        }
        let writable_roots = sandbox
            .get("writableRoots")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "Codex thread/fork returned no writable-root policy".to_string())?;
        if !writable_roots.is_empty() {
            return Err(
                "Codex thread/fork unexpectedly granted additional writable roots".to_string(),
            );
        }
    }

    let roots = result
        .get("runtimeWorkspaceRoots")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Codex thread/fork returned no runtime workspace roots".to_string())?;
    let returned_roots = roots
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(normalize_cli_root)
        .collect::<Vec<_>>();
    if returned_roots != [spec.cwd.to_string()] {
        return Err(format!(
            "Codex thread/fork returned unexpected runtime workspace roots: {returned_roots:?}"
        ));
    }
    if let Some(model) = spec.model {
        if result.get("model").and_then(serde_json::Value::as_str) != Some(model) {
            return Err("Codex thread/fork did not preserve the requested model".to_string());
        }
    }

    result
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Codex thread/fork returned no child thread id".to_string())
}

pub(super) async fn fork_codex_thread(
    thread_id: &str,
    spec: &CodexForkSpec<'_>,
) -> Result<String, String> {
    let mut fork_params = serde_json::json!({
        "threadId": thread_id,
        "cwd": spec.cwd,
        "sandbox": spec.sandbox,
        "runtimeWorkspaceRoots": [spec.cwd],
    });
    if spec.sandbox == "workspace-write" {
        fork_params["config"] = serde_json::json!({
            "sandbox_workspace_write": {
                "exclude_slash_tmp": true,
                "exclude_tmpdir_env_var": true,
                "network_access": false
            }
        });
    }
    if let Some(model) = spec.model {
        fork_params["model"] = serde_json::Value::String(model.to_string());
    }
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "pipeline",
                    "title": "Pipeline",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": { "experimentalApi": true }
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "thread/fork",
            "params": fork_params
        }),
    ];
    let replies = crate::model_catalog::rpc_exchange(
        "codex",
        &["app-server".into(), "--listen".into(), "stdio://".into()],
        &requests,
    )
    .await?;
    let result = replies
        .get(1)
        .and_then(|reply| reply.get("result"))
        .ok_or_else(|| "Codex thread/fork returned no result".to_string())?;
    validate_codex_fork(result, spec)
}
