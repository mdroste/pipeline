-- Global conversation pages can stop once enough recent summaries match.
CREATE INDEX sessions_index_time ON sessions(updated_at DESC, id);
