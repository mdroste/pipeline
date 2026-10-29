//! One independently supervised CLI process per Workspace turn. No Review state.
use super::*;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

pub(super) struct Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
    _directory: tempfile::TempDir,
}

impl Process {
    pub async fn start(args: &[String]) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut args = args.to_vec();
        // macOS has a small argv budget. Keep large profile/context text in
        // private files; prompts themselves travel over the framed stdin pipe.
        for flag in ["--append-system-prompt", "--system-prompt"] {
            if let Some(i) = args.iter().position(|s| s == flag) {
                let path = directory.path().join(format!("{}.txt", &flag[2..]));
                std::fs::write(&path, &args[i + 1]).map_err(|e| e.to_string())?;
                args[i] = format!("{flag}-file");
                args[i + 1] = path.to_string_lossy().into_owned();
            }
        }
        let mut command =
            crate::agent_runtime::claude::subscription_command(&args, directory.path().to_str())
                .await?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start Claude Code: {e}"))?;
        let pid = child
            .id()
            .ok_or("Claude Code did not report a process ID")?;
        crate::commands::lifecycle::register_independent_child_pid(pid);
        let stdin = child
            .stdin
            .take()
            .ok_or("Claude Code stdin is unavailable")?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("Claude Code stdout is unavailable")?,
        );
        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move {
                // Drain without retaining credentials, prompt text, or unbounded logs.
                let mut buffer = [0; 8192];
                while matches!(stderr.read(&mut buffer).await, Ok(n) if n > 0) {}
            });
        }
        Ok(Self {
            child,
            stdin,
            stdout,
            pid,
            _directory: directory,
        })
    }

    pub async fn write(&mut self, value: &Value) -> Result<(), String> {
        let mut text = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if text.len() > 8 * 1024 * 1024 {
            return Err("Claude input exceeded its 8 MiB limit".into());
        }
        text.push(b'\n');
        self.stdin
            .write_all(&text)
            .await
            .map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())
    }

    pub async fn read(&mut self) -> Result<Value, String> {
        let line = crate::pipeline::logging::next_bounded_line(&mut self.stdout, 2 * 1024 * 1024).await.map_err(|e| e.to_string())?
            .ok_or("Claude Code closed its output before completing the turn. Check its subscription sign-in and CLI version.")?;
        if line.truncated {
            return Err("Claude output exceeded its frame limit".into());
        }
        serde_json::from_str(&line.text)
            .map_err(|_| "Claude Code emitted invalid stream-json".into())
    }

    pub async fn initialize(&mut self, specs: &[Value]) -> Result<Value, String> {
        tokio::time::timeout(Duration::from_secs(30), async {
            self.write(&json!({"type":"control_request","request_id":"pipeline-init","request":{"subtype":"initialize","hooks":{}}})).await?;
            for _ in 0..128 {
                let value = self.read().await?;
                if value["type"] == "control_response" && value["response"]["request_id"] == "pipeline-init" {
                    if value["response"]["subtype"] != "success" { return Err("Claude Code initialization was rejected".into()); }
                    return Ok(value["response"]["response"].clone());
                }
                if value["type"] == "control_request" {
                    self.control(&value, specs, None).await?;
                }
            }
            Err("Claude Code initialization exceeded its message limit".into())
        }).await.map_err(|_| "Claude Code initialization timed out".to_string())?
    }

    async fn control(
        &mut self,
        value: &Value,
        specs: &[Value],
        turn: Option<&Turn>,
    ) -> Result<(), String> {
        let request = &value["request"];
        let response = match request["subtype"].as_str() {
            Some("mcp_message") => tools::message(request, specs, turn).await?,
            // No native tool or expanded permissions are available in this adapter.
            Some("can_use_tool") => {
                json!({"behavior":"deny","message":"Use Pipeline's scoped project tools; native tools are disabled."})
            }
            _ => return Err("Claude requested an unsupported control operation".into()),
        };
        self.write(&protocol::control_response(&value["request_id"], response))
            .await
    }

    pub async fn stop(&mut self) {
        let _ = self.stdin.shutdown().await;
        if self.pid > 0 {
            crate::commands::lifecycle::kill_independent_process(self.pid);
            let _ = self.child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
            self.pid = 0;
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.pid > 0 {
            crate::commands::lifecycle::kill_independent_process(self.pid);
            let _ = self.child.start_kill();
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
        }
    }
}

