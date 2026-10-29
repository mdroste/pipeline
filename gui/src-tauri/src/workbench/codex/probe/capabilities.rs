//! Verify conversation-local web/network overrides on start and resume without
//! sending a model prompt or using the researcher's credentials.
use super::*;

pub(super) async fn verify(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    root: &Path,
) -> Result<(), String> {
    for (web, network, profile) in [
        (true, false, "workbench-inspect"),
        (false, true, "workbench-inspect-network"),
        (true, true, "workbench-edit-network"),
    ] {
        let policy = crate::workbench::capabilities::ConversationCapabilities {
            command_domains: vec!["example.org".into()],
            browser: false,
            shell_write: profile.contains("edit"),
        };
        let config = policy.codex_config(web, network, profile);
        let id = json!(format!("capabilities-{profile}-start"));
        let mut request = thread_start_request(id.clone(), root);
        request["params"]["permissions"] = json!(profile);
        request["params"]["config"] = config.clone();
        send(stdin, &request).await?;
        let started = response_for(reader, &id).await?;
        if started["activePermissionProfile"]["id"] != profile {
            return Err(format!(
                "Conversation capability profile changed: {started}"
            ));
        }
        let thread = started["thread"]["id"]
            .as_str()
            .ok_or("Capability probe thread missing")?;
        let id = json!(format!("capabilities-{profile}-inject"));
        send(stdin,&json!({"method":"thread/inject_items","id":id,"params":{"threadId":thread,"items":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Synthetic capability fixture. No response requested."}]}]}})).await?;
        response_for(reader, &id).await?;
        let id = json!(format!("capabilities-{profile}-resume"));
        send(stdin,&json!({"method":"thread/resume","id":id,"params":{"threadId":thread,"permissions":profile,"cwd":root,"runtimeWorkspaceRoots":[root],"config":config}})).await?;
        let resumed = response_for(reader, &id).await?;
        if resumed["activePermissionProfile"]["id"] != profile {
            return Err("Conversation capability profile changed on resume".into());
        }
    }
    if std::env::var_os("PIPELINE_PROBE_NETWORK").is_some() {
        verify_network(stdin, reader, root).await?;
    }
    Ok(())
}

async fn verify_network(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    root: &Path,
) -> Result<(), String> {
    let curl = if cfg!(windows) {
        "curl.exe"
    } else {
        "/usr/bin/curl"
    };
    for (label, url, bypass, allowed) in [
        ("allowed", "https://example.org", false, true),
        ("denied", "https://example.com", false, false),
        ("private", "http://127.0.0.1", false, false),
        ("bypass", "https://example.org", true, false),
    ] {
        let id = json!(format!("capability-network-{label}"));
        let mut command = vec![
            curl.into(),
            "--fail".into(),
            "--silent".into(),
            "--show-error".into(),
            "--max-time".into(),
            "10".into(),
        ];
        if bypass {
            command.extend(["--noproxy".into(), "*".into()]);
        }
        command.push(url.into());
        let mut request = command_exec_request(id.clone(), command, root);
        request["params"]["permissionProfile"] = json!("workbench-inspect-network");
        send(stdin, &request).await?;
        let result = response_for(reader, &id).await?;
        if (result["exitCode"] == 0) != allowed {
            return Err(format!("Command domain policy failed {label}: {result}"));
        }
    }
    Ok(())
}
