//! Copies native team observations into the action's existing journal area.
//! These records describe progress only and never determine execution outcomes.
use super::*;
use crate::workbench::{discovery::idea_team, tasks::activity::Snapshot};
use tokio::sync::watch;

pub(super) async fn run(
    path: std::path::PathBuf,
    binding: crate::workbench::tasks::TaskBinding,
    text: String,
    model: Option<String>,
    effort: Option<String>,
    count: u8,
    mut stop: watch::Receiver<bool>,
) -> Result<Value> {
    let (sender, mut progress) = watch::channel(Snapshot::default());
    let team = idea_team::run(binding, text, model, effort, count, sender);
    tokio::pin!(team);
    loop {
        let result = tokio::select! {
            biased;
            _ = stop.changed() => return Err("Idea generation stopped".into()),
            result = &mut team => Some(result),
            changed = progress.changed() => {
                if changed.is_err() { return team.await; }
                None
            }
        };
        let copy = progress.borrow_and_update().clone();
        let destination = path.clone();
        // Disk waits hold neither a conversation worker nor its archive gate.
        let saved = tokio::task::spawn_blocking(move || {
            atomic_json(&destination, &serde_json::to_value(copy).map_err(err)?)
        })
        .await;
        if !matches!(saved, Ok(Ok(()))) {
            eprintln!("Discovery activity snapshot could not be saved");
        }
        if let Some(result) = result {
            return result;
        }
    }
}

pub(crate) fn read(
    store: &crate::orchestration::store::Store,
    operation: &str,
) -> Result<Option<Snapshot>> {
    use std::io::Read;
    if operation.len() != 32 || !operation.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid discovery activity operation".into());
    }
    let path = store
        .root
        .join("actions")
        .join(operation)
        .join("progress.json");
    if !path.try_exists().map_err(err)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(&path)?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() > 65536 {
        return Err("Discovery activity snapshot exceeds its limit".into());
    }
    let mut snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(err)?;
    snapshot.agents.truncate(8);
    snapshot.entries.truncate(32);
    Ok(Some(snapshot))
}
