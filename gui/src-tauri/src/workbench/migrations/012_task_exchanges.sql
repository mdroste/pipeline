-- Explicit immutable exchanges. Task execution remains in its own database.
CREATE TABLE task_exchanges (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  resolved_task_id TEXT,
  kind TEXT NOT NULL CHECK(kind IN ('proposal','result')),
  value_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX task_exchange_session ON task_exchanges(session_id,kind,created_at);
