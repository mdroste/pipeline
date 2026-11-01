//! One independently supervised CLI process per Workspace turn. No Review state.
use super::super::turn_loop::{ModelClock, TurnEvents, HOST_TIME_LIMIT};
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
    pub(super) usage_account: Option<String>,
    wsl: Option<super::wsl::OwnedGroup>,
}

impl Process {
    /// `sandbox` adds Claude Code's command sandbox settings for a turn that
    /// offers the Bash tool. `working` replaces the process's temporary
    /// working directory when Bash may write in a lasting root.
    pub async fn start(
        args: &[String],
        sandbox: Option<&Value>,
        working: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let wsl = super::wsl::detect_async().await.environment()?;
        Self::start_in(args, sandbox, working, wsl).await
    }

    /// `start` for a turn that already asked whether WSL is available.
    pub async fn start_in(
        args: &[String],
        sandbox: Option<&Value>,
        working: Option<&std::path::Path>,
        wsl: Option<super::wsl::Environment>,
    ) -> Result<Self, String> {
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
        let cwd = working.unwrap_or(directory.path()).to_str();
        let (command, status, wsl) = if let Some(environment) = wsl {
            let sandbox = sandbox.cloned();
            let args = args.clone();
            let directory = directory.path().to_owned();
            let working = working.map(std::path::Path::to_owned);
            let (command, status, group) = tokio::task::spawn_blocking(move || {
                environment.prepare(sandbox.as_ref(), &args, &directory, working.as_deref())
            })
            .await
            .map_err(|e| e.to_string())??;
            (command, status, Some(group))
        } else {
            let (resolved, status) =
                crate::agent_runtime::claude::verified_subscription_with_status().await?;
            (
                crate::agent_runtime::claude::verified_sandboxed_command(
                    &resolved, sandbox, &args, cwd,
                ),
                status,
                None,
            )
        };
        Self::attach(command, directory, super::usage::account_key(&status), wsl)
    }

    /// Takes ownership of a prepared CLI command and its private folder.
    pub(super) fn attach(
        mut command: tokio::process::Command,
        directory: tempfile::TempDir,
        usage_account: Option<String>,
        wsl: Option<super::wsl::OwnedGroup>,
    ) -> Result<Self, String> {
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
            usage_account,
            wsl,
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
        self.read_sized().await.map(|(value, _)| value)
    }

