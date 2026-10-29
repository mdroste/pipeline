//! Paged native turn history for status checks and store reconciliation.
//!
//! `thread/read` with `includeTurns` returns a whole conversation in one
//! frame, so its size grows with the conversation until it passes the
//! transport frame limit. These reads page turn statuses without items, then
//! fetch item detail one turn at a time, only for turns the store does not
//! already hold as terminal.

use super::{NormalizedEvent, RequestError, DEFAULT_REQUEST_TIMEOUT};
use crate::workbench::codex::AppServerClient;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// The newest turns, without items. The permit-holding turn is the newest
/// turn of its thread; the margin covers a turn that has just ended.
const RECENT_STATUS_TURNS: usize = 4;
/// Status pages carry no items, so even a large page is a small frame.
const STATUS_PAGE_TURNS: usize = 100;
/// Bounds one status walk to 5,000 turns.
const MAX_STATUS_PAGES: usize = 50;

/// Open store turns, by thread and turn id, that a complete status walk did
/// not find in the native history. Later walks stop waiting to see them, so
/// such a turn forces at most one full walk per process.
fn absent_turns() -> &'static Mutex<HashSet<(String, String)>> {
    static ABSENT: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    ABSENT.get_or_init(Default::default)
}

/// A turn seen by a status walk: the cursor that requested its page (`None`
/// for the newest page) and its offset on that page.
#[derive(Debug, Clone, PartialEq)]
struct Located {
    id: String,
    cursor: Option<String>,
    offset: usize,
}

/// How a status walk ended.
#[derive(Debug, PartialEq)]
enum Walk {
    /// The visitor asked to stop.
    Stopped,
    /// The oldest turn was visited.
    Exhausted,
    /// `MAX_STATUS_PAGES` were read first.
    Capped,
}

/// The newest turns of a thread with their status and no items.
pub(super) async fn recent(
    client: &AppServerClient,
    thread_id: &str,
) -> Result<Value, RequestError> {
    list(client, thread_id, None, RECENT_STATUS_TURNS, "notLoaded").await
}

/// Projects native turns that are missing or still open in the store and
/// returns the ids of those that are still running.
pub(super) async fn reconcile(
    client: &AppServerClient,
    epoch: u64,
    binding_id: &str,
    thread_id: &str,
) -> Result<Vec<String>, RequestError> {
    let lookup = binding_id.to_string();
    let projected =
        crate::workbench::commands::run_store(move |store| store.projected_turn_states(&lookup))
            .await
            .map_err(|error| RequestError::unavailable(error.message))?;
    let located = unreconciled(client, thread_id, &projected).await?;
    if located.is_empty() {
        return Ok(Vec::new());
    }
    let mut pending = fetch_full(client, thread_id, &located).await?;
    // Oldest first, the order in which `thread/read` replayed turns.
    pending.reverse();
    let running = pending
        .iter()
        .filter(|turn| !is_terminal(turn_status(turn)))
        .filter_map(turn_id)
        .map(str::to_string)
        .collect();
    let binding_id = binding_id.to_string();
    let thread_id = thread_id.to_string();
    crate::workbench::commands::run_store(move |store| {
        project_turns(&store, &binding_id, &thread_id, epoch, &pending)
    })
    .await
    .map_err(|error| RequestError::unavailable(error.message))?;
    Ok(running)
}

/// Re-reads one ended turn and projects the finished items the store missed
/// while it ran, after a projection lag or a failed projection write.
pub(super) async fn repair(
    client: &AppServerClient,
    epoch: u64,
    binding_id: &str,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), RequestError> {
    let mut found = None;
    walk_statuses(client, thread_id, RECENT_STATUS_TURNS, |turn| {
        if turn.id != turn_id {
            return true;
        }
        found = Some(turn);
        false
    })
    .await?;
    let Some(located) = found else {
        return Ok(());
    };
    let Some(turn) = fetch_full(client, thread_id, &[located]).await?.pop() else {
        return Ok(());
    };
    let binding_id = binding_id.to_string();
    let thread_id = thread_id.to_string();
    let turn_id = turn_id.to_string();
    crate::workbench::commands::run_store(move |store| {
        project_items(&store, &binding_id, &thread_id, epoch, &turn_id, &turn)
    })
    .await
    .map_err(|error| RequestError::unavailable(error.message))
}

