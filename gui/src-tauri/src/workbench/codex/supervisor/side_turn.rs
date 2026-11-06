//! Short, isolated native turns. Titles and idea teams each get their own process and temporary home.
use super::*;

impl AppServerSupervisor {
    /// Runs one bounded, tool-free turn in an ephemeral thread and returns the
    /// final assistant text. Server requests are declined and the turn is
    /// interrupted, so a side call can never wait on the user.
    pub async fn run_side_turn(&self, request: SideTurnRequest) -> Result<String, RequestError> {
        // Side calls own a separate process. Their teardown must never interrupt
        // the foreground conversation, even on an ambiguous acknowledgement.
        let (side, home) = self.isolated_process().await?;
        side.run_owned_side_turn(request, Some(home)).await
    }

    pub(super) async fn isolated_process(&self) -> Result<(Self, tempfile::TempDir), RequestError> {
        let account = self.account.as_ref().ok_or_else(|| {
            RequestError::unavailable("A managed account is required for a side turn")
        })?;
        let home = tempfile::tempdir().map_err(|e| RequestError::unavailable(e.to_string()))?;
        let mut side = Self::launch_with_codex_home(self.epoch, home.path().join("codex")).await?;
        account
            .attach(&side.native.client)
            .await
            .map_err(RequestError::unavailable)?;
        side.account = Some(account.clone());
        side.projection_enabled = false;
        Ok((side, home))
    }

    pub(super) async fn run_owned_side_turn(
        &self,
        request: SideTurnRequest,
        home: Option<tempfile::TempDir>,
    ) -> Result<String, RequestError> {
        if request.text.is_empty() || request.text.len() > MAX_SIDE_TURN_TEXT_BYTES {
            return Err(RequestError::invalid(format!(
                "Side turn text must contain 1 to {MAX_SIDE_TURN_TEXT_BYTES} bytes"
            )));
        }
        let mut account_use = SideAccountUse {
            permit: self.account_lease().await?,
            native: self.native.clone(),
            completed: false,
            submitted: false,
            home,
        };
        let mut events = self.native.client.subscribe();
        let connection = self
            .start_thread_unbound(
                StartThreadRequest {
                    workbench_session_id: "side-turn".to_string(),
                    base_instructions: None,
                    cwd: request.cwd.clone(),
                    runtime_workspace_roots: vec![request.cwd],
                    permissions: request.permissions,
                    developer_instructions: request.developer_instructions,
                    model: request.model.clone(),
                    effort: request.effort.clone(),
                    dynamic_tools: Vec::new(),
                    config: Some(json!({"web_search":"disabled"})),
                },
                true,
            )
            .await?;
        let thread_id = connection.thread_id;
        let _registration = SideThreadRegistration::register(&self.side_threads, &thread_id);
        account_use.submitted = true;
        let result = self
            .native
            .client
            .start_text_turn(TextTurnStart {
                thread_id: &thread_id,
                text: &request.text,
                client_user_message_id: None,
                model: request.model.as_deref(),
                effort: request.effort.as_deref(),
                output_schema: None,
            })
            .await?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")?;
        let collected = tokio::time::timeout(
            request.timeout,
            self.collect_side_turn(&mut events, &thread_id, &turn_id),
        )
        .await;
        match collected {
            Ok(Ok(text)) => {
                account_use.completed = true;
                Ok(text)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => {
                let _ = self.interrupt_turn(&thread_id, &turn_id).await;
                Err(RequestError::unavailable(
                    "The side call did not finish within its time budget",
                ))
            }
        }
    }

    async fn collect_side_turn(
        &self,
        events: &mut broadcast::Receiver<NormalizedEvent>,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<String, RequestError> {
        let mut streamed = String::new();
        let mut final_text: Option<String> = None;
        let mut lagged = false;
        loop {
            match events.recv().await {
                Ok(NormalizedEvent::ConnectionClosed { reason, .. }) => {
                    return Err(RequestError::unavailable(format!(
                        "Workspace connection closed during the side call: {reason}"
                    )));
                }
                Ok(NormalizedEvent::ServerRequest {
                    request_id, params, ..
                }) if params.get("threadId").and_then(Value::as_str) == Some(thread_id) => {
                    let _ = self
                        .native
                        .client
                        .respond_to_server_request(
                            request_id,
                            Err(RequestError::invalid(
                                "Side calls cannot request approvals or input",
                            )),
                        )
                        .await;
                    let _ = self.interrupt_turn(thread_id, turn_id).await;
                    return Err(RequestError::unavailable(
                        "The side call tried to use a tool and was stopped",
                    ));
                }
                Ok(event) => {
                    if event_thread_id(&event) != Some(thread_id) {
                        continue;
                    }
                    match event {
                        NormalizedEvent::AgentMessageDelta { delta, .. } => {
                            streamed.push_str(&delta)
                        }
                        NormalizedEvent::ItemCompleted {
                            item_kind, item, ..
                        } if item_kind == "agentMessage" => {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                final_text = Some(text.to_string());
                            }
                        }
                        NormalizedEvent::TurnCompleted {
                            turn_id: completed,
                            status,
                            turn,
                            ..
                        } if completed == turn_id => {
                            if status != "completed" {
                                return Err(RequestError::unavailable(format!(
                                    "The side call ended with status {status}"
                                )));
                            }
                            if final_text.is_none() {
                                final_text = last_agent_message(&turn);
                            }
                            if final_text.is_none() && lagged {
                                if let Ok(snapshot) = self.read_thread(thread_id).await {
                                    final_text = snapshot
                                        .get("thread")
                                        .and_then(|thread| thread.get("turns"))
                                        .and_then(Value::as_array)
                                        .and_then(|turns| {
                                            turns.iter().rev().find_map(last_agent_message)
                                        });
                                }
                            }
                            let text = final_text.unwrap_or(streamed);
                            if text.trim().is_empty() {
                                return Err(RequestError::unavailable(
                                    "The side call returned no assistant text",
                                ));
                            }
                            return Ok(text);
                        }
                        _ => {}
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => lagged = true,
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(RequestError::unavailable(
                        "Workspace event stream closed during the side call",
                    ));
                }
            }
        }
    }
}
