-- Credentials are now stored sealed (AES-256-GCM, `enc:v1:` + base64; see
-- `rbit-storage/src/sealing.rs`), which makes them longer than the plaintext. A push
-- mirror secret may be up to 1024 bytes, so its column must hold the sealed form of that
-- (about 1.4 KB). SQLite cannot alter a CHECK constraint, so the table is rebuilt: every
-- row is copied with its id, then the index is recreated. Nothing references this table.
--
-- Existing plaintext values are sealed by the application at start-up, not here: the key
-- lives outside the database.
CREATE TABLE repo_push_mirrors_v2 (
    id               INTEGER PRIMARY KEY,
    repo_id          INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    url              TEXT    NOT NULL CHECK (length(url) BETWEEN 1 AND 2048),
    username         TEXT    NOT NULL DEFAULT '' CHECK (length(username) <= 255),
    secret           TEXT    CHECK (secret IS NULL OR length(secret) <= 2048),
    enabled          INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    sync_on_push     INTEGER NOT NULL DEFAULT 1 CHECK (sync_on_push IN (0, 1)),
    next_sync_at     INTEGER NOT NULL,
    last_attempt_at  INTEGER,
    last_success_at  INTEGER,
    -- NULL when the last attempt succeeded (or none ran yet).
    last_error       TEXT,
    created_by       INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE (repo_id, url)
) STRICT;

INSERT INTO repo_push_mirrors_v2 (id, repo_id, url, username, secret, enabled, sync_on_push, next_sync_at, last_attempt_at,
                                  last_success_at, last_error, created_by, created_at, updated_at)
SELECT id, repo_id, url, username, secret, enabled, sync_on_push, next_sync_at, last_attempt_at,
       last_success_at, last_error, created_by, created_at, updated_at
FROM repo_push_mirrors;

DROP TABLE repo_push_mirrors;
ALTER TABLE repo_push_mirrors_v2 RENAME TO repo_push_mirrors;

-- The scheduler's "what is due" query.
CREATE INDEX repo_push_mirrors_due ON repo_push_mirrors (next_sync_at) WHERE enabled = 1;
