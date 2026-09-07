-- Research work surfaces extend app-owned records; applied migrations remain immutable.
CREATE TABLE project_records_v7 (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('home','inventory','anchor','task','checkpoint','application','build','response','experiment','specification','series','binding','bibliography','literature')),
 revision INTEGER NOT NULL CHECK(revision > 0), body_json TEXT NOT NULL, updated_at TEXT NOT NULL
);
INSERT INTO project_records_v7 SELECT * FROM project_records;
DROP TABLE project_records;
ALTER TABLE project_records_v7 RENAME TO project_records;
CREATE INDEX project_records_scope ON project_records(workspace_id,kind,updated_at DESC);
CREATE TABLE project_record_history (
 object_id TEXT NOT NULL, revision INTEGER NOT NULL, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, body_json TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(object_id,revision)
);
CREATE TRIGGER project_history BEFORE UPDATE ON project_records BEGIN
 INSERT OR IGNORE INTO project_record_history VALUES(OLD.id,OLD.revision,OLD.workspace_id,OLD.kind,OLD.body_json,OLD.updated_at);
END;
CREATE TABLE execution_jobs (
 execution_id TEXT PRIMARY KEY REFERENCES research_executions(id) ON DELETE CASCADE,
 operation_id TEXT NOT NULL UNIQUE, request_hash TEXT NOT NULL,
 owner TEXT NOT NULL CHECK(owner IN ('turn','detached')),
 process_instance TEXT NOT NULL,
 finalization_json TEXT,
 cancel_requested INTEGER NOT NULL DEFAULT 0,
 created_at TEXT NOT NULL
);
