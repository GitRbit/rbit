-- Which repository a fork was created from. Deleting the fork removes the row;
-- deleting the parent keeps the fork, which then has no parent.
CREATE TABLE repo_forks (
    repo_id     INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    parent_id   INTEGER REFERENCES repositories (id) ON DELETE SET NULL,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX repo_forks_by_parent ON repo_forks (parent_id);