/// The native turn of an event that the projection writes.
pub(super) fn event_turn_id(event: &NormalizedEvent) -> Option<&str> {
    match event {
        NormalizedEvent::TurnStarted { turn_id, .. }
        | NormalizedEvent::TurnCompleted { turn_id, .. }
        | NormalizedEvent::ItemStarted { turn_id, .. }
        | NormalizedEvent::ItemCompleted { turn_id, .. } => Some(turn_id),
        _ => None,
    }
}

/// Turns that `projected` lacks or holds as open, newest first.
///
/// The walk stops at a turn the store holds as terminal once every open store
/// turn has been seen. Open store turns that the whole history lacks are
/// remembered, so they cannot force another full walk.
async fn unreconciled(
    client: &AppServerClient,
    thread_id: &str,
    projected: &HashMap<String, bool>,
) -> Result<Vec<Located>, RequestError> {
    let mut open: HashSet<&str> = {
        let absent = absent_turns()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        projected
            .iter()
            .filter(|(id, terminal)| {
                !**terminal && !absent.contains(&(thread_id.to_string(), id.to_string()))
            })
            .map(|(id, _)| id.as_str())
            .collect()
    };
    let mut pending = Vec::new();
    let mut reached_reconciled = false;
    let walk = walk_statuses(client, thread_id, STATUS_PAGE_TURNS, |turn| {
        open.remove(turn.id.as_str());
        if projected.get(&turn.id) == Some(&true) {
            reached_reconciled = true;
        } else {
            pending.push(turn);
        }
        !(reached_reconciled && open.is_empty())
    })
    .await?;
    if walk == Walk::Capped {
        client.warn(format!(
            "Workbench reconciled only the newest {} turns of a long conversation",
            STATUS_PAGE_TURNS * MAX_STATUS_PAGES
        ));
    }
    if walk != Walk::Stopped && !open.is_empty() {
        absent_turns()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .extend(
                open.into_iter()
                    .map(|id| (thread_id.to_string(), id.to_string())),
            );
    }
    Ok(pending)
}

/// Lists turn statuses newest first, without items, and passes each turn to
/// `visit` until it returns false.
async fn walk_statuses(
    client: &AppServerClient,
    thread_id: &str,
    page_size: usize,
    mut visit: impl FnMut(Located) -> bool,
) -> Result<Walk, RequestError> {
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_STATUS_PAGES {
        let page = list(client, thread_id, cursor.as_deref(), page_size, "notLoaded").await?;
        let next = next_cursor(&page);
        for (offset, turn) in page_turns(page)?.iter().enumerate() {
            let Some(id) = turn_id(turn) else {
                continue;
            };
            let located = Located {
                id: id.to_string(),
                cursor: cursor.clone(),
                offset,
            };
            if !visit(located) {
                return Ok(Walk::Stopped);
            }
        }
        match next {
            Some(next) => cursor = Some(next),
            None => return Ok(Walk::Exhausted),
        }
    }
    Ok(Walk::Capped)
}

/// Fetches located turns, given in walk order, with full items, one turn per
/// request. A turn that is no longer at its walked position is left out.
async fn fetch_full(
    client: &AppServerClient,
    thread_id: &str,
    located: &[Located],
) -> Result<Vec<Value>, RequestError> {
    let mut turns = Vec::with_capacity(located.len());
    // After a fetch: its status page, the next offset and the cursor there.
    let mut resume: Option<(Option<String>, usize, String)> = None;
    for target in located {
        let (mut cursor, skip) = match resume.take() {
            Some((page, offset, next)) if page == target.cursor && offset <= target.offset => {
                (Some(next), target.offset - offset)
            }
            _ => (target.cursor.clone(), target.offset),
        };
        if skip > 0 {
            let skipped = list(client, thread_id, cursor.as_deref(), skip, "notLoaded").await?;
            match next_cursor(&skipped) {
                Some(next) => cursor = Some(next),
                None => continue,
            }
        }
        let page = list(client, thread_id, cursor.as_deref(), 1, "full").await?;
        let next = next_cursor(&page);
        match page_turns(page)?.into_iter().next() {
            Some(turn) if turn_id(&turn) == Some(target.id.as_str()) => {
                turns.push(turn);
                resume = next.map(|next| (target.cursor.clone(), target.offset + 1, next));
            }
            _ => client.warn(
                "Workbench skipped a turn that moved during reconciliation; a later reconciliation retries it",
            ),
        }
    }
    Ok(turns)
}

