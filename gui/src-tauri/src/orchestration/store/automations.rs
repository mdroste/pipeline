//! Read-only index of top-level automations. Filtering and pagination happen
//! across all three kinds before loading summaries; child tasks stay with their owner.
use super::{err, Result, Store};
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationSummary {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub state: String,
    pub reason: Option<String>,
    pub updated_at: i64,
    pub due_at: Option<i64>,
}

impl Store {
    pub fn automations(&self, view: &str, offset: u32) -> Result<Vec<AutomationSummary>> {
        let history = match view {
            "active" => false,
            "history" => true,
            _ => return Err("Choose Active or History to list automation runs".into()),
        };
        let c = self.connection()?;
        let mut q = c.prepare(
            "WITH candidates AS (
                SELECT 'task' kind, id, updated_at FROM runs
                WHERE (state IN ('finished','cancelled','failed')) = ?1
                  AND state<>'unreadable' AND json_valid(body)
                  AND NOT EXISTS (SELECT 1 FROM mission_children WHERE task_id=runs.id)
                  AND NOT EXISTS (SELECT 1 FROM discovery_children WHERE task_id=runs.id)
                UNION ALL
                SELECT 'research', id, updated_at FROM missions
                WHERE (state IN ('completed','exhausted','stopped')) = ?1 AND json_valid(body)
                UNION ALL
                SELECT 'discovery', id, updated_at FROM discovery_runs
                WHERE (state IN ('completed','partial','exhausted','blocked','failed','cancelled')) = ?1 AND json_valid(body)
            ), page AS (
                SELECT * FROM candidates ORDER BY updated_at DESC, kind, id LIMIT 50 OFFSET ?2
            )
            SELECT page.id, page.kind,
                CASE page.kind
                    WHEN 'task' THEN json_extract(r.body,'$.name')
                    WHEN 'research' THEN json_extract(m.body,'$.definition.name')
                    ELSE substr(json_extract(d.body,'$.definition.prompt'),1,240) END,
                COALESCE(r.state,m.state,d.state),
                json_extract(COALESCE(r.body,m.body,d.body),'$.reason'),
                page.updated_at,
                CASE page.kind WHEN 'task' THEN r.due_at ELSE NULL END
            FROM page
            LEFT JOIN runs r ON page.kind='task' AND r.id=page.id
            LEFT JOIN missions m ON page.kind='research' AND m.id=page.id
            LEFT JOIN discovery_runs d ON page.kind='discovery' AND d.id=page.id
            ORDER BY page.updated_at DESC, page.kind, page.id",
        ).map_err(err)?;
        let rows = q
            .query_map(params![history, offset], |r| {
                Ok(AutomationSummary {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    name: r.get(2)?,
                    state: r.get(3)?,
                    reason: r.get(4)?,
                    updated_at: r.get(5)?,
                    due_at: r.get(6)?,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests;
