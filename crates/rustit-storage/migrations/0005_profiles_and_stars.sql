-- Optional public profile fields beyond `accounts.display_name` / `accounts.bio`.
-- A missing row means "nothing set".
CREATE TABLE account_profiles (
    account_id  INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    website     TEXT,
    location    TEXT
) STRICT;

-- Who starred which repository. Deleting either side removes the star.
CREATE TABLE stars (
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, repo_id)
) STRICT, WITHOUT ROWID;

-- Covers the per-page star summary (count + "starred by viewer", since the primary key
-- columns are part of every index of a WITHOUT ROWID table) and the stargazers list.
CREATE INDEX stars_by_repo ON stars (repo_id, created_at DESC);
