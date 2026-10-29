//! Saved chain definitions and coordinator preferences.
use super::{err, hash, Result, Store};
use crate::orchestration::definition::Chain;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

impl Store {
    pub fn preference(&self, key: &str) -> Result<Option<Value>> {
        let value: Option<String> = self
            .connection()?
            .query_row("SELECT value FROM preferences WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(err)?;
        value
            .map(|v| serde_json::from_str(&v).map_err(err))
            .transpose()
    }

    pub fn set_preference(&self, key: &str, value: &Value) -> Result<()> {
        self.connection()?.execute("INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()]).map_err(err)?;
        Ok(())
    }

    pub fn save_chain(&self, chain: &Chain) -> Result<String> {
        crate::orchestration::definition::validate(chain)?;
        let key = hash(chain)?;
        self.connection()?
            .execute(
                "INSERT OR IGNORE INTO chains(id,body) VALUES(?1,?2)",
                params![key, serde_json::to_string(chain).map_err(err)?],
            )
            .map_err(err)?;
        Ok(key)
    }

    pub fn chains(&self) -> Result<Vec<Value>> {
        let c = self.connection()?;
        let mut s = c
            .prepare("SELECT id,body FROM chains ORDER BY rowid DESC LIMIT 200")
            .map_err(err)?;
        let result = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        result
            .into_iter()
            .map(|(id, body)| {
                Ok(json!({"id":id,"chain":serde_json::from_str::<Value>(&body).map_err(err)?}))
            })
            .collect()
    }
}
