//! Views persistence operations.

use super::*;

impl Store {
    /// Reads one app-owned preference. Preferences never enter harness
    /// fingerprints or archives' research semantics.
    pub fn preference(&self, key: &str) -> WorkbenchResult<Option<Value>> {
        validate_id("preference key", key)?;
        let raw: Option<String> = self
            .connection()?
            .query_row(
                "SELECT value_json FROM preferences WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to read preference", error))?;
        Ok(raw.and_then(|value| serde_json::from_str(&value).ok()))
    }

    pub fn set_preference(&self, key: &str, value: &Value) -> WorkbenchResult<()> {
        validate_id("preference key", key)?;
        let encoded = serde_json::to_string(value)
            .map_err(|error| WorkbenchError::invalid(format!("Invalid preference: {error}")))?;
        if encoded.len() > 64 * 1024 {
            return Err(WorkbenchError::invalid("Preference exceeds 64 KiB"));
        }
        self.connection()?
            .execute(
                "INSERT INTO preferences (key, value_json, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                params![key, encoded, now()],
            )
            .map_err(|error| WorkbenchError::storage("Failed to save preference", error))?;
        Ok(())
    }

    pub fn session_snapshot(&self, session_id: &str) -> WorkbenchResult<SessionSnapshot> {
        validate_id("session id", session_id)?;
        let connection = self.connection()?;
        let session = load_session(&connection, session_id)?;
        let workspace = session
            .workspace_id
            .as_deref()
            .map(|workspace_id| load_workspace(&connection, workspace_id))
            .transpose()?;
        Ok(SessionSnapshot {
            workspace,
            session,
            sequence: current_sequence(&connection)?,
        })
    }

    pub fn conversation_snapshot(&self, session_id: &str) -> WorkbenchResult<ConversationSnapshot> {
        const MAX_HYDRATED_TURNS: usize = 500;
        validate_id("session id", session_id)?;
        let connection = self.connection()?;
        let session = load_session(&connection, session_id)?;
        let workspace = session
            .workspace_id
            .as_deref()
            .map(|workspace_id| load_workspace(&connection, workspace_id))
            .transpose()?;
        let active_binding = load_active_binding(&connection, session_id)?;
        let mut turns_statement = connection
            .prepare(
                "SELECT t.id, t.binding_id, t.client_submission_id, t.provider_turn_id, t.state, t.error_json, t.created_at, t.updated_at, t.terminal_at FROM turns t JOIN session_bindings b ON b.id = t.binding_id WHERE b.session_id = ?1 AND (t.provider_turn_id IS NULL OR NOT EXISTS (SELECT 1 FROM turns newer JOIN session_bindings nb ON nb.id = newer.binding_id WHERE nb.session_id = b.session_id AND newer.provider_turn_id = t.provider_turn_id AND nb.incarnation > b.incarnation)) ORDER BY (t.terminal_at IS NULL) DESC, t.created_at DESC, t.id DESC LIMIT ?2",
            )
            .map_err(|error| WorkbenchError::storage("Failed to prepare conversation turns", error))?;
        let mut turns = turns_statement
            .query_map(params![session_id, MAX_HYDRATED_TURNS as i64], |row| {
                let error: Option<String> = row.get(5)?;
                Ok(ConversationTurn {
                    id: row.get(0)?,
                    binding_id: row.get(1)?,
                    client_submission_id: row.get(2)?,
                    provider_turn_id: row.get(3)?,
                    state: row.get(4)?,
                    error: error.and_then(|value| serde_json::from_str(&value).ok()),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    terminal_at: row.get(8)?,
                })
            })
            .map_err(|error| WorkbenchError::storage("Failed to load conversation turns", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                WorkbenchError::storage("Failed to decode conversation turns", error)
            })?;
        turns.reverse();
        let page = super::history::read_page(&connection, session_id, None, false)?;
        Ok(ConversationSnapshot {
            workspace,
            session,
            active_binding,
            turns,
            items: page.items,
            older_cursor: page.next_cursor,
            sequence: current_sequence(&connection)?,
        })
    }

    pub(crate) fn active_binding(
        &self,
        session_id: &str,
    ) -> WorkbenchResult<Option<SessionBinding>> {
        validate_id("session id", session_id)?;
        load_active_binding(&self.connection()?, session_id)
    }

    pub(crate) fn runtime_root(&self, session_id: &str) -> WorkbenchResult<PathBuf> {
        if let Some(root) = crate::workbench::discovery::runtime_root(self, session_id)? {
            return Ok(root);
        }
        let snapshot = self.session_snapshot(session_id)?;
        if let (Some(workspace), Some(checkpoint)) = (
            snapshot.workspace.as_ref(),
            snapshot
                .session
                .overrides
                .get("projectCheckpointId")
                .and_then(Value::as_str),
        ) {
            return crate::workbench::project::session_task_root(self, &workspace.id, checkpoint);
        }
        if let Some(workspace) = snapshot
            .workspace
            .filter(|workspace| workspace.root.is_some())
        {
            return self.registered_root(&workspace);
        }
        let root = self
            .root
            .join("jobs")
            .join("conversations")
            .join(session_id);
        secure_directory(&root)?;
        Ok(root)
    }

    pub fn reconcile_workspace_roots(
        &self,
        operation_id: &str,
    ) -> WorkbenchResult<ReconcileResult> {
        validate_id("operation id", operation_id)?;
        let mut connection = self.connection()?;
        let roots = {
            let mut statement = connection
                .prepare("SELECT id, root FROM workspaces WHERE root IS NOT NULL ORDER BY id")
                .map_err(|error| WorkbenchError::storage("Failed to inspect workspaces", error))?;
            let roots = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|error| WorkbenchError::storage("Failed to inspect workspaces", error))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| WorkbenchError::storage("Failed to decode workspaces", error))?;
            roots
        };
        let mut states = Vec::with_capacity(roots.len());
        for (workspace_id, root) in roots {
            let exists = match std::fs::metadata(&root) {
                Ok(metadata) => metadata.is_dir(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => {
                    return Err(WorkbenchError::storage(
                        "Failed to inspect workspace root",
                        error,
                    ))
                }
            };
            states.push((workspace_id, exists));
        }
        let timestamp = now();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start reconciliation", error))?;
        ensure_operation_unused(&transaction, operation_id)?;
        let mut missing = 0usize;
        let mut restored = 0usize;
        for (workspace_id, exists) in states {
            let prior: Option<String> = transaction
                .query_row(
                    "SELECT missing_root_at FROM workspaces WHERE id = ?1",
                    [&workspace_id],
                    |row| row.get(0),
                )
                .map_err(|error| WorkbenchError::storage("Failed to reconcile workspace", error))?;
            if !exists && prior.is_none() {
                transaction
                    .execute(
                        "UPDATE workspaces SET missing_root_at = ?1, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                        params![timestamp, workspace_id],
                    )
                    .map_err(|error| WorkbenchError::storage("Failed to mark workspace root missing", error))?;
                missing += 1;
            } else if exists && prior.is_some() {
                transaction
                    .execute(
                        "UPDATE workspaces SET missing_root_at = NULL, revision = revision + 1, updated_at = ?1 WHERE id = ?2",
                        params![timestamp, workspace_id],
                    )
                    .map_err(|error| WorkbenchError::storage("Failed to restore workspace", error))?;
                restored += 1;
            }
        }
        let changed = missing + restored;
        let sequence = insert_change(
            &transaction,
            operation_id,
            "workspaces",
            "all",
            "workspace_roots_reconciled",
            None,
            None,
            &json!({ "changed": changed, "missing": missing, "restored": restored }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit reconciliation", error))?;
        Ok(ReconcileResult {
            changed,
            missing,
            restored,
            sequence,
        })
    }
}
