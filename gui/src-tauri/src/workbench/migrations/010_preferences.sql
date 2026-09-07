-- App-owned Workspace preferences that are not harness configuration and
-- therefore must not enter effective-harness fingerprints.
CREATE TABLE preferences (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
