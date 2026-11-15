-- Inventory, checkpoints and application records are excluded from search.
-- Home records still invalidate because they carry note exclusions.
DROP TRIGGER research_dirty_project_records_insert;
CREATE TRIGGER research_dirty_project_records_insert AFTER INSERT ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT NEW.workspace_id WHERE NEW.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
DROP TRIGGER research_dirty_project_records_update;
CREATE TRIGGER research_dirty_project_records_update AFTER UPDATE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT OLD.workspace_id WHERE OLD.kind NOT IN ('inventory','checkpoint','application')
 UNION
 SELECT NEW.workspace_id WHERE NEW.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
DROP TRIGGER research_dirty_project_records_delete;
CREATE TRIGGER research_dirty_project_records_delete AFTER DELETE ON project_records BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT OLD.workspace_id WHERE OLD.kind NOT IN ('inventory','checkpoint','application')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
DROP TRIGGER research_dirty_transcript_items_insert;
CREATE TRIGGER research_dirty_transcript_items_insert AFTER INSERT ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id AND s.workspace_id IS NOT NULL AND NEW.is_final=1 AND NEW.item_kind IN ('userMessage','agentMessage','user_message','agent_message')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
DROP TRIGGER research_dirty_transcript_items_update;
CREATE TRIGGER research_dirty_transcript_items_update AFTER UPDATE ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=OLD.binding_id AND s.workspace_id IS NOT NULL AND OLD.is_final=1 AND OLD.item_kind IN ('userMessage','agentMessage','user_message','agent_message')
 UNION
 SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=NEW.binding_id AND s.workspace_id IS NOT NULL AND NEW.is_final=1 AND NEW.item_kind IN ('userMessage','agentMessage','user_message','agent_message')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
DROP TRIGGER research_dirty_transcript_items_delete;
CREATE TRIGGER research_dirty_transcript_items_delete AFTER DELETE ON transcript_items BEGIN
 INSERT INTO research_index_state(workspace_id)
 SELECT s.workspace_id FROM sessions s JOIN session_bindings b ON b.session_id=s.id WHERE b.id=OLD.binding_id AND s.workspace_id IS NOT NULL AND OLD.is_final=1 AND OLD.item_kind IN ('userMessage','agentMessage','user_message','agent_message')
 ON CONFLICT(workspace_id) DO UPDATE SET generation=generation+1,cursor='',byte_offset=0,complete=0;
END;
