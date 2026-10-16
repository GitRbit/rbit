-- When the user last proved they control `email` (NULL: never, or changed since).
ALTER TABLE user_logins ADD COLUMN email_verified_at INTEGER;

-- One-time links sent by email. Only the SHA-256 digest of the secret is stored, so a
-- copy of the database cannot be used to reset anyone's password. A token is bound to
-- the address it was sent to and stops working when the account's address changes.
CREATE TABLE email_tokens (
    digest      BLOB    PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    purpose     TEXT    NOT NULL CHECK (purpose IN ('reset', 'verify')),
    email       TEXT    NOT NULL COLLATE NOCASE,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    used_at     INTEGER
) STRICT, WITHOUT ROWID;

CREATE INDEX email_tokens_by_account ON email_tokens (account_id, purpose);
CREATE INDEX email_tokens_by_expiry ON email_tokens (expires_at);
