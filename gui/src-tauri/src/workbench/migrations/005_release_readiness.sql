CREATE TABLE recipe_definitions (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
    source_recipe_id TEXT,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT NOT NULL,
    required_inputs_json TEXT NOT NULL,
    required_tools_json TEXT NOT NULL,
    suggested_permission_mode TEXT NOT NULL CHECK (suggested_permission_mode IN ('inspect', 'edit')),
    expected_checks_json TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX recipe_definitions_workspace ON recipe_definitions(workspace_id, name);

CREATE TABLE recipe_runs (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL UNIQUE,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    recipe_id TEXT NOT NULL,
    recipe_version INTEGER NOT NULL,
    recipe_snapshot_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'completed', 'incomplete')),
    artifacts_json TEXT NOT NULL,
    checks_json TEXT NOT NULL,
    unresolved_issues_json TEXT NOT NULL,
    missing_evidence_json TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    UNIQUE(session_id, id)
);
CREATE INDEX recipe_runs_session_started ON recipe_runs(session_id, started_at DESC);

CREATE TABLE research_evaluations (
    id TEXT PRIMARY KEY,
    fixture_id TEXT NOT NULL,
    fixture_version INTEGER NOT NULL,
    variant TEXT NOT NULL CHECK (variant IN ('recipe', 'plain_workspace', 'ordinary_codex')),
    model TEXT,
    settings_json TEXT NOT NULL,
    outcome_json TEXT NOT NULL,
    latency_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT NOT NULL,
    UNIQUE(fixture_id, fixture_version, variant, model, settings_json)
);

CREATE TABLE performance_samples (
    id TEXT PRIMARY KEY,
    metric TEXT NOT NULL,
    unit TEXT NOT NULL CHECK (unit IN ('ms', 'percent', 'mib')),
    budget_value REAL NOT NULL CHECK (budget_value > 0),
    observed_value REAL NOT NULL CHECK (observed_value >= 0),
    passed INTEGER NOT NULL CHECK (passed IN (0, 1)),
    details_json TEXT NOT NULL,
    measured_at TEXT NOT NULL
);

CREATE TABLE review_handoffs (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL CHECK (version = 1),
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL REFERENCES paper_revisions(id) ON DELETE RESTRICT,
    content_hash TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    staged_path TEXT NOT NULL,
    input_interpretation TEXT NOT NULL CHECK (input_interpretation IN ('document', 'source_tree')),
    metadata_json TEXT NOT NULL,
    external_reference TEXT,
    created_at TEXT NOT NULL,
    linked_at TEXT
);
CREATE INDEX review_handoffs_workspace_created ON review_handoffs(workspace_id, created_at DESC);

CREATE TABLE retained_blobs (
    storage_reference TEXT NOT NULL,
    reference_type TEXT NOT NULL,
    reference_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    PRIMARY KEY(storage_reference, reference_type, reference_id)
);
