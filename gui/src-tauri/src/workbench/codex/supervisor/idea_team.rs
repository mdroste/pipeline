//! A discovery lead and its native children share a disposable server. Its
//! process group and private home are reclaimed on success, timeout or Stop.
use super::*;
use crate::workbench::discovery::idea_team::{Children, INSTRUCTIONS};

pub(crate) fn config(count: u8) -> Value {
    json!({"web_search":"disabled","features.multi_agent":true,"agents.max_threads":count,
           "agents.max_depth":1,"features.shell_tool":false})
}

impl AppServerSupervisor {
    pub(crate) async fn run_idea_team(
        &self,
        prompt: String,
        model: Option<String>,
        effort: Option<String>,
        count: u8,
        progress: Option<tokio::sync::watch::Sender<crate::workbench::tasks::activity::Snapshot>>,
    ) -> Result<(String, Children), String> {
        let (side, home) = self.isolated_process().await.map_err(|e| e.message)?;
        let root = home.path().join("brief");
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let cwd = root.to_string_lossy().into_owned();
        let _lifetime = SideAccountUse {
            permit: side.account_lease().await.map_err(|e| e.message)?,
            native: side.native.clone(),
            completed: false,
            submitted: true,
            home: Some(home),
        };
        let mut events = side.native.client.subscribe();
        // Native forks need their parent's rollout. The temporary home owns it.
        let request = StartThreadRequest {
            workbench_session_id: "discovery-ideas".into(),
            cwd: cwd.clone(),
            runtime_workspace_roots: vec![cwd],
            permissions: "discovery-inspect".into(),
            developer_instructions: format!("{INSTRUCTIONS} Use the native spawn_agent tool with the default agent type. Wait for every child to complete."),
            base_instructions: None,
            model: model.clone(),
            effort: effort.clone(),
            dynamic_tools: Vec::new(),
            config: Some(config(count)),
        };
        let thread = side
            .start_thread_unbound(request, false)
            .await
            .map_err(|e| e.message)?
            .thread_id;
        let result = side
            .native
            .client
            .start_text_turn(TextTurnStart {
                thread_id: &thread,
                text: &prompt,
                client_user_message_id: None,
                model: model.as_deref(),
                effort: effort.as_deref(),
                output_schema: None,
            })
            .await
            .map_err(|e| e.message)?;
        let turn_id = required_string(&result, &["turn", "id"], "turn/start response")
            .map_err(|e| e.message)?;
        let mut children =
            progress.map_or_else(Children::default, |p| Children::observed(count, p));
        let mut answer = None;
        loop {
            // A missed event makes the child count unknowable. Never infer it from prose.
            match events
                .recv()
                .await
                .map_err(|e| format!("Lost native idea team events: {e}"))?
            {
                NormalizedEvent::ServerRequest { request_id, .. } => {
                    side.respond_to_server_request(
                        request_id,
                        Err(RequestError::invalid(
                            "Idea agents cannot request tools or approvals",
                        )),
                    )
                    .await
                    .map_err(|e| e.message)?;
                }
                NormalizedEvent::ConnectionClosed { reason, .. } => return Err(reason),
                NormalizedEvent::ItemStarted {
                    thread_id,
                    item_kind,
                    item,
                    ..
                } if thread_id == thread
                    && matches!(
                        item_kind.as_str(),
                        "collabAgentToolCall" | "subAgentActivity"
                    ) =>
                {
                    children.activity();
                    observe(&mut children, &item, &thread, count)?;
                }
                NormalizedEvent::UnknownNotification { method, params, .. }
                    if method.starts_with("item/")
                        && method.ends_with("/delta")
                        && params["threadId"].as_str().is_some_and(|id| {
                            id == thread || children.ids_started().contains(id)
                        }) =>
                {
                    children.activity();
                }
                NormalizedEvent::AgentMessageDelta { thread_id, .. }
                    if thread_id == thread || children.ids_started().contains(&thread_id) =>
                {
                    children.activity();
                }
                NormalizedEvent::ItemCompleted {
                    thread_id,
                    turn_id: tid,
                    item_kind,
                    item,
                    ..
                } if thread_id == thread && tid == turn_id => {
                    children.activity();
                    if matches!(
                        item_kind.as_str(),
                        "collabAgentToolCall" | "subAgentActivity"
                    ) {
                        observe(&mut children, &item, &thread, count)?;
                    }
                    if item_kind == "agentMessage" && item["phase"] != "commentary" {
                        answer = item["text"].as_str().map(str::to_owned);
                    }
                }
                NormalizedEvent::TurnCompleted {
                    thread_id,
                    turn_id: tid,
                    status,
                    ..
                } if thread_id == thread && tid == turn_id => {
                    if status != "completed" {
                        return Err(format!("The idea team lead ended with status {status}"));
                    }
                    children.verify(count)?;
                    return Ok((
                        answer
                            .filter(|s| !s.is_empty() && s.len() <= 512 * 1024)
                            .ok_or("Missing or oversized idea team answer")?,
                        children,
                    ));
                }
                _ => {}
            }
        }
    }
}

