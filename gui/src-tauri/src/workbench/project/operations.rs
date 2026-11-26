//! Filesystem-operation journal. Callers hold the project lock and serialize
//! the original request before dispatch. Unknown is durable before side effects;
//! replay returns the stored JSON, and failed operations are never auto-retried.
use super::{err, hash, now};
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use serde_json::{json, Value};
pub(super) enum Begin {
    Replay(Value),
    Started(Receipt),
}
pub(super) struct Receipt {
    operation_id: String,
}
pub(super) fn begin(
    store: &Store,
    operation_id: &str,
    request: &[u8],
    conflict: &str,
    recovery: &str,
) -> WorkbenchResult<Begin> {
    let fingerprint = hash(request);
    let previous: Option<(String, String)> = store
        .connection()?
        .query_row(
            "SELECT request_hash,response_json FROM project_operations WHERE operation_id=?1",
            [operation_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(err)?;
    if let Some((old, response)) = previous {
        if old != fingerprint {
            return Err(WorkbenchError::conflict(conflict));
        }
        return Ok(Begin::Replay(serde_json::from_str(&response).map_err(err)?));
    }
    store
        .connection()?
        .execute(
            "INSERT INTO project_operations VALUES(?1,?2,?3,?4)",
            params![
                operation_id,
                fingerprint,
                json!({"outcome":"unknown","recovery":recovery}).to_string(),
                now()
            ],
        )
        .map_err(err)?;
    Ok(Begin::Started(Receipt {
        operation_id: operation_id.into(),
    }))
}
impl Receipt {
    pub(super) fn complete(self, store: &Store, value: &Value) -> WorkbenchResult<()> {
        store
            .connection()?
            .execute(
                "UPDATE project_operations SET response_json=?2 WHERE operation_id=?1",
                params![self.operation_id, value.to_string()],
            )
            .map_err(err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{begin, Begin};
    use crate::workbench::project::test_support::fixture;
    use serde_json::json;
    #[test]
    fn unknown_is_durable_and_replay_is_exact() {
        let f = fixture();
        let receipt =
            match begin(&f.store, "journal-test", b"original", "conflict", "recover").unwrap() {
                Begin::Started(receipt) => receipt,
                _ => panic!("new operation"),
            };
        let Begin::Replay(unknown) =
            begin(&f.store, "journal-test", b"original", "conflict", "recover").unwrap()
        else {
            panic!("must not execute twice")
        };
        assert_eq!(unknown, json!({"outcome":"unknown", "recovery":"recover"}));
        assert!(begin(
            &f.store,
            "journal-test",
            b"different",
            "conflict",
            "recover"
        )
        .is_err());
        let value = json!({"opaqueExtension":[null, 3],"saved":true});
        receipt.complete(&f.store, &value).unwrap();
        let Begin::Replay(saved) =
            begin(&f.store, "journal-test", b"original", "conflict", "recover").unwrap()
        else {
            panic!("must replay")
        };
        assert_eq!(saved, value);
    }
}
