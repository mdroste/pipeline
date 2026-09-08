BEGIN IMMEDIATE;
CREATE TABLE missions (
 id TEXT PRIMARY KEY, operation TEXT NOT NULL UNIQUE, fingerprint TEXT NOT NULL,
 revision INTEGER NOT NULL, state TEXT NOT NULL, workspace_id TEXT NOT NULL,
 due_at INTEGER, updated_at INTEGER NOT NULL, body TEXT NOT NULL
);
CREATE INDEX mission_due ON missions(state,due_at);
CREATE INDEX mission_project ON missions(workspace_id,updated_at DESC);
CREATE TABLE mission_children (
 task_id TEXT PRIMARY KEY REFERENCES runs(id), mission_id TEXT NOT NULL REFERENCES missions(id),
 phase TEXT NOT NULL, round INTEGER NOT NULL, adopted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX mission_child_owner ON mission_children(mission_id,adopted);
CREATE TABLE mission_events (
 sequence INTEGER PRIMARY KEY AUTOINCREMENT, mission_id TEXT NOT NULL REFERENCES missions(id),
 at INTEGER NOT NULL, kind TEXT NOT NULL, detail TEXT NOT NULL
);
CREATE INDEX mission_event_owner ON mission_events(mission_id,sequence);
CREATE TABLE mission_answers (
 operation TEXT PRIMARY KEY, mission_id TEXT NOT NULL REFERENCES missions(id),
 question_id TEXT NOT NULL, answer TEXT NOT NULL
);
PRAGMA user_version=2;
COMMIT;
