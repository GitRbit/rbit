-- Allow the `admin` scope for personal access tokens (repository settings, webhooks and
-- deletion through the REST API). SQLite cannot alter a CHECK constraint, so the table is
-- rebuilt: every row is copied with its id, then the index is recreated.
CREATE TABLE access_tokens_v2 (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    token_digest  BLOB    NOT NULL UNIQUE,
    hint          TEXT    NOT NULL,
    scope         TEXT    NOT NULL CHECK (scope IN ('read', 'write', 'admin')),
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER,
    expires_at    INTEGER
) STRICT;

INSERT INTO access_tokens_v2 (id, account_id, name, token_digest, hint, scope, created_at, last_used_at, expires_at)
SELECT id, account_id, name, token_digest, hint, scope, created_at, last_used_at, expires_at FROM access_tokens;

DROP TABLE access_tokens;
ALTER TABLE access_tokens_v2 RENAME TO access_tokens;

CREATE INDEX access_tokens_by_account ON access_tokens (account_id, created_at DESC);
