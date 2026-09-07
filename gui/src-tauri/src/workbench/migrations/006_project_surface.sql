-- App-owned, versioned project records. Payloads are validated by typed services.
CREATE TABLE project_records (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('home','inventory','anchor','task','checkpoint','application')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    body_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX project_records_scope ON project_records(workspace_id,kind,updated_at DESC);
CREATE TABLE project_operations (
    operation_id TEXT PRIMARY KEY,
    request_hash TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE TABLE execution_authorizations (
    profile_id TEXT PRIMARY KEY REFERENCES execution_profiles(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    authorized_at TEXT NOT NULL
);
