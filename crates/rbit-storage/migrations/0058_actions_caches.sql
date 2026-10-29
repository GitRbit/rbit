-- Actions: shared cache. `actions/cache` entries the server keeps for every runner of a
-- repository, as GitHub's cache service does. The files live under
-- <data_dir>/actions/caches and are not backed up.
CREATE TABLE actions_caches (
    id          INTEGER PRIMARY KEY,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    -- The scope it was saved in (`heads/main`, `untrusted/41`), decided by the server.
    scope       TEXT    NOT NULL CHECK (length(scope) BETWEEN 1 AND 512),
    key         TEXT    NOT NULL CHECK (length(key) BETWEEN 1 AND 512),
    -- What else must match for a restore (paths and platform), as the runner names it.
    version     TEXT    NOT NULL CHECK (length(version) = 64 AND version NOT GLOB '*[^0-9a-f]*'),
    size        INTEGER NOT NULL CHECK (size >= 0),
    sha256      BLOB    NOT NULL CHECK (length(sha256) = 32),
    created_at  INTEGER NOT NULL,
    used_at     INTEGER NOT NULL,
    -- Entries are immutable: a second save of the same key is refused.
    UNIQUE (repo_id, scope, key, version)
) STRICT;

CREATE INDEX actions_caches_by_use ON actions_caches (repo_id, used_at);
CREATE INDEX actions_caches_unused ON actions_caches (used_at);
