ALTER TABLE session_bindings ADD COLUMN harness_fingerprint TEXT;

CREATE TABLE workspace_configs (
    scope_key TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    updated_at TEXT NOT NULL,
    CHECK ((scope_key = 'global' AND workspace_id IS NULL) OR scope_key = workspace_id)
);

CREATE TABLE presets (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT NOT NULL,
    modules_json TEXT NOT NULL,
    source_preset_id TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    archived_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX presets_workspace_name ON presets(workspace_id, name);

CREATE TABLE papers (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('manuscript', 'appendix', 'other')),
    current_revision_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX papers_workspace_updated ON papers(workspace_id, updated_at DESC);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    content_hash TEXT NOT NULL,
    media_kind TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    origin TEXT NOT NULL,
    storage_reference TEXT NOT NULL,
    original_path TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(workspace_id, content_hash, media_kind)
);

CREATE TABLE paper_revisions (
    id TEXT PRIMARY KEY,
    paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    input_kind TEXT NOT NULL CHECK (input_kind IN ('pdf', 'tex', 'docx', 'source_tree', 'text')),
    entrypoint TEXT NOT NULL,
    dependency_manifest_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    text_reference TEXT,
    compiled_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    extraction_json TEXT NOT NULL,
    capture_complete INTEGER NOT NULL CHECK (capture_complete IN (0, 1)),
    captured_at TEXT NOT NULL,
    UNIQUE(paper_id, content_hash)
);
CREATE INDEX paper_revisions_paper_captured ON paper_revisions(paper_id, captured_at DESC);

CREATE TABLE research_notes (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    paper_id TEXT REFERENCES papers(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('question', 'assumption', 'decision', 'next_step', 'notation', 'handoff')),
    body TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('proposed', 'accepted', 'rejected', 'retired')),
    origin TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX research_notes_context ON research_notes(workspace_id, state, pinned, updated_at DESC);

CREATE TABLE session_context_items (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    object_type TEXT NOT NULL,
    object_id TEXT NOT NULL,
    added_at TEXT NOT NULL,
    PRIMARY KEY (session_id, object_type, object_id)
);

CREATE TABLE sources (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    citation_key TEXT,
    identifiers_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX sources_workspace_title ON sources(workspace_id, title);

CREATE TABLE source_versions (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    version_label TEXT,
    locator TEXT,
    access_state TEXT NOT NULL CHECK (access_state IN ('metadata', 'abstract', 'partial', 'full', 'unavailable')),
    acquired_via TEXT NOT NULL,
    accessed_at TEXT,
    content_hash TEXT,
    text_reference TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX source_versions_source_created ON source_versions(source_id, created_at DESC);

CREATE TABLE execution_profiles (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    adapter TEXT NOT NULL CHECK (adapter IN ('latex', 'stata', 'command')),
    argv_json TEXT NOT NULL,
    cwd TEXT NOT NULL,
    environment_json TEXT NOT NULL,
    inputs_json TEXT NOT NULL,
    timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds BETWEEN 1 AND 7200),
    outputs_json TEXT NOT NULL,
    tested_at TEXT,
    test_status TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX execution_profiles_workspace ON execution_profiles(workspace_id, name);

CREATE TABLE research_executions (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    binding_id TEXT REFERENCES session_bindings(id) ON DELETE SET NULL,
    provider_turn_id TEXT,
    tool_call_id TEXT,
    profile_id TEXT REFERENCES execution_profiles(id) ON DELETE SET NULL,
    adapter TEXT NOT NULL,
    command_json TEXT NOT NULL,
    cwd TEXT NOT NULL,
    environment_identity TEXT,
    input_manifest_json TEXT NOT NULL,
    dependency_hash TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('queued', 'running', 'completed', 'failed', 'interrupted', 'timed_out', 'outcome_unknown')),
    started_at TEXT,
    ended_at TEXT,
    exit_status INTEGER,
    stdout_text TEXT,
    stderr_text TEXT,
    output_manifest_json TEXT NOT NULL,
    validation_json TEXT NOT NULL,
    snapshot_consistency TEXT NOT NULL CHECK (snapshot_consistency IN ('complete', 'partial', 'uncertain')),
    created_at TEXT NOT NULL
);
CREATE INDEX research_executions_workspace_created ON research_executions(workspace_id, created_at DESC);

CREATE TABLE tool_receipts (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    binding_id TEXT REFERENCES session_bindings(id) ON DELETE SET NULL,
    provider_turn_id TEXT NOT NULL,
    call_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    catalog_version INTEGER NOT NULL,
    arguments_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('running', 'completed', 'failed', 'outcome_unknown')),
    result_json TEXT,
    error_json TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    UNIQUE(binding_id, provider_turn_id, call_id)
);

CREATE TABLE claims (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    paper_id TEXT REFERENCES papers(id) ON DELETE CASCADE,
    current_version_id TEXT,
    workflow_state TEXT NOT NULL CHECK (workflow_state IN ('proposed', 'under_review', 'accepted', 'retired')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX claims_workspace_state ON claims(workspace_id, workflow_state, updated_at DESC);

CREATE TABLE claim_versions (
    id TEXT PRIMARY KEY,
    claim_id TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE,
    version INTEGER NOT NULL CHECK (version >= 1),
    claim_text TEXT NOT NULL,
    kind TEXT NOT NULL,
    origin TEXT NOT NULL,
    paper_locator_json TEXT,
    dependency_hash TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(claim_id, version)
);

CREATE TABLE evidence_links (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    claim_version_id TEXT NOT NULL REFERENCES claim_versions(id) ON DELETE CASCADE,
    target_type TEXT NOT NULL CHECK (target_type IN ('paper_revision', 'source_version', 'execution', 'artifact', 'derivation')),
    target_id TEXT NOT NULL,
    locator_json TEXT,
    relation TEXT NOT NULL CHECK (relation IN ('supports', 'contradicts', 'qualifies')),
    assessment TEXT NOT NULL CHECK (assessment IN ('not_checked', 'model_assessed', 'human_confirmed', 'check_passed', 'check_failed')),
    assessor TEXT NOT NULL,
    dependency_hash TEXT,
    freshness TEXT NOT NULL CHECK (freshness IN ('current', 'stale', 'unknown')),
    stale_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX evidence_claim_freshness ON evidence_links(claim_version_id, freshness);

CREATE TABLE verification_records (
    id TEXT PRIMARY KEY,
    evidence_link_id TEXT NOT NULL REFERENCES evidence_links(id) ON DELETE CASCADE,
    method TEXT NOT NULL,
    checker_identity TEXT NOT NULL,
    input_hashes_json TEXT NOT NULL,
    observed_result_json TEXT NOT NULL,
    limitations TEXT NOT NULL,
    passed INTEGER NOT NULL CHECK (passed IN (0, 1)),
    created_at TEXT NOT NULL
);

CREATE TABLE structured_results (
    id TEXT PRIMARY KEY,
    execution_id TEXT NOT NULL REFERENCES research_executions(id) ON DELETE CASCADE,
    result_id TEXT NOT NULL,
    body_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(execution_id, result_id)
);
