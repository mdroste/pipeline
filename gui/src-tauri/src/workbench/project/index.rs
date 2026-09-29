//! Bounded project navigation metadata. No transcripts, blobs, inventories, or model calls.
use super::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexItem {
    pub id: String,
    pub name: String,
    pub root: Option<String>,
    pub missing_root_at: Option<String>,
    pub updated_at: String,
    pub brief: Option<IndexEntry>,
    pub conversation: Option<IndexEntry>,
    pub activity: Option<IndexEntry>,
    pub next_task: Option<IndexEntry>,
    pub proposed_notes: u32,
    pub interrupted_edits: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub kind: String,
}

// The existing workspace-list boundary is 500 records. Text previews are
// capped in SQL before crossing the command boundary; no note body is loaded in full.
const LIMIT: usize = 500;

pub fn project_index(store: &Store) -> WorkbenchResult<Vec<ProjectIndexItem>> {
    let connection = store.connection()?;
    let mut query = connection.prepare(include_str!("index.sql")).map_err(err)?;
    let rows = query
        .query_map([LIMIT as i64 + 1], |row| {
            Ok((
                ProjectIndexItem {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    root: row.get(2)?,
                    missing_root_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    brief: None,
                    conversation: None,
                    activity: None,
                    next_task: None,
                    proposed_notes: row.get(9)?,
                    interrupted_edits: row.get(10)?,
                },
                [
                    row.get::<_, Option<String>>(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ],
            ))
        })
        .map_err(err)?;
    let mut projects = Vec::new();
    for row in rows {
        let (mut project, entries) = row.map_err(err)?;
        let [brief, conversation, activity, task] = entries;
        let decode = |value: Option<String>| -> WorkbenchResult<Option<IndexEntry>> {
            value
                .map(|json| serde_json::from_str(&json).map_err(err))
                .transpose()
        };
        project.brief = decode(brief)?;
        project.conversation = decode(conversation)?;
        project.activity = decode(activity)?;
        project.next_task = decode(task)?;
        if let Some(activity) = &project.activity {
            if activity.updated_at > project.updated_at {
                project.updated_at.clone_from(&activity.updated_at);
            }
        }
        projects.push(project);
    }
    if projects.len() > LIMIT {
        return Err(WorkbenchError::invalid(
            "Project list exceeds its safety limit",
        ));
    }
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    Ok(projects)
}

#[cfg(test)]
mod tests;
