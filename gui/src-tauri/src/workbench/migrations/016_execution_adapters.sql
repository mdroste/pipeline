-- Execution profiles accept the structured research adapters (R, Python,
-- Julia, Jupyter notebooks and Quarto) alongside latex, stata and command.
-- SQLite cannot alter a CHECK constraint, so the table is rebuilt with the
-- same columns, rows and index. Foreign keys are suspended by the migration
-- runner, and references from other tables keep naming execution_profiles.
CREATE TABLE execution_profiles_v16 (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    adapter TEXT NOT NULL CHECK (adapter IN ('latex', 'stata', 'command', 'r', 'python', 'julia', 'notebook', 'quarto')),
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
INSERT INTO execution_profiles_v16 (
    id, workspace_id, name, adapter, argv_json, cwd, environment_json, inputs_json,
    timeout_seconds, outputs_json, tested_at, test_status, revision, created_at, updated_at
)
SELECT
    id, workspace_id, name, adapter, argv_json, cwd, environment_json, inputs_json,
    timeout_seconds, outputs_json, tested_at, test_status, revision, created_at, updated_at
FROM execution_profiles;
DROP TABLE execution_profiles;
ALTER TABLE execution_profiles_v16 RENAME TO execution_profiles;
CREATE INDEX execution_profiles_workspace ON execution_profiles(workspace_id, name);
