//! Researcher saves from the artifact pane. Project edits go through Studio.
//! Private saves record their recovery claim before replacing a file.
use super::*;
use crate::workbench::store::RuntimeFileOwner;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationFileSave {
    pub session_id: String,
    pub path: String,
    pub scope_root: String,
    pub expected_hash: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
struct PrivateEdit {
    path: String,
    claim: String,
}

pub fn save_conversation_file(
    store: &Store,
    request: ConversationFileSave,
) -> WorkbenchResult<String> {
    let (root, owner) = store.runtime_file_scope(&request.session_id)?;
    if root.to_str() != Some(request.scope_root.as_str()) {
        return Err(WorkbenchError::conflict(
            "The conversation's working folder changed. Reopen the file before saving.",
        ));
    }
    if matches!(owner, RuntimeFileOwner::Discovery) {
        return Err(WorkbenchError::invalid(
            "Discovery files open read-only here.",
        ));
    }
    research::artifact_runs::ensure_folder_free(&root)?;
    let path = file_workspace::conversation_relative_path(&root, &request.path)?;
    if !files::REPLACEMENT || !file_workspace::editable_source(&path) || ignored(&path, &[]) {
        return Err(WorkbenchError::invalid(
            "Choose a supported source file outside excluded folders.",
        ));
    }
    if request.content.len() > file_open::TEXT_LIMIT || request.content.contains('\0') {
        return Err(WorkbenchError::invalid(
            "Save UTF-8 text up to 2 MiB without null characters.",
        ));
    }
    let safe = files::SafeRoot::open(&root)?;
    let _private_lock = if matches!(owner, RuntimeFileOwner::Private) {
        let guard = lock(store, &request.session_id)?;
        recover_private_edit(store, &request.session_id, &safe, &path)?;
        Some(guard)
    } else {
        None
    };
    let before = safe.read(&path)?;
    if hash(&before) != request.expected_hash {
        return Err(WorkbenchError::conflict(
            "This file changed since it was opened.",
        ));
    }
    if !file_open::is_editor_text(&path, &before) {
        return Err(WorkbenchError::invalid(
            "This file cannot be edited as UTF-8 text.",
        ));
    }
    let content = request.content;
    if content.as_bytes() == before {
        return Ok(request.expected_hash);
    }
    match owner {
        RuntimeFileOwner::Project {
            workspace_id,
            checkpoint_id,
        } => {
            let saved = studio::studio_mutate(
                store,
                studio::StudioMutation {
                    workspace_id,
                    operation_id: id("artifact-edit")?,
                    action: studio::StudioAction::SaveText {
                        checkpoint_id,
                        path,
                        expected_hash: request.expected_hash,
                        content,
                    },
                },
            )?;
            saved["hash"].as_str().map(str::to_owned).ok_or_else(|| {
                WorkbenchError::conflict("Check the saved file before retrying this edit.")
            })
        }
        RuntimeFileOwner::Private => {
            let claim = id("artifact-edit")?;
            let key = journal_key(&request.session_id, &path);
            let mode = safe.executable(&path)?;
            store
                .connection()?
                .execute(
                    "INSERT INTO project_operations VALUES (?1, ?2, ?3, ?4)",
                    params![
                        key,
                        request.expected_hash,
                        json!(PrivateEdit {
                            path: path.clone(),
                            claim: claim.clone(),
                        })
                        .to_string(),
                        now()
                    ],
                )
                .map_err(err)?;
            if let Err(error) = safe.replace(
                &path,
                Some(&request.expected_hash),
                mode,
                Some(content.as_bytes()),
                mode,
                &claim,
            ) {
                recover_private_edit(store, &request.session_id, &safe, &path)?;
                return Err(error);
            }
            safe.clear_claim(&path, &claim)?;
            clear_journal(store, &key)?;
            Ok(hash(content.as_bytes()))
        }
        RuntimeFileOwner::Discovery => Err(WorkbenchError::invalid(
            "Discovery files open read-only here.",
        )),
    }
}

pub(super) fn recover_private_edit(
    store: &Store,
    session_id: &str,
    folder: &files::SafeRoot,
    path: &str,
) -> WorkbenchResult<()> {
    let key = journal_key(session_id, path);
    let edit: Option<String> = store
        .connection()?
        .query_row(
            "SELECT response_json FROM project_operations WHERE operation_id = ?1",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(err)?;
    if let Some(edit) = edit {
        restore_private_edit(store, folder, &key, &edit)?;
    }
    Ok(())
}

/// A file missing during an interrupted replacement must reappear in the
/// browser even if no transcript message links to it.
pub(super) fn recover_private_edits(
    store: &Store,
    session_id: &str,
    folder: &Path,
) -> WorkbenchResult<()> {
    let prefix = journal_prefix(session_id);
    let connection = store.connection()?;
    let mut statement = connection
        .prepare(
            "SELECT operation_id, response_json FROM project_operations
         WHERE operation_id >= ?1 AND operation_id < ?2",
        )
        .map_err(err)?;
    let edits = statement
        .query_map(params![prefix, format!("{prefix}g")], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    if !edits.is_empty() {
        let safe = files::SafeRoot::open(folder)?;
        for (key, edit) in edits {
            restore_private_edit(store, &safe, &key, &edit)?;
        }
    }
    Ok(())
}

fn restore_private_edit(
    store: &Store,
    folder: &files::SafeRoot,
    key: &str,
    edit: &str,
) -> WorkbenchResult<()> {
    let edit: PrivateEdit = serde_json::from_str(edit).map_err(err)?;
    // Restore a missing target. If an external writer has already placed a
    // file at that path, keep both versions on disk.
    folder.recover_claim(&edit.path, &edit.claim)?;
    clear_journal(store, key)
}

fn journal_key(session_id: &str, path: &str) -> String {
    format!("{}{}", journal_prefix(session_id), hash(path.as_bytes()))
}

fn journal_prefix(session_id: &str) -> String {
    format!("private-artifact-edit-{}-", hash(session_id.as_bytes()))
}

fn clear_journal(store: &Store, key: &str) -> WorkbenchResult<()> {
    store
        .connection()?
        .execute(
            "DELETE FROM project_operations WHERE operation_id = ?1",
            [key],
        )
        .map_err(err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_recovers_an_interrupted_private_save() {
        let f = super::super::test_support::fixture();
        let session = f
            .store
            .create_session(crate::workbench::store::CreateSessionRequest {
                workspace_id: None,
                title: "Artifacts".into(),
                operation_id: id("session").unwrap(),
            })
            .unwrap()
            .record;
        let root = f.store.runtime_root(&session.id).unwrap();
        let path = "unlinked.py";
        let claim = id("artifact-edit").unwrap();
        fs::write(root.join(format!(".pipeline-apply-{claim}")), "print(1)").unwrap();
        let key = journal_key(&session.id, path);
        f.store
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO project_operations VALUES (?1, ?2, ?3, ?4)",
                params![
                    key,
                    hash(b"print(1)"),
                    json!(PrivateEdit {
                        path: path.into(),
                        claim
                    })
                    .to_string(),
                    now()
                ],
            )
            .unwrap();
        let listing = list_conversation_directory(&f.store, &session.id, String::new())
            .unwrap()
            .listing
            .unwrap();
        assert_eq!(listing.entries[0].path, path);
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), "print(1)");
    }
}
