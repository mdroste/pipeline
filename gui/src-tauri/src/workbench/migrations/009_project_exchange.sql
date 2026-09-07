-- Selective project exchange (coauthor packages) and recoverable storage retention.
CREATE TABLE exchange_imports (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 package_hash TEXT NOT NULL,
 source_namespace TEXT NOT NULL,
 manifest_json TEXT NOT NULL,
 decisions_json TEXT NOT NULL,
 summary_json TEXT NOT NULL,
 imported_at TEXT NOT NULL
);
CREATE INDEX exchange_imports_workspace ON exchange_imports(workspace_id, imported_at DESC);
CREATE TABLE exchange_conflicts (
 id TEXT PRIMARY KEY,
 import_id TEXT NOT NULL REFERENCES exchange_imports(id) ON DELETE CASCADE,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 object_kind TEXT NOT NULL,
 object_id TEXT NOT NULL,
 local_json TEXT NOT NULL,
 imported_json TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('open','kept_local','took_imported')),
 recorded_at TEXT NOT NULL,
 resolved_at TEXT
);
CREATE INDEX exchange_conflicts_workspace ON exchange_conflicts(workspace_id, state, recorded_at DESC);
CREATE TABLE storage_trash (
 id TEXT PRIMARY KEY,
 category TEXT NOT NULL,
 original_path TEXT NOT NULL,
 trash_path TEXT NOT NULL,
 size_bytes INTEGER NOT NULL CHECK(size_bytes >= 0),
 reason TEXT NOT NULL,
 moved_at TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('trashed','restored','deleted')),
 finished_at TEXT
);
CREATE INDEX storage_trash_state ON storage_trash(state, moved_at DESC);
