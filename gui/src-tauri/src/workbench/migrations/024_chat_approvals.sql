CREATE TABLE chat_approvals (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    request_key TEXT NOT NULL,
    PRIMARY KEY (session_id, scope, request_key)
);
