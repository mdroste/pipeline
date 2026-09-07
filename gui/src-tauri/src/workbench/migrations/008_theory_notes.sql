-- Lightweight theory records, typed check receipts and research-direction notes
-- extend the app-owned project record kinds. Applied migrations remain immutable.
CREATE TABLE project_records_v8 (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('home','inventory','anchor','task','checkpoint','application','build','response','experiment','specification','series','binding','bibliography','literature','theory','check','direction')),
 revision INTEGER NOT NULL CHECK(revision > 0), body_json TEXT NOT NULL, updated_at TEXT NOT NULL
);
INSERT INTO project_records_v8 SELECT * FROM project_records;
DROP TABLE project_records;
ALTER TABLE project_records_v8 RENAME TO project_records;
CREATE INDEX project_records_scope ON project_records(workspace_id,kind,updated_at DESC);
-- Dropping the table also dropped its history trigger; recreate it unchanged.
CREATE TRIGGER project_history BEFORE UPDATE ON project_records BEGIN
 INSERT OR IGNORE INTO project_record_history VALUES(OLD.id,OLD.revision,OLD.workspace_id,OLD.kind,OLD.body_json,OLD.updated_at);
END;
