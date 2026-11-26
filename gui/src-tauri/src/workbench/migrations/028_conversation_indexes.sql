-- Inventory records are folder snapshots that every project check rewrites.
-- Nothing reads their history, so stop copying each replaced snapshot and drop
-- the copies already kept. Other kinds keep their history unchanged.
DROP TRIGGER project_history;
CREATE TRIGGER project_history BEFORE UPDATE ON project_records WHEN OLD.kind <> 'inventory' BEGIN
 INSERT OR IGNORE INTO project_record_history VALUES(OLD.id,OLD.revision,OLD.workspace_id,OLD.kind,OLD.body_json,OLD.updated_at);
END;
DELETE FROM project_record_history WHERE kind = 'inventory';

-- A turn's items, newest first, for activity and reply reads.
CREATE INDEX transcript_items_turn ON transcript_items(turn_id, created_at, id);
-- Rows that deleting a conversation removes or must check for references.
CREATE INDEX turns_config_snapshot ON turns(config_snapshot_id);
CREATE INDEX turns_context_snapshot ON turns(context_snapshot_id);
CREATE INDEX config_snapshots_session ON config_snapshots(session_id);
CREATE INDEX context_snapshots_session ON context_snapshots(session_id);
CREATE INDEX research_executions_session ON research_executions(session_id);
CREATE INDEX research_executions_binding ON research_executions(binding_id);
CREATE INDEX tool_receipts_session ON tool_receipts(session_id);
CREATE INDEX conversation_tool_receipts_session ON conversation_tool_receipts(session_id);
CREATE INDEX review_handoffs_session ON review_handoffs(session_id);
-- Tool calls find their conversation by provider thread; datasets and sources
-- look for a newer version; search reads a project's note versions.
CREATE INDEX session_bindings_thread ON session_bindings(provider_thread_id);
CREATE INDEX desk_records_supersedes ON desk_records(supersedes);
CREATE INDEX desk_note_versions_workspace ON desk_note_versions(workspace_id);
