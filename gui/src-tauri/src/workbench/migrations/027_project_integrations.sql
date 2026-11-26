-- Extend kind constraints for project integrations and immutable context captures.
-- Rebuild tables atomically, preserving history, foreign references and search.
DROP VIEW research_search_sources;
CREATE TABLE project_records_v27 (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('home','inventory','anchor','task','checkpoint','application','build','response','experiment','specification','series','binding','bibliography','literature','theory','check','direction','check_policy','check_state','connection','github_comment')),
 revision INTEGER NOT NULL CHECK(revision > 0), body_json TEXT NOT NULL, updated_at TEXT NOT NULL
);
INSERT INTO project_records_v27 SELECT * FROM project_records;
DROP TABLE project_records;
ALTER TABLE project_records_v27 RENAME TO project_records;
CREATE INDEX project_records_scope ON project_records(workspace_id,kind,updated_at DESC);
-- Dropping the table also dropped its history trigger; recreate it unchanged.
CREATE TRIGGER project_history BEFORE UPDATE ON project_records BEGIN
 INSERT OR IGNORE INTO project_record_history VALUES(OLD.id,OLD.revision,OLD.workspace_id,OLD.kind,OLD.body_json,OLD.updated_at);
END;

CREATE TABLE desk_records_v27 (
 id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('collection','decision','relation','dataset','sample','acquisition','execution_plan','handoff','experiment_plan','publication_asset','asset_inclusion','figure_recipe','symbol','assumption_branch','revision_campaign','coauthor_review','deliverable','kit_install','deliverable_role','monitor','capsule_import','github_collaboration','submission','source_selection','response_context','check_receipt')),
 title TEXT NOT NULL, body_json TEXT NOT NULL, content_hash TEXT NOT NULL,
 supersedes TEXT REFERENCES desk_records(id), created_at TEXT NOT NULL
);
INSERT INTO desk_records_v27 SELECT * FROM desk_records;
DROP TABLE desk_records;
ALTER TABLE desk_records_v27 RENAME TO desk_records;
CREATE INDEX desk_records_scope ON desk_records(workspace_id,kind,created_at);
CREATE VIEW research_search_sources AS
 SELECT p.workspace_id,'paper:'||pr.id object_key,'paper' kind,pr.id id,pr.content_hash revision,p.title title,'' body,pr.text_reference text_reference,'captured document' provenance,CASE WHEN pr.text_reference IS NULL THEN 'unavailable' ELSE 'full' END access,CASE WHEN json_extract(pr.extraction_json,'$.status')='complete' THEN 'complete' ELSE 'partial' END completeness FROM papers p JOIN paper_revisions pr ON pr.paper_id=p.id
 UNION ALL SELECT s.workspace_id,'source:'||v.id,'source',v.id,COALESCE(v.content_hash,v.created_at),CASE WHEN s.citation_key IS NOT NULL THEN s.title||' ['||s.citation_key||']' ELSE s.title END,s.identifiers_json,v.text_reference,v.acquired_via,v.access_state,CASE WHEN v.access_state='full' THEN 'complete' ELSE 'metadata_or_partial' END FROM sources s JOIN source_versions v ON v.source_id=s.id
 UNION ALL SELECT n.workspace_id,'note:'||n.id||':'||n.revision,'note',n.id,n.revision,n.title,n.body,NULL,n.provenance,'local','complete' FROM desk_note_versions n
 UNION ALL SELECT workspace_id,'record:'||id||':'||revision,'record',id,CAST(revision AS TEXT),kind,body_json,NULL,'project record','local','complete' FROM project_records WHERE kind NOT IN ('inventory','home','checkpoint','application')
 UNION ALL SELECT workspace_id,'record:'||object_id||':'||revision,'record',object_id,CAST(revision AS TEXT),kind,body_json,NULL,'historical project record','local','complete' FROM project_record_history WHERE kind NOT IN ('inventory','home','checkpoint','application')
 UNION ALL SELECT s.workspace_id,'message:'||i.id,'message',i.id,i.updated_at,s.title,i.payload_json,NULL,'conversation / unaccepted','local','complete' FROM transcript_items i JOIN session_bindings b ON b.id=i.binding_id JOIN sessions s ON s.id=b.session_id WHERE s.workspace_id IS NOT NULL AND i.is_final=1 AND i.item_kind IN ('userMessage','agentMessage','user_message','agent_message')
 UNION ALL SELECT workspace_id,'execution:'||id,'execution',id,created_at,adapter||' / '||outcome,output_manifest_json,NULL,'execution receipt','local','declared_only' FROM research_executions
 UNION ALL SELECT e.workspace_id,'result:'||sr.id,'result',sr.id,sr.created_at,sr.result_id,sr.body_json,NULL,'structured result','local','declared_only' FROM structured_results sr JOIN research_executions e ON e.id=sr.execution_id
 UNION ALL SELECT c.workspace_id,'claim:'||v.id,'claim',v.id,CAST(v.version AS TEXT),v.claim_text,v.claim_text,NULL,c.workflow_state||' / '||v.origin,'local','complete' FROM claims c JOIN claim_versions v ON v.claim_id=c.id
 UNION ALL SELECT workspace_id,'artifact:'||id,'artifact',id,content_hash,media_kind||' artifact',json_object('artifactId',id,'mediaKind',media_kind,'contentHash',content_hash,'sizeBytes',size_bytes),NULL,origin,'local','metadata_only' FROM artifacts WHERE origin NOT IN ('dataset','execution_plan')
 UNION ALL SELECT workspace_id,'desk:'||id,kind,id,content_hash,title,CASE WHEN kind='dataset' THEN json_remove(body_json,'$.rows','$.previewRows') ELSE body_json END,NULL,'researcher declaration','local','complete' FROM desk_records WHERE kind NOT IN ('execution_plan','acquisition','relation','experiment_plan','figure_recipe','capsule_import','monitor');
CREATE TRIGGER research_dirty_desk_records_insert AFTER INSERT ON desk_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_desk_records_update AFTER UPDATE ON desk_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_desk_records_delete AFTER DELETE ON desk_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;


CREATE TRIGGER research_dirty_project_records_insert AFTER INSERT ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT NEW.workspace_id WHERE NEW.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
CREATE TRIGGER research_dirty_project_records_update AFTER UPDATE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT OLD.workspace_id WHERE OLD.kind NOT IN ('inventory','checkpoint','application')
 UNION
 SELECT NEW.workspace_id WHERE NEW.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
CREATE TRIGGER research_dirty_project_records_delete AFTER DELETE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT OLD.workspace_id WHERE OLD.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
