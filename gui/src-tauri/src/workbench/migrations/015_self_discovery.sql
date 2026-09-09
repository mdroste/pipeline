CREATE TABLE discovery_roles (
 session_id TEXT PRIMARY KEY REFERENCES sessions(id), workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 run_key TEXT NOT NULL, root_key TEXT NOT NULL, writable INTEGER NOT NULL,
 enabled INTEGER NOT NULL DEFAULT 1, deadline_at INTEGER NOT NULL
);
CREATE INDEX discovery_role_run ON discovery_roles(run_key,enabled);
