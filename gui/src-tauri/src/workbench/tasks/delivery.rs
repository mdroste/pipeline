//! Workspace owns delivered bytes. Files are staged by digest; one transaction
//! associates them with their immutable conversation exchange. The exchange JSON
//! stays byte-compatible with the coordinator's journal and conversation context.
use super::{error, Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use serde_json::{json, Value};
use std::{io::Read, path::Path};

pub(crate) struct DeliveryFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub(crate) fn deliver(
    store: &Store,
    session: &str,
    operation: &str,
    value: Value,
    files: Vec<DeliveryFile>,
) -> WorkbenchResult<Value> {
    let key = format!("task-result-{operation}");
    let result = json!({"sessionId":session,"origin":"task","result":value});
    let encoded = serde_json::to_string(&result).map_err(error)?;
    if encoded.len() > 512 * 1024 {
        return Err(WorkbenchError::invalid("Task delivery exceeds 512 KiB"));
    }
    let mut conn = store.connection()?;
    let adopted = |c: &rusqlite::Connection| -> WorkbenchResult<bool> {
        let prior: Option<(String, bool)> = c
            .query_row(
                "SELECT value_json,files_adopted FROM task_exchanges WHERE id=?1",
                [&key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(error)?;
        let Some((json, adopted)) = prior else {
            return Ok(false);
        };
        if serde_json::from_str::<Value>(&json).map_err(error)? != result {
            return Err(WorkbenchError::conflict(
                "Delivery operation reused with different contents",
            ));
        }
        Ok(adopted)
    };
    if adopted(&conn)? {
        return Ok(result);
    }
    // The Workspace gate excludes prune/restore while bytes are staged.
    // A crash here leaves only unreferenced files for ordinary storage cleanup.
    let root = store.root_path().join("blobs");
    let mut staged = Vec::with_capacity(files.len());
    if !files.is_empty() {
        if !std::fs::symlink_metadata(&root).map_err(error)?.is_dir() {
            return Err(WorkbenchError::invalid(
                "Workspace blob folder was replaced",
            ));
        }
        for file in files {
            let size = i64::try_from(file.bytes.len()).map_err(error)?;
            let digest = crate::workbench::desk::hash(&file.bytes);
            let path = root.join(format!("{digest}.delivery"));
            crate::workbench::desk::write_blob_atomic(&path, &file.bytes, &digest)
                .map_err(error)?;
            staged.push((file.name, digest, size, path));
        }
        #[cfg(unix)]
        crate::safety::sync_file(&std::fs::File::open(&root).map_err(error)?).map_err(error)?;
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(error)?;
    tx.execute("INSERT INTO task_exchanges(id,session_id,kind,value_json,created_at) VALUES(?1,?2,'result',?3,?4) ON CONFLICT(id) DO NOTHING",params![key,session,encoded,crate::workbench::desk::now()]).map_err(error)?;
    if adopted(&tx)? {
        return Ok(result);
    }
    for (ordinal, (name, digest, size, path)) in staged.into_iter().enumerate() {
        let ordinal = i64::try_from(ordinal).map_err(error)?;
        tx.execute("INSERT INTO task_delivery_files(exchange_id,ordinal,filename,sha256,size_bytes,storage_reference) VALUES(?1,?2,?3,?4,?5,?6)",params![key,ordinal,name,digest,size,path.to_string_lossy()]).map_err(error)?;
    }
    tx.execute(
        "UPDATE task_exchanges SET files_adopted=1 WHERE id=?1",
        [&key],
    )
    .map_err(error)?;
    tx.commit().map_err(error)?;
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeliveredFile {
    pub exchange_id: String,
    pub ordinal: u32,
    pub filename: String,
    pub created_at: String,
}

pub(crate) fn list(
    store: &Store,
    session: &str,
    offset: u32,
) -> WorkbenchResult<Vec<DeliveredFile>> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT f.exchange_id,f.ordinal,f.filename,e.created_at FROM task_delivery_files f JOIN task_exchanges e ON e.id=f.exchange_id WHERE e.session_id=?1 ORDER BY e.created_at DESC,e.id,f.ordinal LIMIT 50 OFFSET ?2").map_err(error)?;
    let rows = q
        .query_map(params![session, offset], |r| {
            Ok(DeliveredFile {
                exchange_id: r.get(0)?,
                ordinal: r.get(1)?,
                filename: r.get(2)?,
                created_at: r.get(3)?,
            })
        })
        .map_err(error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(error)
}

pub(crate) fn read(
    store: &Store,
    session: &str,
    exchange: &str,
    ordinal: u32,
) -> WorkbenchResult<DeliveryFile> {
    let (name,path,digest,size):(String,String,String,i64)=store.connection()?.query_row(
        "SELECT f.filename,f.storage_reference,f.sha256,f.size_bytes FROM task_delivery_files f JOIN task_exchanges e ON e.id=f.exchange_id WHERE e.session_id=?1 AND e.id=?2 AND f.ordinal=?3",
        params![session,exchange,ordinal], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(error)?;
    if !(0..=256 * 1024 * 1024).contains(&size)
        || digest.len() != 64
        || !digest.bytes().all(|b| b.is_ascii_hexdigit())
        || Path::new(&path)
            != store
                .root_path()
                .join("blobs")
                .join(format!("{digest}.delivery"))
    {
        return Err(WorkbenchError::invalid(
            "Delivered file is unavailable in this library",
        ));
    }
    if name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || (cfg!(windows) && name.contains(':'))
        || name == "."
        || name == ".."
        || !std::fs::symlink_metadata(store.root_path().join("blobs"))
            .map_err(error)?
            .is_dir()
    {
        return Err(WorkbenchError::invalid("Invalid delivered file location"));
    }
    let mut bytes = Vec::new();
    crate::safety::open_regular_file(Path::new(&path))
        .map_err(WorkbenchError::invalid)?
        .take(size as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as i64 != size || crate::workbench::desk::hash(&bytes) != digest {
        return Err(WorkbenchError::invalid(
            "Delivered file failed its integrity check",
        ));
    }
    Ok(DeliveryFile { name, bytes })
}

/// Old receipts retain the exact result object needed to adopt their files.
pub(crate) fn pending(
    store: &Store,
    session: Option<&str>,
    after: &str,
) -> WorkbenchResult<Vec<(String, String, Value)>> {
    let c = store.connection()?;
    let mut q=c.prepare("SELECT id,session_id,value_json FROM task_exchanges WHERE kind='result' AND files_adopted=0 AND id>?1 AND (?2 IS NULL OR session_id=?2) ORDER BY id LIMIT 16").map_err(error)?;
    let rows = q
        .query_map(params![after, session], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, String>(2)?))
        })
        .map_err(error)?;
    rows.map(|row| {
        let (id, session, body) = row.map_err(error)?;
        Ok((
            id,
            session,
            serde_json::from_str::<Value>(&body).map_err(error)?["result"].clone(),
        ))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_file_registration_leaves_no_exchange_and_retry_commits_once() {
        let (_temp, store, session) = super::super::tests::fixture();
        let files = || {
            vec![
                DeliveryFile {
                    name: "paper.md".into(),
                    bytes: b"Paper".to_vec(),
                },
                DeliveryFile {
                    name: "code.txt".into(),
                    bytes: b"Code".to_vec(),
                },
            ]
        };
        let conn = store.connection().unwrap();
        conn.execute_batch("CREATE TRIGGER interrupt_delivery BEFORE INSERT ON task_delivery_files WHEN NEW.ordinal=1 BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(deliver(&store, &session, "operation", json!("Result"), files()).is_err());
        assert!(
            super::super::delivery_outcome(&store, &session, "operation")
                .unwrap()
                .is_none()
        );
        assert!(list(&store, &session, 0).unwrap().is_empty());
        conn.execute_batch("DROP TRIGGER interrupt_delivery")
            .unwrap();
        for _ in 0..2 {
            deliver(&store, &session, "operation", json!("Result"), files()).unwrap();
        }
        assert_eq!(list(&store, &session, 0).unwrap().len(), 2);
    }

    #[test]
    fn delivered_bytes_roundtrip_and_negative_recorded_sizes_are_refused() {
        let (_temp, store, session) = super::super::tests::fixture();
        let bytes = b"Retained research result".to_vec();
        deliver(
            &store,
            &session,
            "integer-storage",
            json!({}),
            vec![DeliveryFile {
                name: "result.txt".into(),
                bytes: bytes.clone(),
            }],
        )
        .unwrap();
        let exchange = "task-result-integer-storage";
        assert_eq!(read(&store, &session, exchange, 0).unwrap().bytes, bytes);
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE task_delivery_files SET size_bytes=-1 WHERE exchange_id=?1",
                [exchange],
            )
            .unwrap();
        assert!(read(&store, &session, exchange, 0).is_err());
    }
}
