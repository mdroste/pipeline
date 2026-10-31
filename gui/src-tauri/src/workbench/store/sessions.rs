//! Sessions persistence operations.

use super::*;

impl Store {
    pub fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        self.create_session_with_overrides(request, None)
    }

    /// Create a session whose settings are in place from its first revision.
    pub fn create_session_with_overrides(
        &self,
        request: CreateSessionRequest,
        overrides: Option<&Value>,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        if let Some(workspace_id) = &request.workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        validate_id("operation id", &request.operation_id)?;
        let title = validate_title(&request.title)?;
        let overrides = overrides
            .map(validate_overrides)
            .transpose()?
            .unwrap_or_else(|| "{}".to_string());
        let id = new_id("session")?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session creation", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        if let Some(workspace_id) = &request.workspace_id {
            let workspace = load_workspace(&transaction, workspace_id)?;
            if workspace.archived_at.is_some() {
                return Err(WorkbenchError::invalid(
                    "Cannot create a session for an archived workspace",
                ));
            }
        }
        transaction
            .execute(
                "INSERT INTO sessions (id, workspace_id, title, overrides_json, draft, revision, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, '', 1, ?5, ?5)",
                params![id, request.workspace_id, title, overrides, timestamp],
            )
            .map_err(|error| WorkbenchError::storage("Failed to create session", error))?;
        let session = load_session(&transaction, &id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &id,
            "session_created",
            request.workspace_id.as_deref(),
            Some(&id),
            &json!({ "revision": 1 }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session creation", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    pub fn update_session(
        &self,
        request: UpdateSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        let title = request.title.as_deref().map(validate_title).transpose()?;
        if let Some(draft) = &request.draft {
            validate_draft(draft)?;
        }
        let overrides = request
            .overrides
            .as_ref()
            .map(validate_overrides)
            .transpose()?;
        if let Some(preset_id) = request.preset_id.as_deref() {
            validate_id("preset id", preset_id)?;
        }
        if let Some(paper_id) = request.paper_id.as_deref() {
            validate_id("paper id", paper_id)?;
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session update", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        if current.revision != request.expected_revision {
            return Err(WorkbenchError::conflict(format!(
                "Session revision changed (expected {}, found {})",
                request.expected_revision, current.revision
            )));
        }
        let next_revision = current.revision + 1;
        let archived_at = match request.archived {
            Some(true) => current.archived_at.or_else(|| Some(timestamp.clone())),
            Some(false) => None,
            None => current.archived_at,
        };
        let paper_id = if request.clear_paper == Some(true) {
            None
        } else {
            request.paper_id.or(current.paper_id)
        };
        transaction
            .execute(
                "UPDATE sessions SET title = ?1, overrides_json = ?2, draft = ?3, revision = ?4, archived_at = ?5, updated_at = ?6, preset_id = ?7, paper_id = ?8 WHERE id = ?9",
                params![
                    title.unwrap_or(current.title),
                    overrides.unwrap_or_else(|| current.overrides.to_string()),
                    request.draft.unwrap_or(current.draft),
                    next_revision,
                    archived_at,
                    timestamp,
                    request.preset_id.or(current.preset_id),
                    paper_id,
                    request.session_id
                ],
            )
            .map_err(|error| WorkbenchError::storage("Failed to update session", error))?;
        let session = load_session(&transaction, &request.session_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_updated",
            session.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({ "revision": next_revision, "archived": session.archived_at.is_some() }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session update", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    /// Lists the most recently updated conversations, newest first.
    ///
    /// The list is capped at `MAX_SESSIONS_PER_LIST` rows. Older
    /// conversations past the cap are omitted rather than failing the whole
    /// list, so a project flooded by recurring automations still opens and
    /// the user can archive or delete conversations from it.
    pub fn list_sessions(
        &self,
        workspace_id: Option<&str>,
        include_archived: bool,
    ) -> WorkbenchResult<SessionList> {
        if let Some(workspace_id) = workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        let connection = self.connection()?;
        let sql = if include_archived {
            "SELECT id, workspace_id, paper_id, title, preset_id, overrides_json, draft, revision, archived_at, created_at, updated_at FROM sessions WHERE id NOT IN (SELECT session_id FROM discovery_roles) AND ((?1 IS NULL AND workspace_id IS NULL) OR workspace_id = ?1) ORDER BY updated_at DESC, id LIMIT ?2"
        } else {
            "SELECT id, workspace_id, paper_id, title, preset_id, overrides_json, draft, revision, archived_at, created_at, updated_at FROM sessions WHERE id NOT IN (SELECT session_id FROM discovery_roles) AND ((?1 IS NULL AND workspace_id IS NULL) OR workspace_id = ?1) AND archived_at IS NULL ORDER BY updated_at DESC, id LIMIT ?2"
        };
        let mut statement = connection
            .prepare(sql)
            .map_err(|error| WorkbenchError::storage("Failed to prepare session list", error))?;
        let sessions = statement
            .query_map(
                params![workspace_id, MAX_SESSIONS_PER_LIST as i64],
                read_session_row,
            )
            .map_err(|error| WorkbenchError::storage("Failed to list sessions", error))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode sessions", error))?;
        Ok(SessionList {
            sessions,
            sequence: current_sequence(&connection)?,
        })
    }

    /// Files a conversation under another Workspace, or unfiles it.
    ///
    /// The effective harness fingerprints the Workspace, so the next turn
    /// starts a successor native thread through the ordinary handoff path.
    /// Records that belong to the current Workspace (recipe runs, tool
    /// receipts, executions, curated context) pin the conversation there.
    pub fn move_session(
        &self,
        request: MoveSessionRequest,
    ) -> WorkbenchResult<Mutation<WorkbenchSession>> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        if let Some(workspace_id) = &request.workspace_id {
            validate_id("workspace id", workspace_id)?;
        }
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session move", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        require_revision(current.revision, request.expected_revision, "Session")?;
        if current.workspace_id == request.workspace_id {
            return Err(WorkbenchError::invalid(
                "The conversation is already filed there",
            ));
        }
        if current.overrides.get("projectCheckpointId").is_some() {
            return Err(WorkbenchError::invalid(
                "Task conversations stay with the project that owns their task copy",
            ));
        }
        if let Some(workspace_id) = &request.workspace_id {
            let workspace = load_workspace(&transaction, workspace_id)?;
            if workspace.archived_at.is_some() {
                return Err(WorkbenchError::invalid(
                    "Cannot move a conversation into an archived workspace",
                ));
            }
        }
        let bound_records: i64 = transaction
            .query_row(
                "SELECT (SELECT COUNT(*) FROM recipe_runs WHERE session_id = ?1) + (SELECT COUNT(*) FROM tool_receipts WHERE session_id = ?1) + (SELECT COUNT(*) FROM research_executions WHERE session_id = ?1) + (SELECT COUNT(*) FROM session_context_items WHERE session_id = ?1)",
                [&request.session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to inspect conversation records", error))?;
        if bound_records > 0 {
            return Err(WorkbenchError::invalid(
                "This conversation has research records in its current Workspace and cannot be moved",
            ));
        }
        let next_revision = current.revision + 1;
        transaction
            .execute(
                "UPDATE sessions SET workspace_id = ?1, paper_id = NULL, revision = ?2, updated_at = ?3 WHERE id = ?4",
                params![request.workspace_id, next_revision, timestamp, request.session_id],
            )
            .map_err(|error| map_constraint("Failed to move session", error))?;
        let session = load_session(&transaction, &request.session_id)?;
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_moved",
            session.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({
                "revision": next_revision,
                "fromWorkspaceId": current.workspace_id,
                "toWorkspaceId": session.workspace_id,
                "paperCleared": current.paper_id.is_some()
            }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session move", error))?;
        Ok(Mutation {
            record: session,
            sequence,
        })
    }

    /// Permanently removes a conversation with its native bindings, turns,
    /// transcript, snapshots, receipts, and recipe runs. Executions and Review
    /// handoffs keep their records with the conversation reference cleared.
    pub fn delete_session(&self, request: DeleteSessionRequest) -> WorkbenchResult<i64> {
        validate_id("session id", &request.session_id)?;
        validate_id("operation id", &request.operation_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session deletion", error))?;
        ensure_operation_unused(&transaction, &request.operation_id)?;
        let current = load_session(&transaction, &request.session_id)?;
        let running: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM research_executions WHERE session_id = ?1 AND outcome IN ('queued', 'running')",
                [&request.session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to inspect conversation jobs", error))?;
        if running > 0 {
            return Err(WorkbenchError::conflict(
                "Stop this conversation's local jobs before deleting it",
            ));
        }
        for sql in [
            "DELETE FROM transcript_items WHERE binding_id IN (SELECT id FROM session_bindings WHERE session_id = ?1)",
            "DELETE FROM turns WHERE binding_id IN (SELECT id FROM session_bindings WHERE session_id = ?1)",
            "DELETE FROM session_bindings WHERE session_id = ?1",
            "DELETE FROM config_snapshots WHERE session_id = ?1",
            "DELETE FROM context_snapshots WHERE session_id = ?1",
            "DELETE FROM sessions WHERE id = ?1",
        ] {
            transaction
                .execute(sql, [&request.session_id])
                .map_err(|error| map_constraint("Failed to delete session", error))?;
        }
        let sequence = insert_change(
            &transaction,
            &request.operation_id,
            "session",
            &request.session_id,
            "session_deleted",
            current.workspace_id.as_deref(),
            Some(&request.session_id),
            &json!({ "title": current.title }),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session deletion", error))?;
        Ok(sequence)
    }

    /// Native turns already projected for a binding's thread, keyed by
    /// provider turn id, with whether their newest projection is terminal.
    /// Every incarnation of the conversation on that thread counts, matching
    /// the transcript view's newest-incarnation rule, so reconciliation can
    /// skip native history the store already holds.
    pub(crate) fn projected_turn_states(
        &self,
        binding_id: &str,
    ) -> WorkbenchResult<std::collections::HashMap<String, bool>> {
        validate_id("binding id", binding_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT t.provider_turn_id, t.terminal_at IS NOT NULL FROM session_bindings current JOIN session_bindings b ON b.session_id = current.session_id AND b.provider_thread_id = current.provider_thread_id JOIN turns t ON t.binding_id = b.id WHERE current.id = ?1 AND t.provider_turn_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM turns newer JOIN session_bindings nb ON nb.id = newer.binding_id WHERE nb.session_id = b.session_id AND newer.provider_turn_id = t.provider_turn_id AND nb.incarnation > b.incarnation)",
            )
            .map_err(|error| WorkbenchError::storage("Failed to prepare projected turns", error))?;
        let states = statement
            .query_map([binding_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|error| WorkbenchError::storage("Failed to read projected turns", error))?
            .collect::<Result<_, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode projected turns", error))?;
        Ok(states)
    }

    /// The conversation currently bound to a live native thread, if any.
    pub(crate) fn session_for_thread(
        &self,
        runtime_namespace: &str,
        provider_thread_id: &str,
    ) -> WorkbenchResult<Option<String>> {
        validate_id("runtime namespace", runtime_namespace)?;
        validate_provider_id("provider thread id", provider_thread_id)?;
        self.connection()?
            .query_row(
                "SELECT session_id FROM session_bindings WHERE runtime_namespace = ?1 AND provider_thread_id = ?2 AND retired_at IS NULL ORDER BY incarnation DESC LIMIT 1",
                params![runtime_namespace, provider_thread_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to resolve thread binding", error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_sessions_returns_the_newest_rows_past_its_cap() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open_at(&temporary.path().join("workbench-store")).unwrap();
        let mut connection = store.connection().unwrap();
        let transaction = connection.transaction().unwrap();
        let total = MAX_SESSIONS_PER_LIST + 3;
        for index in 0..total {
            let timestamp = format!("2026-01-01T00:00:00.{index:06}+00:00");
            transaction
                .execute(
                    "INSERT INTO sessions (id, workspace_id, title, overrides_json, draft, revision, created_at, updated_at) VALUES (?1, NULL, 'Recurring follow-up', '{}', '', 1, ?2, ?2)",
                    params![format!("session_{index:04}"), timestamp],
                )
                .unwrap();
        }
        transaction.commit().unwrap();

        let listed = store.list_sessions(None, false).unwrap().sessions;
        assert_eq!(listed.len(), MAX_SESSIONS_PER_LIST);
        assert_eq!(listed[0].id, format!("session_{:04}", total - 1));
        assert_eq!(listed.last().unwrap().id, "session_0003");
        assert_eq!(
            store.list_sessions(None, true).unwrap().sessions.len(),
            MAX_SESSIONS_PER_LIST
        );
    }

    #[test]
    fn projected_turn_states_follow_the_newest_incarnation_of_the_thread() {
        use crate::agent_runtime::events::NormalizedEvent;
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::open_at(&temporary.path().join("workbench-store")).unwrap();
        let session = store
            .create_session(CreateSessionRequest {
                workspace_id: None,
                title: "Identification notes".to_string(),
                operation_id: "session-create".to_string(),
            })
            .unwrap()
            .record;
        let project = |binding: &str, epoch: u64, turn: &str, status: &str| {
            let thread_id = "thread-1".to_string();
            let turn_id = turn.to_string();
            let payload = json!({ "id": turn, "status": status, "items": [] });
            let event = if status == "inProgress" {
                NormalizedEvent::TurnStarted {
                    epoch,
                    thread_id,
                    turn_id,
                    turn: payload,
                }
            } else {
                NormalizedEvent::TurnCompleted {
                    epoch,
                    thread_id,
                    turn_id,
                    status: status.to_string(),
                    turn: payload,
                }
            };
            store.project_codex_event(binding, &event).unwrap();
        };
        let first = store
            .bind_session(&session.id, "codex-0-147-0-epoch-1", "thread-1")
            .unwrap();
        project(&first.id, 1, "turn-a", "completed");
        project(&first.id, 1, "turn-b", "inProgress");
        let second = store
            .bind_session(&session.id, "codex-0-147-0-epoch-2", "thread-1")
            .unwrap();
        project(&second.id, 2, "turn-a", "inProgress");
        project(&second.id, 2, "turn-c", "interrupted");

        let states = store.projected_turn_states(&second.id).unwrap();
        assert_eq!(states.len(), 3);
        assert!(!states["turn-a"]);
        assert!(!states["turn-b"]);
        assert!(states["turn-c"]);

        let successor = store
            .bind_session(&session.id, "codex-0-147-0-epoch-3", "thread-2")
            .unwrap();
        assert!(store
            .projected_turn_states(&successor.id)
            .unwrap()
            .is_empty());
    }
}
