-- Research desk state is app-owned; it never shares Workflow runtime state.
CREATE TABLE desk_records (
 id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('collection','decision','relation','dataset','sample','acquisition','execution_plan','handoff')),
 title TEXT NOT NULL, body_json TEXT NOT NULL, content_hash TEXT NOT NULL,
 supersedes TEXT REFERENCES desk_records(id), created_at TEXT NOT NULL
);
CREATE INDEX desk_records_scope ON desk_records(workspace_id,kind,created_at);
CREATE TABLE desk_context (
 session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL, items_json TEXT NOT NULL
);
CREATE TABLE data_policies (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL, body_json TEXT NOT NULL
);
CREATE TABLE acquisition_settings (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
 enabled INTEGER NOT NULL DEFAULT 0
);
-- Retain exact note revisions, including rejected rationale. Existing history remains intact.
CREATE TABLE desk_note_versions (
 id TEXT NOT NULL, revision TEXT NOT NULL, workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
 title TEXT NOT NULL, body TEXT NOT NULL, provenance TEXT NOT NULL, PRIMARY KEY(id,revision)
);
INSERT INTO desk_note_versions SELECT id,CAST(revision AS TEXT),workspace_id,kind,body,state||' / '||origin FROM research_notes;
INSERT OR IGNORE INTO desk_note_versions
 SELECT n.id,CAST(json_extract(c.details_json,'$.previous.revision') AS TEXT),n.workspace_id,
 json_extract(c.details_json,'$.previous.kind'),json_extract(c.details_json,'$.previous.body'),
 json_extract(c.details_json,'$.previous.state')||' / '||json_extract(c.details_json,'$.previous.origin')
 FROM change_log c JOIN research_notes n ON n.id=c.entity_id AND n.workspace_id=c.workspace_id
 WHERE c.entity_type='research_note' AND c.action='updated' AND json_valid(c.details_json)
 AND json_type(c.details_json,'$.previous.revision')='integer'
 AND json_type(c.details_json,'$.previous.body')='text';
CREATE TRIGGER desk_note_insert AFTER INSERT ON research_notes BEGIN
 INSERT OR IGNORE INTO desk_note_versions VALUES(NEW.id,CAST(NEW.revision AS TEXT),NEW.workspace_id,NEW.kind,NEW.body,NEW.state||' / '||NEW.origin);
END;
CREATE TRIGGER desk_note_update AFTER UPDATE ON research_notes BEGIN
 INSERT OR IGNORE INTO desk_note_versions VALUES(NEW.id,CAST(NEW.revision AS TEXT),NEW.workspace_id,NEW.kind,NEW.body,NEW.state||' / '||NEW.origin);
END;
CREATE TRIGGER desk_note_delete AFTER DELETE ON research_notes BEGIN
 DELETE FROM desk_note_versions WHERE id=OLD.id;
END;
-- One coalesced rebuild request per workspace. The cursor resumes bounded batches.
CREATE TABLE research_index_state (
 workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
 generation INTEGER NOT NULL DEFAULT 1, cursor TEXT NOT NULL DEFAULT '', byte_offset INTEGER NOT NULL DEFAULT 0,
 complete INTEGER NOT NULL DEFAULT 0
);
CREATE VIRTUAL TABLE research_fts USING fts5(title,body,metadata,workspace_id UNINDEXED,generation UNINDEXED,object_json UNINDEXED,provenance UNINDEXED,access UNINDEXED,completeness UNINDEXED,tokenize='unicode61 remove_diacritics 2');
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
 UNION ALL SELECT workspace_id,'desk:'||id,kind,id,content_hash,title,CASE WHEN kind='dataset' THEN json_remove(body_json,'$.rows','$.previewRows') ELSE body_json END,NULL,'researcher declaration','local','complete' FROM desk_records WHERE kind NOT IN ('execution_plan','acquisition','relation');