async fn list(
    client: &AppServerClient,
    thread_id: &str,
    cursor: Option<&str>,
    limit: usize,
    items_view: &str,
) -> Result<Value, RequestError> {
    client
        .request(
            "thread/turns/list",
            json!({
                "threadId": thread_id,
                "limit": limit,
                "sortDirection": "desc",
                "itemsView": items_view,
                "cursor": cursor
            }),
            DEFAULT_REQUEST_TIMEOUT,
        )
        .await
}

fn next_cursor(page: &Value) -> Option<String> {
    page.get("nextCursor")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn page_turns(mut page: Value) -> Result<Vec<Value>, RequestError> {
    match page.get_mut("data").map(Value::take) {
        Some(Value::Array(turns)) => Ok(turns),
        _ => Err(RequestError::unavailable(
            "thread/turns/list response omitted its data array",
        )),
    }
}

fn turn_id(turn: &Value) -> Option<&str> {
    turn.get("id").and_then(Value::as_str)
}

fn turn_status(turn: &Value) -> &str {
    turn.get("status")
        .and_then(Value::as_str)
        .unwrap_or("inProgress")
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "interrupted")
}

/// Replays native turns, oldest first, as authoritative events. A running
/// turn is projected as started, followed by each of its finished items,
/// which restores items whose live events were lost.
pub(super) fn project_turns(
    store: &crate::workbench::store::Store,
    binding_id: &str,
    thread_id: &str,
    epoch: u64,
    turns: &[Value],
) -> crate::workbench::store::WorkbenchResult<()> {
    for turn in turns {
        let Some(turn_id) = turn_id(turn) else {
            continue;
        };
        let status = turn_status(turn);
        if is_terminal(status) {
            let event = NormalizedEvent::TurnCompleted {
                epoch,
                thread_id: thread_id.to_string(),
                turn_id: turn_id.to_string(),
                status: status.to_string(),
                turn: turn.clone(),
            };
            store.project_codex_event(binding_id, &event)?;
            continue;
        }
        // The store ignores a started turn's items, so they are left out of
        // its payload and projected one by one.
        let event = NormalizedEvent::TurnStarted {
            epoch,
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            turn: without_items(turn),
        };
        store.project_codex_event(binding_id, &event)?;
        project_items(store, binding_id, thread_id, epoch, turn_id, turn)?;
    }
    Ok(())
}

/// Projects a turn's finished items one by one. An item already projected in
/// this epoch has the same operation id, so the store skips it. A running item
/// is left to its own completion event, whose operation id a partial copy
/// here would take.
fn project_items(
    store: &crate::workbench::store::Store,
    binding_id: &str,
    thread_id: &str,
    epoch: u64,
    turn_id: &str,
    turn: &Value,
) -> crate::workbench::store::WorkbenchResult<()> {
    let items = turn.get("items").and_then(Value::as_array);
    for item in items.into_iter().flatten() {
        if item.get("status").and_then(Value::as_str) == Some("inProgress") {
            continue;
        }
        let (Some(item_id), Some(item_kind)) = (
            item.get("id").and_then(Value::as_str),
            item.get("type").and_then(Value::as_str),
        ) else {
            continue;
        };
        let event = NormalizedEvent::ItemCompleted {
            epoch,
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            item_kind: item_kind.to_string(),
            item: item.clone(),
        };
        store.project_codex_event(binding_id, &event)?;
    }
    Ok(())
}

