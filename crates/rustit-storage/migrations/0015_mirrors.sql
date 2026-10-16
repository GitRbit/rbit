-- Pull mirrors: repositories fetched from an upstream URL on a schedule.
-- Removing the row turns the mirror into an ordinary repository.
CREATE TABLE repo_mirrors (
    repo_id           INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    url               TEXT    NOT NULL,
    interval_minutes  INTEGER NOT NULL CHECK (interval_minutes > 0),
    next_sync_at      INTEGER NOT NULL,
    last_sync_at      INTEGER,
    last_error        TEXT,
    created_at        INTEGER NOT NULL
) STRICT;

-- The scheduler's "what is due" query.
CREATE INDEX repo_mirrors_due ON repo_mirrors (next_sync_at);