INSERT INTO research_index_state(workspace_id) SELECT id FROM workspaces;
CREATE TRIGGER research_dirty_papers_insert AFTER INSERT ON papers BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_papers_update AFTER UPDATE ON papers BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_papers_delete AFTER DELETE ON papers BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_paper_revisions_insert AFTER INSERT ON paper_revisions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM papers WHERE id=NEW.paper_id) WHERE (SELECT workspace_id FROM papers WHERE id=NEW.paper_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_paper_revisions_update AFTER UPDATE ON paper_revisions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM papers WHERE id=NEW.paper_id) WHERE (SELECT workspace_id FROM papers WHERE id=NEW.paper_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_paper_revisions_delete AFTER DELETE ON paper_revisions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM papers WHERE id=OLD.paper_id) WHERE (SELECT workspace_id FROM papers WHERE id=OLD.paper_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_sources_insert AFTER INSERT ON sources BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_sources_update AFTER UPDATE ON sources BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_sources_delete AFTER DELETE ON sources BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_source_versions_insert AFTER INSERT ON source_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM sources WHERE id=NEW.source_id) WHERE (SELECT workspace_id FROM sources WHERE id=NEW.source_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_source_versions_update AFTER UPDATE ON source_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM sources WHERE id=NEW.source_id) WHERE (SELECT workspace_id FROM sources WHERE id=NEW.source_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_source_versions_delete AFTER DELETE ON source_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM sources WHERE id=OLD.source_id) WHERE (SELECT workspace_id FROM sources WHERE id=OLD.source_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_research_notes_insert AFTER INSERT ON research_notes BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_research_notes_update AFTER UPDATE ON research_notes BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_research_notes_delete AFTER DELETE ON research_notes BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_project_records_insert AFTER INSERT ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_project_records_update AFTER UPDATE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_project_records_delete AFTER DELETE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
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
CREATE TRIGGER research_dirty_research_executions_insert AFTER INSERT ON research_executions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_research_executions_update AFTER UPDATE ON research_executions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_research_executions_delete AFTER DELETE ON research_executions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_structured_results_insert AFTER INSERT ON structured_results BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM research_executions WHERE id=NEW.execution_id) WHERE (SELECT workspace_id FROM research_executions WHERE id=NEW.execution_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_structured_results_update AFTER UPDATE ON structured_results BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM research_executions WHERE id=NEW.execution_id) WHERE (SELECT workspace_id FROM research_executions WHERE id=NEW.execution_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_structured_results_delete AFTER DELETE ON structured_results BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM research_executions WHERE id=OLD.execution_id) WHERE (SELECT workspace_id FROM research_executions WHERE id=OLD.execution_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_transcript_items_insert AFTER INSERT ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id) WHERE (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_transcript_items_update AFTER UPDATE ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id) WHERE (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_transcript_items_delete AFTER DELETE ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=OLD.binding_id) WHERE (SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=OLD.binding_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_data_policies_insert AFTER INSERT ON data_policies BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_data_policies_update AFTER UPDATE ON data_policies BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
CREATE TRIGGER research_dirty_data_policies_delete AFTER DELETE ON data_policies BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TABLE execution_plan_state(plan_id TEXT PRIMARY KEY REFERENCES desk_records(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL,authorized INTEGER NOT NULL DEFAULT 0,test_status TEXT);

CREATE TRIGGER desk_session_move AFTER UPDATE OF workspace_id ON sessions WHEN OLD.workspace_id IS NOT NEW.workspace_id BEGIN
 DELETE FROM desk_context WHERE session_id=NEW.id;
 UPDATE research_index_state SET generation=generation+1,cursor='',byte_offset=0,complete=0 WHERE workspace_id IN (OLD.workspace_id,NEW.workspace_id);
END;
CREATE TRIGGER desk_session_delete BEFORE DELETE ON sessions BEGIN
 UPDATE research_index_state SET generation=generation+1,cursor='',byte_offset=0,complete=0 WHERE workspace_id=OLD.workspace_id;
END;

CREATE TRIGGER research_dirty_claims_insert AFTER INSERT ON claims BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_claims_update AFTER UPDATE ON claims BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_claims_delete AFTER DELETE ON claims BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_claim_versions_insert AFTER INSERT ON claim_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM claims WHERE id=NEW.claim_id) WHERE (SELECT workspace_id FROM claims WHERE id=NEW.claim_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_claim_versions_update AFTER UPDATE ON claim_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM claims WHERE id=NEW.claim_id) WHERE (SELECT workspace_id FROM claims WHERE id=NEW.claim_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_claim_versions_delete AFTER DELETE ON claim_versions BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT (SELECT workspace_id FROM claims WHERE id=OLD.claim_id) WHERE (SELECT workspace_id FROM claims WHERE id=OLD.claim_id) IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_artifacts_insert AFTER INSERT ON artifacts BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_artifacts_update AFTER UPDATE ON artifacts BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT NEW.workspace_id WHERE NEW.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;

CREATE TRIGGER research_dirty_artifacts_delete AFTER DELETE ON artifacts BEGIN
 INSERT INTO research_index_state(workspace_id) SELECT OLD.workspace_id WHERE OLD.workspace_id IS NOT NULL
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
 END;
