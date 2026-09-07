CREATE TABLE project_workspaces (
    project_id TEXT PRIMARY KEY,
    root TEXT NOT NULL UNIQUE,
    root_identity TEXT NOT NULL,
    settings_revision INTEGER NOT NULL DEFAULT 0 CHECK (settings_revision >= 0),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    archived_at TEXT,
    orphaned_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES project_workspaces(project_id) ON DELETE RESTRICT,
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
CREATE INDEX sessions_project_updated ON sessions(project_id, updated_at DESC);

CREATE TABLE session_bindings (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE RESTRICT,
    runtime_namespace TEXT NOT NULL,
    provider_thread_id TEXT NOT NULL,
    incarnation INTEGER NOT NULL CHECK (incarnation >= 1),
    created_at TEXT NOT NULL,
    retired_at TEXT,
    retirement_reason TEXT,
    UNIQUE(session_id, incarnation),
    UNIQUE(runtime_namespace, provider_thread_id)
);

CREATE TABLE config_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE RESTRICT,
    schema_version INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE context_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE RESTRICT,
    schema_version INTEGER NOT NULL,
    manifest_json TEXT NOT NULL,
    body_reference TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE turns (
    id TEXT PRIMARY KEY,
    binding_id TEXT NOT NULL REFERENCES session_bindings(id) ON DELETE RESTRICT,
    client_submission_id TEXT NOT NULL,
    provider_turn_id TEXT,
    state TEXT NOT NULL,
    instruction_snapshot_id TEXT,
    config_snapshot_id TEXT REFERENCES config_snapshots(id) ON DELETE RESTRICT,
    context_snapshot_id TEXT REFERENCES context_snapshots(id) ON DELETE RESTRICT,
    usage_json TEXT,
    error_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    terminal_at TEXT,
    UNIQUE(binding_id, client_submission_id)
);

CREATE TABLE transcript_items (
    id TEXT PRIMARY KEY,
    binding_id TEXT NOT NULL REFERENCES session_bindings(id) ON DELETE RESTRICT,
    turn_id TEXT REFERENCES turns(id) ON DELETE RESTRICT,
    provider_item_id TEXT NOT NULL,
    item_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    is_final INTEGER NOT NULL CHECK (is_final IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(binding_id, provider_item_id)
);

CREATE TABLE change_log (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL UNIQUE,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    project_id TEXT,
    session_id TEXT,
    details_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX change_log_project_sequence ON change_log(project_id, sequence);
CREATE INDEX change_log_session_sequence ON change_log(session_id, sequence);
