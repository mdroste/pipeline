-- Preserve document and revision identities while adding user-facing types.
ALTER TABLE papers ADD COLUMN document_type TEXT NOT NULL DEFAULT 'other'
    CHECK (document_type IN ('paper', 'slides', 'appendix', 'response', 'other'));
ALTER TABLE papers ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1));
ALTER TABLE papers ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));
UPDATE papers SET document_type = CASE role
    WHEN 'manuscript' THEN 'paper' WHEN 'appendix' THEN 'appendix' ELSE 'other' END;
