-- Archived (read-only) repositories: one row while a repository is archived. Unarchiving
-- deletes the row, so `repositories` itself is unchanged and existing rows need nothing.
CREATE TABLE repo_archives (
    repo_id      INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    archived_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    archived_at  INTEGER NOT NULL
) STRICT;
