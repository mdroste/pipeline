//! Runtime persistence operations.

use super::*;

impl Store {
    pub(crate) fn bind_session(
        &self,
        session_id: &str,
        runtime_namespace: &str,
        provider_thread_id: &str,
    ) -> WorkbenchResult<SessionBinding> {
        validate_id("session id", session_id)?;
        validate_id("runtime namespace", runtime_namespace)?;
        validate_provider_id("provider thread id", provider_thread_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start session binding", error))?;
        let session = load_session(&transaction, session_id)?;
        // A new connection reopening the same thread keeps what that thread was
        // started with, so the next send can still resume it.
        let mut same_thread = None;
        if let Some(existing) = load_active_binding(&transaction, session_id)? {
            if existing.runtime_namespace == runtime_namespace
                && existing.provider_thread_id == provider_thread_id
            {
                return Ok(existing);
            }
            if existing.provider_thread_id == provider_thread_id {
                same_thread = Some(existing.id.clone());
            }
            transaction
                .execute(
                    "UPDATE session_bindings SET retired_at = ?1, retirement_reason = 'superseded' WHERE id = ?2",
                    params![timestamp, existing.id],
                )
                .map_err(|error| WorkbenchError::storage("Failed to retire session binding", error))?;
        }
        let incarnation: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(incarnation), 0) + 1 FROM session_bindings WHERE session_id = ?1",
                [session_id],
                |row| row.get(0),
            )
            .map_err(|error| WorkbenchError::storage("Failed to allocate binding incarnation", error))?;
        let id = new_id("binding")?;
        transaction
            .execute(
                "INSERT INTO session_bindings (id, session_id, runtime_namespace, provider_thread_id, incarnation, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, session_id, runtime_namespace, provider_thread_id, incarnation, timestamp],
            )
            .map_err(|error| map_constraint("Failed to bind Workbench session", error))?;
        if let Some(previous) = same_thread {
            transaction
                .execute(
                    "UPDATE session_bindings SET (harness_fingerprint, instruction_sources_json) = (SELECT harness_fingerprint, instruction_sources_json FROM session_bindings WHERE id = ?2) WHERE id = ?1",
                    params![id, previous],
                )
                .map_err(|error| WorkbenchError::storage("Failed to carry over the thread's harness", error))?;
        }
        insert_change(
            &transaction,
            &format!("binding-create-{id}"),
            "session_binding",
            &id,
            "session_bound",
            session.workspace_id.as_deref(),
            Some(session_id),
            &json!({
                "runtimeNamespace": runtime_namespace,
                "providerThreadId": provider_thread_id,
                "incarnation": incarnation
            }),
            &timestamp,
        )?;
        let binding = load_binding(&transaction, &id)?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit session binding", error))?;
        Ok(binding)
    }

    pub(crate) fn record_turn_submission(
        &self,
        binding_id: &str,
        client_submission_id: &str,
        provider_turn_id: &str,
    ) -> WorkbenchResult<()> {
        validate_id("binding id", binding_id)?;
        validate_id("client submission id", client_submission_id)?;
        validate_provider_id("provider turn id", provider_turn_id)?;
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                WorkbenchError::storage("Failed to start turn reconciliation", error)
            })?;
        let turn_id: Option<String> = transaction
            .query_row(
                "SELECT id FROM turns WHERE binding_id = ?1 AND provider_turn_id = ?2",
                params![binding_id, provider_turn_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| WorkbenchError::storage("Failed to inspect projected turn", error))?;
        if let Some(turn_id) = turn_id {
            transaction
                .execute(
                    "UPDATE turns SET client_submission_id = ?1, updated_at = ?2 WHERE id = ?3",
                    params![client_submission_id, timestamp, turn_id],
                )
                .map_err(|error| map_constraint("Failed to attach turn submission", error))?;
        } else {
            let id = new_id("turn")?;
            transaction
                .execute(
                    "INSERT INTO turns (id, binding_id, client_submission_id, provider_turn_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'inProgress', ?5, ?5)",
                    params![id, binding_id, client_submission_id, provider_turn_id, timestamp],
                )
                .map_err(|error| map_constraint("Failed to record turn submission", error))?;
        }
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit turn reconciliation", error))
    }

    /// Provider turns of this binding with no recorded ending.
    pub(crate) fn open_provider_turns(&self, binding_id: &str) -> WorkbenchResult<Vec<String>> {
        validate_id("binding id", binding_id)?;
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT provider_turn_id FROM turns WHERE binding_id = ?1 AND terminal_at IS NULL AND provider_turn_id IS NOT NULL ORDER BY created_at")
            .map_err(|error| WorkbenchError::storage("Failed to prepare open turns", error))?;
        let turns = statement
            .query_map([binding_id], |row| row.get(0))
            .map_err(|error| WorkbenchError::storage("Failed to read open turns", error))?
            .collect::<Result<_, _>>()
            .map_err(|error| WorkbenchError::storage("Failed to decode open turns", error))?;
        Ok(turns)
    }

    pub(crate) fn set_binding_harness(
        &self,
        binding_id: &str,
        fingerprint: &str,
        instruction_sources: &[String],
    ) -> WorkbenchResult<()> {
        validate_id("binding id", binding_id)?;
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(WorkbenchError::invalid("Harness fingerprint is invalid"));
        }
        let instruction_sources = serde_json::to_string(instruction_sources).map_err(|error| {
            WorkbenchError::storage("Failed to encode native instruction sources", error)
        })?;
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE session_bindings SET harness_fingerprint = ?2, instruction_sources_json = ?3 WHERE id = ?1",
                params![binding_id, fingerprint, instruction_sources],
            )
            .map_err(|error| WorkbenchError::storage("Failed to bind harness snapshot", error))?;
        if changed != 1 {
            return Err(WorkbenchError::invalid(
                "Workbench session binding does not exist",
            ));
        }
        Ok(())
    }

    pub(crate) fn attach_turn_snapshots(
        &self,
        binding_id: &str,
        client_submission_id: &str,
        config_snapshot_id: &str,
        context_snapshot_id: &str,
    ) -> WorkbenchResult<()> {
        for (label, value) in [
            ("binding id", binding_id),
            ("client submission id", client_submission_id),
            ("configuration snapshot id", config_snapshot_id),
            ("context snapshot id", context_snapshot_id),
        ] {
            validate_id(label, value)?;
        }
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE turns SET config_snapshot_id = ?3, context_snapshot_id = ?4, instruction_snapshot_id = ?3 WHERE binding_id = ?1 AND client_submission_id = ?2",
                params![binding_id, client_submission_id, config_snapshot_id, context_snapshot_id],
            )
            .map_err(|error| WorkbenchError::storage("Failed to attach immutable turn snapshots", error))?;
        if changed != 1 {
            return Err(WorkbenchError::invalid(
                "Submitted turn could not be linked to its snapshots",
            ));
        }
        Ok(())
    }

    pub(crate) fn project_codex_event(
        &self,
        binding_id: &str,
        event: &crate::agent_runtime::events::NormalizedEvent,
    ) -> WorkbenchResult<Option<i64>> {
        use crate::agent_runtime::events::NormalizedEvent;
        validate_id("binding id", binding_id)?;
        let (thread_id, operation_id) = match event {
            NormalizedEvent::TurnStarted {
                epoch,
                thread_id,
                turn_id,
                ..
            } => (thread_id, format!("codex-{epoch}-turn-{turn_id}-started")),
            NormalizedEvent::TurnCompleted {
                epoch,
                thread_id,
                turn_id,
                ..
            } => (thread_id, format!("codex-{epoch}-turn-{turn_id}-completed")),
            NormalizedEvent::ItemStarted {
                epoch,
                thread_id,
                item_id,
                ..
            } => (thread_id, format!("codex-{epoch}-item-{item_id}-started")),
            NormalizedEvent::ItemCompleted {
                epoch,
                thread_id,
                item_id,
                ..
            } => (thread_id, format!("codex-{epoch}-item-{item_id}-completed")),
            _ => return Ok(None),
        };
        let timestamp = now();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| WorkbenchError::storage("Failed to start event projection", error))?;
        let binding = load_binding(&transaction, binding_id)?;
        if binding.provider_thread_id != *thread_id {
            return Err(WorkbenchError::invalid(
                "Codex event thread does not match its Workbench binding",
            ));
        }
        if let Some(sequence) = change_sequence(&transaction, &operation_id)? {
            return Ok(Some(sequence));
        }
        match event {
            NormalizedEvent::TurnStarted { turn_id, turn, .. } => {
                upsert_projected_turn(&transaction, binding_id, turn_id, turn, &timestamp, false)?;
            }
            NormalizedEvent::TurnCompleted { turn_id, turn, .. } => {
                upsert_projected_turn(&transaction, binding_id, turn_id, turn, &timestamp, true)?;
                if let Some(items) = turn.get("items").and_then(Value::as_array) {
                    for (index, item) in items.iter().enumerate() {
                        let item_id = required_json_string(item, "id", "completed turn item")?;
                        let item_kind = required_json_string(item, "type", "completed turn item")?;
                        // Re-projection under a new incarnation must keep the
                        // transcript order: inherit the prior row's time, and
                        // give genuinely new rows strictly increasing times
                        // (random ids cannot order items within one batch).
                        let item_timestamp =
                            prior_item_created_at(&transaction, &binding.session_id, item_id)?
                                .unwrap_or_else(|| offset_rfc3339(&timestamp, index));
                        upsert_projected_item(
                            &transaction,
                            binding_id,
                            turn_id,
                            item_id,
                            item_kind,
                            item,
                            true,
                            &item_timestamp,
                        )?;
                    }
                }
            }
            NormalizedEvent::ItemStarted {
                turn_id,
                item_id,
                item_kind,
                item,
                ..
            } => upsert_projected_item(
                &transaction,
                binding_id,
                turn_id,
                item_id,
                item_kind,
                item,
                false,
                &prior_item_created_at(&transaction, &binding.session_id, item_id)?
                    .unwrap_or_else(|| timestamp.clone()),
            )?,
            NormalizedEvent::ItemCompleted {
                turn_id,
                item_id,
                item_kind,
                item,
                ..
            } => upsert_projected_item(
                &transaction,
                binding_id,
                turn_id,
                item_id,
                item_kind,
                item,
                true,
                &prior_item_created_at(&transaction, &binding.session_id, item_id)?
                    .unwrap_or_else(|| timestamp.clone()),
            )?,
            _ => unreachable!("non-projectable events returned above"),
        }
        let session = load_session(&transaction, &binding.session_id)?;
        let sequence = insert_change(
            &transaction,
            &operation_id,
            "codex_projection",
            binding_id,
            "provider_event_projected",
            session.workspace_id.as_deref(),
            Some(&binding.session_id),
            &json!({"providerThreadId": thread_id}),
            &timestamp,
        )?;
        transaction
            .commit()
            .map_err(|error| WorkbenchError::storage("Failed to commit event projection", error))?;
        Ok(Some(sequence))
    }
}

/// The ordering time an earlier incarnation already assigned to this
/// provider item, if any. Same-incarnation upserts keep created_at anyway.
fn prior_item_created_at(
    connection: &Connection,
    session_id: &str,
    provider_item_id: &str,
) -> WorkbenchResult<Option<String>> {
    connection
        .query_row(
            "SELECT i.created_at FROM transcript_items i JOIN session_bindings b ON b.id = i.binding_id WHERE b.session_id = ?1 AND i.provider_item_id = ?2 ORDER BY b.incarnation DESC LIMIT 1",
            params![session_id, provider_item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| WorkbenchError::storage("Failed to read prior transcript item", error))
}

/// `timestamp` plus `index` nanoseconds in the same RFC 3339 layout `now()`
/// produces, so text ordering in SQLite matches time ordering.
fn offset_rfc3339(timestamp: &str, index: usize) -> String {
    if index == 0 {
        return timestamp.to_string();
    }
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .map(|time| (time + chrono::Duration::nanoseconds(index as i64)).to_rfc3339())
        .unwrap_or_else(|_| timestamp.to_string())
}
