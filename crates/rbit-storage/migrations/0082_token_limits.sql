-- Agent loop: scoped tokens (agent plan step 4.8). A personal access token may be limited to
-- one repository and to some rights within its scope. The limit lives beside the token, not
-- in it, so the credential table and its writer protocol stay as they are; it goes with the
-- token (and with the repository).
CREATE TABLE token_limits (
    token_id INTEGER PRIMARY KEY REFERENCES access_tokens (id) ON DELETE CASCADE,
    repo_id  INTEGER REFERENCES repositories (id) ON DELETE CASCADE,
    -- Comma-separated: push, statuses, merge, dispatch, approve, read_logs. Empty: none.
    rights   TEXT    NOT NULL CHECK (length(rights) <= 200)
) STRICT;
