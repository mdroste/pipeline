//! Open-task context selection and explicit paging for the full task history.
use super::*;
const TASK_LIMIT: usize = 500;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCursor {
    pub updated_at: String,
    pub id: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPage {
    pub records: Vec<ProjectRecord>,
    pub next_cursor: Option<TaskCursor>,
}

pub fn task_page(
    store: &Store,
    workspace: &str,
    before: Option<&TaskCursor>,
) -> WorkbenchResult<TaskPage> {
    page(store, workspace, before, false)
}
pub(super) fn page(
    store: &Store,
    workspace: &str,
    before: Option<&TaskCursor>,
    open_only: bool,
) -> WorkbenchResult<TaskPage> {
    scope(store, workspace)?;
    if let Some(c) = before {
        valid_id(&c.id)?;
        if c.updated_at.len() > 128 {
            return Err(WorkbenchError::invalid("Invalid task cursor"));
        }
    }
    let conn = store.connection()?;
    let mut query=conn.prepare("SELECT id,workspace_id,kind,revision,body_json,updated_at FROM project_records WHERE workspace_id=?1 AND kind='task' AND (?2=0 OR json_extract(body_json,'$.status') NOT IN ('completed','rejected')) AND (?3 IS NULL OR updated_at<?3 OR (updated_at=?3 AND id>?4)) ORDER BY updated_at DESC,id LIMIT ?5").map_err(err)?;
    let mut records = query
        .query_map(
            params![
                workspace,
                open_only,
                before.map(|c| &c.updated_at),
                before.map(|c| &c.id),
                (TASK_LIMIT + 1) as i64
            ],
            read_record,
        )
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let more = records.len() > TASK_LIMIT;
    records.truncate(TASK_LIMIT);
    let next_cursor = more.then(|| {
        let last = records.last().expect("nonempty page");
        TaskCursor {
            updated_at: last.updated_at.clone(),
            id: last.id.clone(),
        }
    });
    Ok(TaskPage {
        records,
        next_cursor,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_context_survives_newer_completed_tasks_and_history_is_pageable() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::open_at(&t.path().join("store")).unwrap();
        let ws = s
            .create_workspace(super::super::super::store::CreateWorkspaceRequest {
                name: "audit".into(),
                root: None,
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let c = s.connection().unwrap();
        for i in 0..501 {
            let body = json!({"objective":if i==0 {"OPEN_SENTINEL"}else{"closed"},"anchorId":null,"expectedOutputs":[],"expectedChecks":[],"status":if i==0 {"open"}else{"completed"}});
            c.execute("INSERT INTO project_records(id,workspace_id,kind,revision,body_json,updated_at) VALUES(?1,?2,'task',1,?3,?4)",params![format!("task-{i:04}"),ws,body.to_string(),format!("{i:04}")]).unwrap();
        }
        assert!(project_context(&s, &ws).unwrap().contains("OPEN_SENTINEL"));
        let first = task_page(&s, &ws, None).unwrap();
        assert_eq!(first.records.len(), 500);
        let second = task_page(&s, &ws, first.next_cursor.as_ref()).unwrap();
        assert_eq!(second.records[0].id, "task-0000");
        assert!(second.next_cursor.is_none());
        c.execute("UPDATE project_records SET body_json=json_set(body_json,'$.status','open','$.objective','x')",[]).unwrap();
        assert!(project_context(&s, &ws)
            .unwrap()
            .contains("older open tasks are omitted"));
    }
}
