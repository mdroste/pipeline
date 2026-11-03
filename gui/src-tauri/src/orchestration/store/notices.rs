//! Notices owed to a window that could not hear them.
//!
//! The coordinator starts before the window listens, and a notice raised while
//! the app quits reaches a window that is closing. Those notices wait here and
//! go out once, when the next window is listening. Each run owes at most one.
use super::{err, Result, Store, TaskRun};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const OWED: &str = "owedNotices";
/// More would only repeat what one look at Automations shows.
const MAX_OWED: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwedNotice {
    pub event: String,
    pub payload: Value,
}

impl TaskRun {
    pub fn notice(&self) -> Value {
        json!({"id":self.id,"name":self.name,"state":self.state,"reason":self.reason})
    }
}

impl Store {
    /// A later notice for the same record replaces the earlier one.
    pub fn owe_notice(&self, event: &str, payload: &Value) -> Result<()> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let mut owed = read(&tx)?;
        owed.retain(|n| n.event != event || n.payload["id"] != payload["id"]);
        owed.push(OwedNotice {
            event: event.into(),
            payload: payload.clone(),
        });
        if owed.len() > MAX_OWED {
            owed.remove(0);
        }
        tx.execute(
            "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![OWED, serde_json::to_string(&owed).map_err(err)?],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    /// Hands over what is owed. Once taken, a notice is never raised again.
    pub fn take_notices(&self) -> Result<Vec<OwedNotice>> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let owed = read(&tx)?;
        tx.execute("DELETE FROM preferences WHERE key=?1", [OWED])
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(owed)
    }
}

/// An unreadable list is dropped: a lost notice must not stop the coordinator.
fn read(tx: &rusqlite::Transaction<'_>) -> Result<Vec<OwedNotice>> {
    let value: Option<String> = tx
        .query_row("SELECT value FROM preferences WHERE key=?1", [OWED], |r| {
            r.get(0)
        })
        .optional()
        .map_err(err)?;
    Ok(value
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default())
}