    /// One frame and its size on the wire, which a turn counts against its budget.
    async fn read_sized(&mut self) -> Result<(Value, usize), String> {
        let line = crate::pipeline::logging::next_bounded_line(&mut self.stdout, 2 * 1024 * 1024).await.map_err(|e| e.to_string())?
            .ok_or("Claude Code closed its output before completing the turn. Check its subscription sign-in and CLI version.")?;
        if line.truncated {
            return Err("Claude output exceeded its frame limit".into());
        }
        serde_json::from_str(&line.text)
            .map(|value| (value, line.text.len()))
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

    pub(super) async fn control(
        &mut self,
        value: &Value,
        specs: &[Value],
        turn: Option<&Turn>,
    ) -> Result<(), String> {
        let answer = control_answer(value, specs, turn).await?;
        self.write(&answer).await
    }

    pub async fn stop(&mut self) {
        if let Some(group) = self.wsl.take() {
            let _ = tokio::task::spawn_blocking(move || group.stop()).await;
        }
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
        if let Some(group) = self.wsl.take() {
            group.stop();
        }
        if self.pid > 0 {
            crate::commands::lifecycle::kill_independent_process(self.pid);
            let _ = self.child.start_kill();
            crate::commands::lifecycle::unregister_independent_child_pid(self.pid);
        }
    }
}

/// Pipeline's control response frame for one of Claude's control requests.
async fn control_answer(
    value: &Value,
    specs: &[Value],
    turn: Option<&Turn>,
) -> Result<Value, String> {
    let request = &value["request"];
    let response = match request["subtype"].as_str() {
        Some("mcp_message") => tools::message(request, specs, turn).await?,
        // Only sandboxed commands are pre-approved; nothing may expand that.
        Some("can_use_tool") => {
            json!({"behavior":"deny","message":"Only sandboxed shell commands and Pipeline's scoped project tools are available."})
        }
        _ => return Err("Claude requested an unsupported control operation".into()),
    };
    Ok(protocol::control_response(&value["request_id"], response))
}

/// Model time allowed for one turn, including exchanges with Claude's stdin
/// and stdout. Time spent running a command in Claude's Bash tool or in
/// Pipeline's project tools, including a local job that a research run waits
/// for under its own profile timeout, is not counted.
const MODEL_TIME_LIMIT: Duration = Duration::from_secs(15 * 60);
const MODEL_TIMEOUT: &str = "Claude turn timed out after 15 minutes of model time. Time spent in commands, project tools and local jobs is not counted.";
const HOST_TIMEOUT: &str =
    "Claude turn stopped after 4 hours in commands, project tools and local jobs.";

pub(super) fn turn_clock() -> ModelClock {
    ModelClock::new(MODEL_TIME_LIMIT, HOST_TIME_LIMIT)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn drive(
    process: &mut Process,
    app: &impl TurnEvents,
    turn: &Turn,
    prompt: &protocol::Prompt,
    specs: Vec<Value>,
    mut clock: ModelClock,
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
        let message = json!({"type":"user","message":{"role":"user","content":prompt.text},"parent_tool_use_id":null,"session_id":""});
        clock
            .model(process.write(&message))
            .await
            .ok_or(MODEL_TIMEOUT)??;
        let mut bytes = 0usize;
        let mut answers = super::answers::Answers::default();
        let mut calls = 0;
        let mut activity = super::activity::Activity::default();
        loop {
            // Claude's own tools run inside the CLI, so a command in progress
            // looks like silence here. It is host time, not model time.
            let frame = if activity.running() {
                clock.host(process.read_sized()).await.ok_or(HOST_TIMEOUT)?
            } else {
                clock
                    .model(process.read_sized())
                    .await
                    .ok_or(MODEL_TIMEOUT)?
            };
            let (value, size) = frame?;
            bytes += size;
            if bytes > 64 * 1024 * 1024 {
                return Err("Claude turn exceeded its output budget".into());
            }
            activity.observe(&value, turn, app).await?;
            for event in answers.observe(&value, turn) {
                turn.emit(app, event).await?;
            }
            match value["type"].as_str() {
                Some("control_request") => {
                    calls += 1;
                    if calls > 128 {
                        return Err("Claude turn exceeded its tool/control request budget".into());
                    }
                    // Only the tool work is host time. Claude must still read
                    // the answer within the model time.
                    let answer = clock
                        .host(control_answer(&value, &specs, Some(turn)))
                        .await
                        .ok_or(HOST_TIMEOUT)??;
                    clock
                        .model(process.write(&answer))
                        .await
                        .ok_or(MODEL_TIMEOUT)??;
                }
                Some("stream_event") if value["parent_tool_use_id"].is_null() => {
                    if let Some(delta) = value["event"]["delta"]["text"].as_str() {
                        let _ = app.publish(&NormalizedEvent::AgentMessageDelta {
                            epoch: turn.epoch,
                            thread_id: turn.thread.clone(),
                            turn_id: turn.id.clone(),
                            item_id: format!("{}-assistant-{}", turn.id, answers.stream_index()),
                            delta: delta.into(),
                        });
                    }
                }
                Some("rate_limit_event") => {
                    super::usage::record(&value, process.usage_account.as_deref())
                }
                Some("result") => {
                    if value["is_error"] != false || value["subtype"] != "success" {
                        return Err(value["result"].as_str().map(str::to_owned).unwrap_or_else(
                            || format!("Claude run failed: {}", value["subtype"]),
                        ));
                    }
                    if !answers.final_received {
                        return Err("Claude returned no assistant response".into());
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    };
    tokio::select! {
        result = work => result,
        _ = cancel.changed() => Err("interrupted".into()),
    }
}

#[tauri::command]
pub async fn workbench_claude_model_catalog(
) -> Result<crate::connections::models::ModelCatalog, String> {
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
    let mut process = Process::start(&args, None, None).await?;
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
            Some(crate::connections::models::WorkspaceModel {
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
                    .map(|v| crate::connections::models::ReasoningEffortOption {
                        reasoning_effort: v.into(),
                        description: v.into(),
                    })
                    .collect(),
            })
        })
        .collect();
    Ok(crate::connections::models::ModelCatalog { models })
}
