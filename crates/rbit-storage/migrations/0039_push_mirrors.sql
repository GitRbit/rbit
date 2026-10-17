-- Milestone 3, workstream M: push mirrors, remotes a repository force-pushes its branches
-- and tags to (deleting what no longer exists here), after pushes and on an interval.
--
-- `secret` (a password or access token for HTTP Basic authentication) is stored as given,
-- like webhook secrets: Git must send the raw value. It is write-only: never shown,
-- returned or logged.
CREATE TABLE repo_push_mirrors (
    id               INTEGER PRIMARY KEY,
    repo_id          INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    url              TEXT    NOT NULL CHECK (length(url) BETWEEN 1 AND 2048),
    username         TEXT    NOT NULL DEFAULT '' CHECK (length(username) <= 255),
    secret           TEXT    CHECK (secret IS NULL OR length(secret) <= 1024),
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

-- The scheduler's "what is due" query.
CREATE INDEX repo_push_mirrors_due ON repo_push_mirrors (next_sync_at) WHERE enabled = 1;
