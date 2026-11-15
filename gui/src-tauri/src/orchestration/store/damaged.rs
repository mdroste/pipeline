//! Preserve unreadable records verbatim and take them out of the dispatch queue.
use super::{err, Result, Store};
use rusqlite::params;
use serde_json::{json, Value};

impl Store {
    pub(super) fn quarantine(&self, kind: &str, id: &str, diagnostic: &str) -> Result<()> {
        let mut c = self.connection()?;
        let tx = c.transaction().map_err(err)?;
        match kind {
            "run" => tx.execute(
                "UPDATE runs SET state='unreadable', due_at=NULL WHERE id=?1",
                [id],
            ),
            "schedule" => tx.execute(
                "UPDATE schedules SET enabled=0,next_due_at=NULL WHERE id=?1",
                [id],
            ),
            _ => return Err("Unknown damaged record type".into()),
        }
        .map_err(err)?;
        tx.execute(
            "INSERT OR REPLACE INTO damaged_records(kind,id,diagnostic) VALUES(?1,?2,?3)",
            params![kind, id, diagnostic],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }
    pub fn storage_issues(&self) -> Result<Vec<Value>> {
        let c = self.connection()?;
        let mut q = c
            .prepare("SELECT kind,id,diagnostic FROM damaged_records ORDER BY kind,id LIMIT 100")
            .map_err(err)?;
        let rows=q.query_map([],|r|Ok(json!({"kind":r.get::<_,String>(0)?,"id":r.get::<_,String>(1)?,"diagnostic":r.get::<_,String>(2)?}))).map_err(err)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)
    }
}
