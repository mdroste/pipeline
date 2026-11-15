//! Read-only, bounded observations of one submitted turn. Raw provider payloads
//! and reasoning stay in the conversation store, outside automation progress.
use super::*;
use crate::agent_runtime::events::NormalizedEvent;
use std::sync::{Mutex, OnceLock};

type LiveTimes = std::collections::BTreeMap<(String, String), i64>;
static LIVE: OnceLock<Mutex<LiveTimes>> = OnceLock::new();

// Streaming deltas are deliberately absent from durable transcripts. Keep only
// their observation times, with no text or authority to alter turn lifecycle.
pub(crate) fn observe(event: &NormalizedEvent) {
    let (thread, turn) = match event {
        NormalizedEvent::AgentMessageDelta {
            thread_id, turn_id, ..
        }
        | NormalizedEvent::ItemStarted {
            thread_id, turn_id, ..
        }
        | NormalizedEvent::ItemCompleted {
            thread_id, turn_id, ..
        }
        | NormalizedEvent::TurnStarted {
            thread_id, turn_id, ..
        }
        | NormalizedEvent::TurnCompleted {
            thread_id, turn_id, ..
        } => (thread_id.as_str(), turn_id.as_str()),
        NormalizedEvent::UnknownNotification { method, params, .. }
            if method.starts_with("item/") && method.ends_with("/delta") =>
        {
            let (Some(thread), Some(turn)) =
                (params["threadId"].as_str(), params["turnId"].as_str())
            else {
                return;
            };
            (thread, turn)
        }
        _ => return,
    };
    if thread.is_empty() || turn.is_empty() || thread.len() > 512 || turn.len() > 512 {
        return;
    }
    let mut live = LIVE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    live.insert((thread.into(), turn.into()), chrono::Utc::now().timestamp());
    if live.len() > 64 {
        if let Some(oldest) = live
            .iter()
            .min_by_key(|(_, at)| **at)
            .map(|(id, _)| id.clone())
        {
            live.remove(&oldest);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Entry {
    pub id: String,
    pub at: i64,
    pub label: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Agent {
    pub id: String,
    pub label: String,
    pub state: String,
    pub updated_at: i64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub last_activity_at: Option<i64>,
    pub entries: Vec<Entry>,
    pub agents: Vec<Agent>,
}

fn timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.timestamp())
}

pub(crate) fn read(store: &Store, session: &str, submission: &str) -> WorkbenchResult<Snapshot> {
    let Some((turn, _, thread, provider_turn, _)) = submitted_turn(store, session, submission)?
    else {
        return Ok(Snapshot::default());
    };
    let c = store.connection()?;
    let latest: String = c.query_row(
        "SELECT MAX(updated_at,COALESCE((SELECT MAX(updated_at) FROM transcript_items WHERE turn_id=turns.id),updated_at)) FROM turns WHERE id=?1",
        [&turn], |r| r.get(0),
    ).map_err(error)?;
    // Extract only small display fields, so a large manuscript or tool result
    // never has to be loaded to display progress.
    let mut q = c.prepare("SELECT id,item_kind,is_final,updated_at,
        CASE WHEN json_valid(payload_json) THEN
          substr(CASE WHEN item_kind='agentMessage' AND json_extract(payload_json,'$.phase')='commentary'
            THEN json_extract(payload_json,'$.text') ELSE json_extract(payload_json,'$.tool') END,1,500)
        END,
        CASE WHEN json_valid(payload_json) THEN json_extract(payload_json,'$.status') END
        FROM transcript_items WHERE turn_id=?1 AND item_kind IN
          ('agentMessage','commandExecution','dynamicToolCall','mcpToolCall','webSearch','fileChange','plan')
        ORDER BY created_at DESC,id DESC LIMIT 30").map_err(error)?;
    let rows = q
        .query_map([turn], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(error)?;
    let mut entries = Vec::new();
    for row in rows {
        let (id, kind, finished, at, detail, status) = row.map_err(error)?;
        let label = match kind.as_str() {
            "agentMessage" => detail
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Assistant response".into()),
            "commandExecution" => "Command execution".into(),
            "dynamicToolCall" | "mcpToolCall" => match detail.as_deref() {
                Some("project_read" | "workbench_paper_read" | "workbench_source_read") => {
                    "Read research file"
                }
                Some("project_files") => "List project files",
                Some("project_edit" | "project_create") => "Write research file",
                Some("workbench_paper_search" | "workbench_research_search_v1") => {
                    "Search research material"
                }
                Some(
                    "workbench_research_records"
                    | "workbench_research_object_v1"
                    | "workbench_project_context",
                ) => "Inspect research records",
                Some("workbench_dataset_rows_v1") => "Inspect data rows",
                Some("workbench_paper_page") => "Inspect paper page",
                Some("web_search") => "Search the web",
                Some("web_fetch") => "Read web page",
                Some("shell") => "Run command",
                _ => "Research tool call",
            }
            .into(),
            "webSearch" => "Web search".into(),
            "fileChange" => "File changes".into(),
            _ => "Plan update".into(),
        };
        entries.push(Entry {
            id,
            at: timestamp(&at).unwrap_or_default(),
            label,
            state: match status.as_deref() {
                Some("failed" | "declined") => "failed",
                _ if finished => "completed",
                _ => "running",
            }
            .into(),
        });
    }
    let streamed = LIVE.get().and_then(|live| {
        live.lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(thread, provider_turn.unwrap_or_default()))
            .copied()
    });
    Ok(Snapshot {
        last_activity_at: timestamp(&latest).max(streamed),
        entries,
        agents: vec![],
    })
}

#[cfg(test)]
mod tests;
