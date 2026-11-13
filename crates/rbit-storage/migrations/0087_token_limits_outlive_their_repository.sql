-- Scoped tokens (agent plan step 4.8): a token limited to a repository that is deleted must
-- reach no repository. Its limit went with the repository (`ON DELETE CASCADE`), which left
-- the token unlimited. The limit now stays, with the repository forgotten and `scoped`
-- saying it had one. A CHECK and a foreign key action cannot change in place: the table is
-- rebuilt with every row.
CREATE TABLE token_limits_next (
    token_id INTEGER PRIMARY KEY REFERENCES access_tokens (id) ON DELETE CASCADE,
    repo_id  INTEGER REFERENCES repositories (id) ON DELETE SET NULL,
    -- Whether the token is limited to one repository (`repo_id`, or none once it is gone).
    scoped   INTEGER NOT NULL CHECK (scoped IN (0, 1)),
    -- Comma-separated: push, statuses, merge, dispatch, approve, read_logs. Empty: none.
    rights   TEXT    NOT NULL CHECK (length(rights) <= 200),
    CHECK (scoped = 1 OR repo_id IS NULL)
) STRICT;

INSERT INTO token_limits_next (token_id, repo_id, scoped, rights)
    SELECT token_id, repo_id, repo_id IS NOT NULL, rights FROM token_limits;

DROP TABLE token_limits;

ALTER TABLE token_limits_next RENAME TO token_limits;
