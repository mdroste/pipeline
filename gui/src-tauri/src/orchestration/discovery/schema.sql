BEGIN IMMEDIATE;
CREATE TABLE discovery_runs (
 id TEXT PRIMARY KEY, operation TEXT UNIQUE NOT NULL, fingerprint TEXT NOT NULL,
 revision INTEGER NOT NULL, state TEXT NOT NULL, workspace_id TEXT NOT NULL,
 due_at INTEGER, deadline_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, body TEXT NOT NULL
);
CREATE INDEX discovery_due ON discovery_runs(state,due_at);
CREATE INDEX discovery_project ON discovery_runs(workspace_id,updated_at DESC);
CREATE TABLE discovery_records (
 run_id TEXT NOT NULL REFERENCES discovery_runs(id), kind TEXT NOT NULL,
 ordinal INTEGER NOT NULL, body TEXT NOT NULL, PRIMARY KEY(run_id,kind,ordinal)
);
CREATE TABLE discovery_children (
 task_id TEXT PRIMARY KEY REFERENCES runs(id), run_id TEXT NOT NULL REFERENCES discovery_runs(id),
 phase TEXT NOT NULL, adopted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX discovery_child_owner ON discovery_children(run_id,adopted);
CREATE TABLE discovery_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, run_id TEXT NOT NULL REFERENCES discovery_runs(id),
 at INTEGER NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL
);
CREATE INDEX discovery_event_owner ON discovery_events(run_id,sequence);
CREATE TABLE discovery_selections (
 operation TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES discovery_runs(id),
 fingerprint TEXT NOT NULL
);
PRAGMA user_version=3;
COMMIT;