fn observe(children: &mut Children, item: &Value, parent: &str, count: u8) -> Result<(), String> {
    // Current managed Codex emits named activity on the lead's own turn.
    // Older runtimes publish the spawn/wait tool calls instead. Both can
    // report the same thread, so Children counts unique native IDs.
    if item["type"] == "subAgentActivity" {
        let path = item["agentPath"]
            .as_str()
            .and_then(|s| s.strip_prefix("/root/"));
        if !path.is_some_and(|s| !s.is_empty() && !s.contains('/')) {
            return Err("Codex reported an idea agent outside the requested team".into());
        }
        let id = item["agentThreadId"]
            .as_str()
            .ok_or("Missing native idea agent ID")?;
        match item["kind"].as_str() {
            Some("started") => children.start(id, count)?,
            Some("completed") => children.complete(id),
            Some("interrupted") => return Err("A Codex idea subagent was interrupted".into()),
            _ => {}
        }
        return Ok(());
    }
    if item["senderThreadId"] == parent
        && item["tool"] == "spawnAgent"
        && item["status"] == "failed"
    {
        return Err(format!(
            "Codex could not start a requested idea subagent (requested {count})."
        ));
    }
    if item["senderThreadId"] != parent || item["status"] != "completed" {
        return Ok(());
    }
    if item["tool"] == "spawnAgent" {
        for id in item["receiverThreadIds"].as_array().into_iter().flatten() {
            children.start(id.as_str().ok_or("Missing spawned agent ID")?, count)?;
        }
    }
    if let Some(states) = item["agentsStates"].as_object() {
        for (id, state) in states {
            if state["status"] == "completed" {
                children.complete(id);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_activity_confirms_distinct_named_children_on_the_leads_turn() {
        let mut children = Children::default();
        for kind in ["started", "started", "completed"] {
            observe(&mut children, &json!({"type":"subAgentActivity","agentPath":"/root/history","agentThreadId":"t","kind":kind}), "parent", 1).unwrap();
        }
        children.verify(1).unwrap();
        let nested = json!({"type":"subAgentActivity","agentPath":"/root/history/extra","agentThreadId":"u","kind":"started"});
        assert!(observe(&mut children, &nested, "parent", 1).is_err());
    }
    #[test]
    fn only_completed_direct_children_count_and_repeated_events_are_idempotent() {
        let mut c = Children::default();
        let spawn = json!({"senderThreadId":"parent","status":"completed","tool":"spawnAgent","receiverThreadIds":["a"]});
        observe(&mut c, &spawn, "parent", 1).unwrap();
        observe(&mut c, &spawn, "parent", 1).unwrap();
        assert!(c.verify(1).is_err());
        observe(&mut c,&json!({"senderThreadId":"parent","status":"completed","tool":"wait","agentsStates":{"a":{"status":"completed"},"unrelated":{"status":"completed"}}}),"parent",1).unwrap();
        assert!(c.verify(1).is_ok());
    }
}

#[cfg(test)]
#[tokio::test]
#[ignore = "Uses the managed ChatGPT subscription for one lead and two native idea subagents"]
async fn live_codex_idea_team() {
    let prompt = "Use exactly two native subagents. Assign one historical and one mathematical angle on calendars. Ask each for one short research question. Wait for both and return JSON with an ideas array of two strings.";
    let main = super::supervisor_manager().connect().await.unwrap();
    let (text, children) = tokio::time::timeout(
        Duration::from_secs(180),
        main.run_idea_team(prompt.into(), None, None, 2, None),
    )
    .await
    .expect("Codex idea team timed out")
    .unwrap();
    children.verify(2).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["ideas"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
