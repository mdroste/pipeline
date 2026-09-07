CREATE TABLE workspaces_v2 (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root TEXT UNIQUE,
    root_identity TEXT,
    settings_revision INTEGER NOT NULL DEFAULT 0 CHECK (settings_revision >= 0),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    archived_at TEXT,
    missing_root_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((root IS NULL AND root_identity IS NULL) OR (root IS NOT NULL AND root_identity IS NOT NULL))
);

INSERT INTO workspaces_v2 (
    id, name, root, root_identity, settings_revision, revision,
    archived_at, missing_root_at, created_at, updated_at
)
SELECT
    project_id, project_id, root, root_identity, settings_revision, revision,
    archived_at, NULL, created_at, updated_at
FROM project_workspaces;

CREATE TABLE sessions_v2 (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces_v2(id) ON DELETE SET NULL,
    paper_id TEXT,
    title TEXT NOT NULL,
    preset_id TEXT,
    overrides_json TEXT NOT NULL DEFAULT '{}',
    draft TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO sessions_v2 (
    id, workspace_id, paper_id, title, preset_id, overrides_json, draft,
    revision, archived_at, created_at, updated_at
)
SELECT
    id, project_id, paper_id, title, preset_id, overrides_json, draft,
    revision, archived_at, created_at, updated_at
FROM sessions;

CREATE TABLE session_bindings_v2 (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions_v2(id) ON DELETE RESTRICT,
    runtime_namespace TEXT NOT NULL,
    provider_thread_id TEXT NOT NULL,
    incarnation INTEGER NOT NULL CHECK (incarnation >= 1),
    created_at TEXT NOT NULL,
    retired_at TEXT,
    retirement_reason TEXT,
    UNIQUE(session_id, incarnation),
    UNIQUE(runtime_namespace, provider_thread_id)
);
INSERT INTO session_bindings_v2 SELECT * FROM session_bindings;

CREATE TABLE config_snapshots_v2 (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions_v2(id) ON DELETE RESTRICT,
    schema_version INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO config_snapshots_v2 SELECT * FROM config_snapshots;

CREATE TABLE context_snapshots_v2 (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions_v2(id) ON DELETE RESTRICT,
    schema_version INTEGER NOT NULL,
    manifest_json TEXT NOT NULL,
    body_reference TEXT,
    created_at TEXT NOT NULL
);
INSERT INTO context_snapshots_v2 SELECT * FROM context_snapshots;

CREATE TABLE turns_v2 (
    id TEXT PRIMARY KEY,
    binding_id TEXT NOT NULL REFERENCES session_bindings_v2(id) ON DELETE RESTRICT,
    client_submission_id TEXT NOT NULL,
    provider_turn_id TEXT,
    state TEXT NOT NULL,
    instruction_snapshot_id TEXT,
    config_snapshot_id TEXT REFERENCES config_snapshots_v2(id) ON DELETE RESTRICT,
    context_snapshot_id TEXT REFERENCES context_snapshots_v2(id) ON DELETE RESTRICT,
    usage_json TEXT,
    error_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    terminal_at TEXT,
    UNIQUE(binding_id, client_submission_id)
);
INSERT INTO turns_v2 SELECT * FROM turns;

CREATE TABLE transcript_items_v2 (
    id TEXT PRIMARY KEY,
    binding_id TEXT NOT NULL REFERENCES session_bindings_v2(id) ON DELETE RESTRICT,
    turn_id TEXT REFERENCES turns_v2(id) ON DELETE RESTRICT,
    provider_item_id TEXT NOT NULL,
    item_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    is_final INTEGER NOT NULL CHECK (is_final IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(binding_id, provider_item_id)
);
INSERT INTO transcript_items_v2 SELECT * FROM transcript_items;

CREATE TABLE change_log_v2 (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL UNIQUE,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    workspace_id TEXT,
    session_id TEXT,
    details_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
INSERT INTO change_log_v2 (
    sequence, operation_id, entity_type, entity_id, action,
    workspace_id, session_id, details_json, created_at
)
SELECT
    sequence, operation_id, entity_type, entity_id, action,
    project_id, session_id, details_json, created_at
FROM change_log;

DROP TABLE transcript_items;
DROP TABLE turns;
DROP TABLE context_snapshots;
DROP TABLE config_snapshots;
DROP TABLE session_bindings;
DROP TABLE sessions;
DROP TABLE project_workspaces;
DROP TABLE change_log;

ALTER TABLE workspaces_v2 RENAME TO workspaces;
ALTER TABLE sessions_v2 RENAME TO sessions;
ALTER TABLE session_bindings_v2 RENAME TO session_bindings;
ALTER TABLE config_snapshots_v2 RENAME TO config_snapshots;
ALTER TABLE context_snapshots_v2 RENAME TO context_snapshots;
ALTER TABLE turns_v2 RENAME TO turns;
ALTER TABLE transcript_items_v2 RENAME TO transcript_items;
ALTER TABLE change_log_v2 RENAME TO change_log;

CREATE INDEX sessions_workspace_updated ON sessions(workspace_id, updated_at DESC);
CREATE INDEX sessions_unfiled_updated ON sessions(updated_at DESC) WHERE workspace_id IS NULL;
CREATE INDEX change_log_workspace_sequence ON change_log(workspace_id, sequence);
CREATE INDEX change_log_session_sequence ON change_log(session_id, sequence);
