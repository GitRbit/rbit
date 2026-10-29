-- Actions: disabled workflows. GitHub's "Disable workflow": a disabled workflow file starts
-- no run for any event until it is enabled again. Kept by path, so it survives edits of the
-- file and applies on every branch.
CREATE TABLE actions_disabled_workflows (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    path        TEXT    NOT NULL CHECK (length(path) BETWEEN 1 AND 1024),
    disabled_at INTEGER NOT NULL,
    disabled_by INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    PRIMARY KEY (repo_id, path)
) STRICT;
