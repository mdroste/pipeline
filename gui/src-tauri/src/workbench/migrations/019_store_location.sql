-- The data folder this store's file references were last confirmed against.
-- Empty until an open confirms one, which is also how a store moved before this
-- table existed looks. The open path owns this row; archives never carry it.
CREATE TABLE IF NOT EXISTS store_location (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    root TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);
