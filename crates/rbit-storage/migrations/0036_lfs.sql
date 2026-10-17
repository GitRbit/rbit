-- Git LFS objects: the bytes live on disk at `<data_dir>/lfs/<aa>/<bb>/<oid>`, stored once
-- however many repositories use them. A row exists only for an object whose bytes are in
-- place. `unlinked_at` is when the last repository stopped using it, so maintenance
-- removes orphans only after a grace period.
CREATE TABLE lfs_objects (
    oid          TEXT    PRIMARY KEY CHECK (length(oid) = 64),
    size         INTEGER NOT NULL CHECK (size >= 0),
    created_at   INTEGER NOT NULL,
    unlinked_at  INTEGER
) STRICT;

-- Which repositories may download an object. Deleting a repository unlinks its objects.
CREATE TABLE lfs_repo_objects (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    oid         TEXT    NOT NULL REFERENCES lfs_objects (oid) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (repo_id, oid)
) STRICT, WITHOUT ROWID;

CREATE INDEX lfs_repo_objects_by_oid ON lfs_repo_objects (oid);

CREATE TRIGGER lfs_repo_objects_unlinked AFTER DELETE ON lfs_repo_objects
BEGIN
    UPDATE lfs_objects SET unlinked_at = unixepoch() WHERE oid = OLD.oid;
END;

-- Paths locked for editing (the LFS locks API), one lock per path and repository.
CREATE TABLE lfs_locks (
    id          INTEGER PRIMARY KEY,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    owner_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    UNIQUE (repo_id, path)
) STRICT;
