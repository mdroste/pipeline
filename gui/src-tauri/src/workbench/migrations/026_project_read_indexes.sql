-- These lists filter by workspace and read newest records first. Keep the
-- claim/freshness and journal sequence indexes for their existing consumers.
CREATE INDEX evidence_workspace_updated ON evidence_links(workspace_id, updated_at DESC);
CREATE INDEX change_log_note_history ON change_log(workspace_id, created_at DESC)
WHERE entity_type = 'research_note' AND action = 'updated';
