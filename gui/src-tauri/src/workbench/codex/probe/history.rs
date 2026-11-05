//! Exercise the paged history reads that replace `thread/read` with turns,
//! with no authenticated model turn.
use super::*;

pub(super) async fn verify(
    stdin: &mut tokio::process::ChildStdin,
    reader: &mut BufReader<tokio::process::ChildStdout>,
    thread_id: &str,
    resumed: &Value,
) -> Result<(), String> {
    if resumed["thread"]["turns"]
        .as_array()
        .is_some_and(|turns| !turns.is_empty())
    {
        return Err("thread/resume returned turns despite excludeTurns".to_string());
    }
    // The same parameters as the Workspace status heartbeat, the history
    // walk's status pages, and its per-turn item reads.
    for (items_view, limit) in [("notLoaded", 4), ("notLoaded", 100), ("full", 1)] {
        let id = json!(format!("turns-{items_view}-{limit}"));
        send(
            stdin,
            &json!({
                "method": "thread/turns/list",
                "id": id,
                "params": {
                    "threadId": thread_id,
                    "limit": limit,
                    "sortDirection": "desc",
                    "itemsView": items_view,
                    "cursor": null
                }
            }),
        )
        .await?;
        let page = response_for(reader, &id).await?;
        let turns = page["data"]
            .as_array()
            .ok_or_else(|| format!("thread/turns/list ({items_view}) omitted data: {page}"))?;
        if turns.len() > limit {
            return Err(format!(
                "thread/turns/list ({items_view}) ignored its page size of {limit}"
            ));
        }
        if items_view == "notLoaded"
            && turns.iter().any(|turn| {
                turn["items"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
            })
        {
            return Err("thread/turns/list returned items with itemsView notLoaded".to_string());
        }
    }
    Ok(())
}
