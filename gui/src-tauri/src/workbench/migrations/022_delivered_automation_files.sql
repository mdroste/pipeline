-- The exchange owns its verified files even after the producer's history expires.
ALTER TABLE task_exchanges ADD COLUMN files_adopted INTEGER NOT NULL DEFAULT 0;
CREATE INDEX task_exchange_pending_files ON task_exchanges(id)
    WHERE kind='result' AND files_adopted=0;
CREATE TABLE task_delivery_files (
    exchange_id TEXT NOT NULL REFERENCES task_exchanges(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    filename TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_reference TEXT NOT NULL,
    PRIMARY KEY(exchange_id, ordinal)
);
