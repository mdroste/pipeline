ALTER TABLE session_bindings ADD COLUMN instruction_sources_json TEXT NOT NULL DEFAULT '[]';
