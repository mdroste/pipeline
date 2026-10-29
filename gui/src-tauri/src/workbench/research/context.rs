//! Bounded automatic context from selected papers and curated project notes.

use super::open_connection;
use crate::workbench::store::{Store, WorkbenchError, WorkbenchResult};
use rusqlite::{params, OptionalExtension as _};
use std::fs;
use std::path::PathBuf;

pub(super) fn context_for_session(
    store: &Store,
    workspace_id: Option<&str>,
    paper_id: Option<&str>,
    budget: usize,
) -> WorkbenchResult<(String, bool)> {
    struct SelectedPaperContext {
        title: String,
        revision_id: Option<String>,
        content_hash: Option<String>,
        text_reference: Option<String>,
    }

    let Some(workspace_id) = workspace_id else {
        return Ok((String::new(), false));
    };
    let connection = open_connection(store)?;
    let mut sections = vec![crate::workbench::project::project_context(
        store,
        workspace_id,
    )?];
    let project_excluded = crate::workbench::project::excluded_notes(store, workspace_id)?;
    let mut paper_section = None;
    let accepted_revision = crate::workbench::project::selected_manuscript(store, workspace_id)?;
    if paper_id.is_some() || accepted_revision.is_some() {
        let paper: Option<SelectedPaperContext> = connection
            .query_row(
                "SELECT p.title, pr.id, pr.content_hash, pr.text_reference FROM papers p JOIN paper_revisions pr ON pr.paper_id=p.id WHERE p.workspace_id=?2 AND ((?1 IS NOT NULL AND p.id=?1 AND pr.id=p.current_revision_id) OR (?1 IS NULL AND pr.id=?3))",
                params![paper_id, workspace_id, accepted_revision],
                |row| Ok(SelectedPaperContext {
                    title: row.get(0)?,
                    revision_id: row.get(1)?,
                    content_hash: row.get(2)?,
                    text_reference: row.get(3)?,
                }),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to read selected paper", error))?;
        if let Some(paper) = paper {
            let mut section = format!(
                "Selected paper: {}\nImmutable revision: {}\nContent hash: {}",
                paper.title,
                paper.revision_id.unwrap_or_else(|| "none".to_string()),
                paper.content_hash.unwrap_or_else(|| "none".to_string())
            );
            if let Some(reference) = paper.text_reference {
                let path = PathBuf::from(reference);
                if path.parent() == Some(store.root_path().join("blobs").as_path()) {
                    if let Ok(text) = fs::read_to_string(path) {
                        section.push_str("\n\nSelected paper text:\n");
                        section.push_str(&text);
                    }
                }
            }
            paper_section = Some(section);
        }
    }
    let mut statement = connection.prepare("SELECT kind, body, state, origin, id FROM research_notes WHERE workspace_id = ?1 AND state = 'accepted' ORDER BY pinned DESC, updated_at DESC LIMIT 200").map_err(|error| WorkbenchError::storage("Failed to prepare research context", error))?;
    let notes = statement
        .query_map([workspace_id], |row| {
            if project_excluded.contains(&row.get::<_, String>(4)?) {
                return Ok(String::new());
            }
            Ok(format!(
                "- [{}; {}; origin={}] {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(1)?
            ))
        })
        .map_err(|error| WorkbenchError::storage("Failed to read research context", error))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| WorkbenchError::storage("Failed to decode research context", error))?;
    if !notes.is_empty() {
        sections.push(format!("Curated research notes:\n{}", notes.join("\n")));
    }
    if let Some(section) = paper_section {
        sections.push(section);
    }
    let mut body = sections.join("\n\n");
    let truncated = body.len() > budget;
    if truncated {
        let mut end = budget;
        while end > 0 && !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n[Context truncated at configured byte budget]");
    }
    Ok((body, truncated))
}
