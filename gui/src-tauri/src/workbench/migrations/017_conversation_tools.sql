-- Network/browser receipts also support conversations without a project.
CREATE TABLE conversation_tool_receipts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    binding_id TEXT NOT NULL REFERENCES session_bindings(id) ON DELETE CASCADE,
    provider_turn_id TEXT NOT NULL,
    call_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('running','completed','failed','outcome_unknown')),
    result_json TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    UNIQUE(binding_id, provider_turn_id, call_id)
);