pub(super) async fn drive(
    process: &mut Process,
    app: &tauri::AppHandle,
    turn: &Turn,
    user_text: &str,
    prompt: &protocol::Prompt,
    specs: Vec<Value>,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let work = async {
        turn.emit(
            app,
            NormalizedEvent::TurnStarted {
                epoch: turn.epoch,
                thread_id: turn.thread.clone(),
                turn_id: turn.id.clone(),
                turn: json!({"id":turn.id,"status":"inProgress"}),
            },
        )
        .await?;
        turn.item(app, "user", "userMessage", user_text).await?;
        process.write(&json!({"type":"user","message":{"role":"user","content":prompt.text},"parent_tool_use_id":null,"session_id":""})).await?;
        let mut bytes = 0usize;
        let mut messages = 0;
        let mut calls = 0;
        loop {
            let value = process.read().await?;
            bytes += value.to_string().len();
            if bytes > 64 * 1024 * 1024 {
                return Err("Claude turn exceeded its output budget".into());
            }
            match value["type"].as_str() {
                Some("control_request") => {
                    calls += 1;
                    if calls > 128 {
                        return Err("Claude turn exceeded its tool/control request budget".into());
                    }
                    process.control(&value, &specs, Some(turn)).await?;
                }
                Some("stream_event") if value["parent_tool_use_id"].is_null() => {
                    if let Some(delta) = value["event"]["delta"]["text"].as_str() {
                        let _ = app.emit(
                            "workbench:event",
                            NormalizedEvent::AgentMessageDelta {
                                epoch: turn.epoch,
                                thread_id: turn.thread.clone(),
                                turn_id: turn.id.clone(),
                                item_id: format!("{}-assistant-{messages}", turn.id),
                                delta: delta.into(),
                            },
                        );
                    }
                }
                Some("assistant") if value["parent_tool_use_id"].is_null() => {
                    let text = value["message"]["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|part| part["type"] == "text")
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        turn.item(app, &format!("assistant-{messages}"), "agentMessage", &text)
                            .await?;
                        messages += 1;
                    }
                }
                Some("result") => {
                    if value["is_error"] != false || value["subtype"] != "success" {
                        return Err(value["result"].as_str().map(str::to_owned).unwrap_or_else(
                            || format!("Claude run failed: {}", value["subtype"]),
                        ));
                    }
                    if messages == 0 {
                        let text = value["result"]
                            .as_str()
                            .ok_or("Claude returned no assistant response")?;
                        turn.item(app, "assistant-0", "agentMessage", text).await?;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    };
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(900), work) => result.map_err(|_| "Claude turn timed out after 15 minutes".to_string())?,
        _ = cancel.changed() => Err("interrupted".into()),
    }
}

#[tauri::command]
pub async fn workbench_claude_model_catalog() -> Result<super::super::codex::ModelCatalog, String> {
    let args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--safe-mode",
        "--tools",
        "",
        "--no-session-persistence",
    ]
    .map(str::to_owned);
    let mut process = Process::start(&args).await?;
    let result = process.initialize(&[]).await;
    process.stop().await;
    let initialization = result?;
    protocol::validate_model(&initialization, None, None)?;
    let models = initialization["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| {
            let name = item["value"].as_str()?;
            if name == "default" {
                return None;
            }
            Some(super::super::codex::WorkspaceModel {
                id: name.into(),
                model: name.into(),
                display_name: item["displayName"].as_str().unwrap_or(name).into(),
                description: item["description"].as_str().unwrap_or("").into(),
                is_default: false,
                default_reasoning_effort: String::new(),
                supported_reasoning_efforts: item["supportedEffortLevels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|v| super::super::codex::ReasoningEffortOption {
                        reasoning_effort: v.into(),
                        description: v.into(),
                    })
                    .collect(),
            })
        })
        .collect();
    Ok(super::super::codex::ModelCatalog { models })
}