fn without_items(turn: &Value) -> Value {
    let Some(fields) = turn.as_object() else {
        return turn.clone();
    };
    let mut summary: serde_json::Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| key.as_str() != "items")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    summary.insert("items".to_string(), Value::Array(Vec::new()));
    Value::Object(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::store::{CreateSessionRequest, Store};
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// Serves `history` (newest first) in pages of at most `max_page` turns,
    /// with items only for `itemsView: full`. The task returns every request.
    fn serve(
        history: Vec<Value>,
        max_page: usize,
    ) -> (AppServerClient, tokio::task::JoinHandle<Vec<Value>>) {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(server_reader);
            let mut requests = Vec::new();
            let mut line = String::new();
            while reader.read_line(&mut line).await.unwrap() > 0 {
                let request: Value = serde_json::from_str(&line).unwrap();
                line.clear();
                let params = &request["params"];
                let start = params["cursor"]
                    .as_str()
                    .map_or(0, |cursor| cursor.parse::<usize>().unwrap());
                let limit = params["limit"].as_u64().unwrap() as usize;
                let end = history.len().min(start + limit.min(max_page));
                let full = params["itemsView"] == "full";
                let data: Vec<Value> = history[start..end]
                    .iter()
                    .map(|turn| {
                        let mut turn = turn.clone();
                        if !full {
                            turn["items"] = json!([]);
                            turn["itemsView"] = json!("notLoaded");
                        }
                        turn
                    })
                    .collect();
                let next = (end < history.len()).then(|| end.to_string());
                let result = json!({"data":data,"nextCursor":next});
                requests.push(request.clone());
                server_writer
                    .write_all(
                        format!("{}\n", json!({"id":request["id"],"result":result})).as_bytes(),
                    )
                    .await
                    .unwrap();
            }
            requests
        });
        (
            AppServerClient::from_io(3, client_reader, client_writer),
            server,
        )
    }

    fn turn(id: &str, status: &str) -> Value {
        json!({"id":id,"status":status,"itemsView":"full","items":[{"id":format!("{id}-item"),"type":"agentMessage","text":"Result"}]})
    }

    fn states(projected: &[(&str, bool)]) -> HashMap<String, bool> {
        projected
            .iter()
            .map(|(id, terminal)| (id.to_string(), *terminal))
            .collect()
    }

    fn ids(turns: &[Value]) -> Vec<Value> {
        turns.iter().map(|turn| turn["id"].clone()).collect()
    }

    #[test]
    fn reconciliation_stops_at_the_first_turn_the_store_already_holds() {
        runtime().block_on(async {
            let history = vec![
                turn("turn-3", "completed"),
                turn("turn-2", "completed"),
                turn("turn-1", "completed"),
            ];
            let (client, server) = serve(history, 100);
            let projected = states(&[("turn-1", true), ("turn-2", true), ("turn-3", false)]);
            let located = unreconciled(&client, "thread-1", &projected).await.unwrap();
            let pending = fetch_full(&client, "thread-1", &located).await.unwrap();
            client.close_writer().await.unwrap();
            let requests = server.await.unwrap();

            assert_eq!(ids(&pending), [json!("turn-3")]);
            assert_eq!(pending[0]["items"][0]["text"], "Result");
            // One item-free status page, then item detail for the one turn.
            assert_eq!(requests.len(), 2);
            for request in &requests {
                assert_eq!(request["method"], "thread/turns/list");
                assert_eq!(request["params"]["threadId"], "thread-1");
                assert_eq!(request["params"]["sortDirection"], "desc");
                assert!(request["params"]["cursor"].is_null());
            }
            assert_eq!(requests[0]["params"]["itemsView"], "notLoaded");
            assert_eq!(requests[0]["params"]["limit"], STATUS_PAGE_TURNS);
            assert_eq!(requests[1]["params"]["itemsView"], "full");
            assert_eq!(requests[1]["params"]["limit"], 1);
        });
    }

    #[test]
    fn reconciliation_continues_past_reconciled_turns_to_reach_open_ones() {
        runtime().block_on(async {
            let history = vec![
                turn("turn-4", "inProgress"),
                turn("turn-3", "completed"),
                turn("turn-2", "completed"),
                turn("turn-1", "interrupted"),
                turn("turn-0", "completed"),
            ];
            let (client, server) = serve(history, 100);
            let projected = states(&[
                ("turn-4", false),
                ("turn-2", true),
                ("turn-1", false),
                ("turn-0", true),
            ]);
            let located = unreconciled(&client, "thread-1", &projected).await.unwrap();
            let pending = fetch_full(&client, "thread-1", &located).await.unwrap();
            client.close_writer().await.unwrap();
            let requests = server.await.unwrap();

            assert_eq!(
                ids(&pending),
                [json!("turn-4"), json!("turn-3"), json!("turn-1")]
            );
            assert!(pending
                .iter()
                .all(|turn| turn["items"][0]["text"] == "Result"));
            // The status page, two adjacent turns, then a skip over turn-2.
            let shape: Vec<_> = requests
                .iter()
                .map(|request| {
                    let params = &request["params"];
                    (
                        params["itemsView"].as_str().unwrap().to_string(),
                        params["limit"].as_u64().unwrap(),
                        params["cursor"].clone(),
                    )
                })
                .collect();
            assert_eq!(
                shape,
                [
                    (
                        "notLoaded".to_string(),
                        STATUS_PAGE_TURNS as u64,
                        Value::Null
                    ),
                    ("full".to_string(), 1, Value::Null),
                    ("full".to_string(), 1, json!("1")),
                    ("notLoaded".to_string(), 1, json!("2")),
                    ("full".to_string(), 1, json!("3")),
                ]
            );
        });
    }

    #[test]
    fn an_open_turn_missing_from_history_forces_only_one_full_walk() {
        runtime().block_on(async {
            let history: Vec<_> = (0..7)
                .rev()
                .map(|index| turn(&format!("turn-{index}"), "completed"))
                .collect();
            // The server caps its pages below the requested size.
            let (client, server) = serve(history, 3);
            let mut projected: Vec<(String, bool)> = (0..7)
                .map(|index| (format!("turn-{index}"), true))
                .collect();
            projected.push(("turn-lost".to_string(), false));
            let projected: HashMap<_, _> = projected.into_iter().collect();
            let thread = "thread-missing-open-turn";
            let first = unreconciled(&client, thread, &projected).await.unwrap();
            let second = unreconciled(&client, thread, &projected).await.unwrap();
            client.close_writer().await.unwrap();
            let requests = server.await.unwrap();

            assert!(first.is_empty() && second.is_empty());
            // Three pages for the whole history, then one for the next walk.
            assert_eq!(requests.len(), 4);
            assert!(requests[3]["params"]["cursor"].is_null());
            assert!(absent_turns()
                .lock()
                .unwrap()
                .contains(&(thread.to_string(), "turn-lost".to_string())));
        });
    }

    fn item(id: &str, text: &str) -> Value {
        json!({"id":id,"type":"agentMessage","text":text})
    }

    fn command(id: &str, status: &str) -> Value {
        json!({"id":id,"type":"commandExecution","command":"stata -b do analysis.do","status":status})
    }

    fn stored_items(store: &Store, session_id: &str) -> HashMap<String, (Value, bool)> {
        store
            .transcript_page(session_id, None)
            .unwrap()
            .items
            .into_iter()
            .map(|item| (item.provider_item_id, (item.payload, item.is_final)))
            .collect()
    }

    #[test]
    fn reconciliation_restores_finished_items_missing_from_an_open_turn() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open_at(&temporary.path().join("workbench-store")).unwrap();
        let session = store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: "Wage regressions".to_string(),
                operation_id: "session-create".to_string(),
            })
            .unwrap()
            .record;
        let binding = store
            .bind_session(&session.id, "codex-0-147-0-epoch-5", "thread-1")
            .unwrap();
        let live = |event: NormalizedEvent| {
            store.project_codex_event(&binding.id, &event).unwrap();
        };
        let completed = |item: Value| NormalizedEvent::ItemCompleted {
            epoch: 5,
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: item["id"].as_str().unwrap().to_string(),
            item_kind: item["type"].as_str().unwrap().to_string(),
            item,
        };
        // Live events reached the store for the turn and its first item only.
        live(NormalizedEvent::TurnStarted {
            epoch: 5,
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            turn: json!({"id":"turn-1","status":"inProgress","items":[]}),
        });
        live(completed(item("item-a", "Loading the panel.")));

        let native = json!({"id":"turn-1","status":"inProgress","itemsView":"full","items":[
            item("item-a", "Native copy"),
            item("item-b", "The coefficient is stable."),
            command("item-c", "inProgress")
        ]});
        project_turns(&store, &binding.id, "thread-1", 5, &[native]).unwrap();

        let items = stored_items(&store, &session.id);
        assert_eq!(items.len(), 2);
        // The live copy is kept: the same operation id skips the replay.
        assert_eq!(items["item-a"].0["text"], "Loading the panel.");
        assert_eq!(items["item-b"].0["text"], "The coefficient is stable.");
        assert!(items["item-b"].1);
        assert!(!store.projected_turn_states(&binding.id).unwrap()["turn-1"]);

        // The running command's own completion still lands.
        live(completed(command("item-c", "completed")));
        assert_eq!(
            stored_items(&store, &session.id)["item-c"].0["status"],
            "completed"
        );

        // After the turn ends without items, a repair restores a lost item.
        live(NormalizedEvent::TurnCompleted {
            epoch: 5,
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            status: "completed".to_string(),
            turn: json!({"id":"turn-1","status":"completed","items":[]}),
        });
        let ended = json!({"id":"turn-1","status":"completed","items":[
            item("item-a", "Native copy"),
            item("item-b", "The coefficient is stable."),
            command("item-c", "completed"),
            item("item-d", "Robustness checks agree.")
        ]});
        project_items(&store, &binding.id, "thread-1", 5, "turn-1", &ended).unwrap();
        let items = stored_items(&store, &session.id);
        assert_eq!(items.len(), 4);
        assert_eq!(items["item-d"].0["text"], "Robustness checks agree.");
        assert!(store.projected_turn_states(&binding.id).unwrap()["turn-1"]);
    }
}
