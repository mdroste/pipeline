//! Subscription-only Claude idea team. The lead has only Agent(idea-researcher);
//! children have no tools. CLI task/result envelopes establish who actually ran.
use super::{budget::OutputBudget, protocol, transport::Process};
use crate::workbench::discovery::idea_team::{Children, INSTRUCTIONS};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn arguments(model: Option<&str>, effort: Option<&str>) -> Vec<String> {
    let agents = json!({
        "idea-lead":{"description":"Research discovery lead","prompt":INSTRUCTIONS,"tools":["Agent(idea-researcher)"]},
        "idea-researcher":{"description":"Explore one assigned research angle","prompt":"Develop concise, distinct, testable research questions from the supplied brief. Return your sketches and their first investigations. Do not request tools or delegate.","tools":[],"model":"inherit","background":false}
    });
    let mut args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--disable-slash-commands",
        "--no-chrome",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
        "--tools",
        "Agent",
        "--allowedTools",
        "Agent(idea-researcher)",
        "--agent",
        "idea-lead",
        "--agents",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(agents.to_string());
    if let Some(m) = model {
        args.push(format!("--model={m}"));
    }
    if let Some(e) = effort {
        args.push(format!("--effort={e}"));
    }
    args
}

#[derive(Default)]
struct Activity {
    calls: BTreeSet<String>,
    tasks: BTreeMap<String, String>,
    children: Children,
}
impl Activity {
    fn started(&mut self, call: &str, id: &str, count: u8) -> Result<(), String> {
        if self.calls.contains(call) {
            self.children.identify(call, id);
            self.children.start(id, count)?;
            if self
                .tasks
                .insert(call.into(), id.into())
                .is_some_and(|old| old != id)
            {
                return Err("Claude reported conflicting subagent identities".into());
            }
        }
        Ok(())
    }
    fn observe(&mut self, v: &Value, count: u8) -> Result<(), String> {
        if !v["parent_tool_use_id"].is_null() {
            return Ok(());
        }
        for part in v["message"]["content"].as_array().into_iter().flatten() {
            if v["type"] == "assistant" && part["type"] == "tool_use" && part["name"] == "Agent" {
                let id = part["id"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 512)
                    .ok_or("Invalid Claude Agent call")?;
                if part["input"]["subagent_type"] != "idea-researcher" {
                    return Err("Claude requested an unexpected idea subagent type".into());
                }
                if self.calls.insert(id.into()) {
                    self.children.requested(id);
                }
                if self.calls.len() > usize::from(count) {
                    return Err(format!(
                        "Claude requested more than {count} idea subagents."
                    ));
                }
            }
            if v["type"] == "user" && part["type"] == "tool_result" {
                let Some(call) = part["tool_use_id"].as_str() else {
                    continue;
                };
                if !self.calls.contains(call) {
                    continue;
                }
                if part["is_error"] == true {
                    return Err(format!(
                        "Claude could not start a requested idea subagent (requested {count})."
                    ));
                }
                let result = &v["tool_use_result"];
                // A foreground Agent tool result carries the native agent ID.
                if result["status"] == "completed" {
                    if let Some(id) = result["agentId"].as_str() {
                        if let Some(task) = self.tasks.get(call) {
                            self.children.complete(task);
                        } else {
                            self.started(call, id, count)?;
                            self.children.complete(id);
                        }
                    }
                }
            }
        }
        if v["type"] == "system" {
            if let (Some(call), Some(task)) = (v["tool_use_id"].as_str(), v["task_id"].as_str()) {
                if v["subtype"] == "task_started" {
                    self.started(call, task, count)?;
                }
                if v["subtype"] == "task_notification"
                    && self.tasks.get(call).is_some_and(|id| id == task)
                {
                    match v["status"].as_str() {
                        Some("completed") => self.children.complete(task),
                        Some("failed" | "stopped") => return Err(format!("A Claude idea subagent stopped before completing its assignment (requested {count}).")),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) async fn run(
    prompt: String,
    model: Option<String>,
    effort: Option<String>,
    count: u8,
    progress: Option<tokio::sync::watch::Sender<crate::workbench::tasks::activity::Snapshot>>,
) -> Result<(String, Children), String> {
    let mut process =
        Process::start(&arguments(model.as_deref(), effort.as_deref()), None, None).await?;
    let result = collect(
        &mut process,
        &prompt,
        model.as_deref(),
        effort.as_deref(),
        count,
        progress,
    )
    .await;
    process.stop().await;
    result
}
async fn collect(
    process: &mut Process,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
    count: u8,
    progress: Option<tokio::sync::watch::Sender<crate::workbench::tasks::activity::Snapshot>>,
) -> Result<(String, Children), String> {
    let init = process.initialize(&[]).await?;
    protocol::validate_model(&init, model, effort)?;
    let message = json!({"type":"user","message":{"role":"user","content":prompt},"parent_tool_use_id":null,"session_id":""});
    process.write(&message).await?;
    let mut activity = Activity::default();
    if let Some(progress) = progress {
        activity.children = Children::observed(count, progress);
    }
    let mut budget = OutputBudget::default();
    for _ in 0..16384 {
        let frame = process.read().await?;
        budget.frame(&frame, frame.to_string().len())?;
        if matches!(
            frame["type"].as_str(),
            Some("assistant" | "user" | "stream_event" | "system")
        ) {
            activity.children.activity();
        }
        activity.observe(&frame, count)?;
        match frame["type"].as_str() {
            Some("rate_limit_event") => {
                super::usage::record(&frame, process.usage_account.as_deref())
            }
            Some("control_request") => process.control(&frame, &[], None).await?,
            Some("result") => {
                if frame["is_error"] != false || frame["subtype"] != "success" {
                    return Err(frame["result"]
                        .as_str()
                        .unwrap_or("Claude's idea team failed")
                        .chars()
                        .take(1000)
                        .collect());
                }
                // Claude can return a waiting message or an empty result while
                // background children finish. Their notifications wake the lead.
                if activity.children.pending() {
                    continue;
                }
                activity.children.verify(count)?;
                let Some(text) = frame["result"].as_str().filter(|s| !s.trim().is_empty()) else {
                    continue;
                };
                if text.len() > 512 * 1024 {
                    return Err("Oversized idea team answer".into());
                }
                return Ok((text.into(), activity.children));
            }
            _ => {}
        }
    }
    Err("Claude idea team exceeded its event limit".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreground_agents_show_starting_until_their_native_identity_is_observed() {
        let (sender, progress) = tokio::sync::watch::channel(Default::default());
        let mut activity = Activity {
            children: Children::observed(1, sender),
            ..Activity::default()
        };
        activity.observe(&json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"call","name":"Agent","input":{"subagent_type":"idea-researcher"}}]}}), 1).unwrap();
        assert_eq!(progress.borrow().agents[0].state, "starting");
        assert!(activity.children.verify(1).is_err());
        activity.observe(&json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"call"}]},"tool_use_result":{"status":"completed","agentId":"native"}}),1).unwrap();
        assert_eq!(progress.borrow().agents[0].state, "completed");
        activity.children.verify(1).unwrap();
    }
    #[test]
    fn async_launch_and_child_prose_do_not_count_as_completed_workers() {
        let mut a = Activity::default();
        let call = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"call","name":"Agent","input":{"subagent_type":"idea-researcher"}}]}});
        a.observe(&call, 1).unwrap();
        a.observe(&json!({"type":"system","subtype":"task_started","tool_use_id":"call","task_id":"actual"}),1).unwrap();
        a.observe(&json!({"type":"assistant","parent_tool_use_id":"call","message":{"content":[{"type":"text","text":"all agents completed"}]}}),1).unwrap();
        assert!(a.children.verify(1).is_err());
        a.observe(&json!({"type":"system","subtype":"task_notification","tool_use_id":"call","task_id":"actual","status":"completed"}),1).unwrap();
        assert!(a.children.verify(1).is_ok());
    }
}

#[cfg(test)]
#[tokio::test]
#[ignore = "Uses Claude subscription quota for one lead and two native idea subagents"]
async fn live_claude_idea_team() {
    let prompt = "Use exactly two foreground idea-researcher subagents. Assign one historical and one mathematical angle on calendars. Ask each for one short research question. Wait for both and return JSON with an ideas array of two strings.";
    let (text, children) = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        run(prompt.into(), None, None, 2, None),
    )
    .await
    .expect("Claude idea team timed out")
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
