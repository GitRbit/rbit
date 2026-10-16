-- Personal access tokens. Only the SHA-256 digest of the secret is stored.
CREATE TABLE access_tokens (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    token_digest  BLOB    NOT NULL UNIQUE,
    hint          TEXT    NOT NULL,
    scope         TEXT    NOT NULL CHECK (scope IN ('read', 'write')),
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER,
    expires_at    INTEGER
) STRICT;

CREATE INDEX access_tokens_by_account ON access_tokens (account_id, created_at DESC);
