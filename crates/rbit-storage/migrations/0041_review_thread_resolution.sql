-- Resolution belongs to the root of a submitted line-comment conversation.
ALTER TABLE pull_review_comments ADD COLUMN resolved_at INTEGER
    CHECK (resolved_at IS NULL OR in_reply_to IS NULL);
ALTER TABLE pull_review_comments ADD COLUMN resolved_by INTEGER REFERENCES accounts (id) ON DELETE SET NULL
    CHECK (resolved_by IS NULL OR resolved_at IS NOT NULL);
