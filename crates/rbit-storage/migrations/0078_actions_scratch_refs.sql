-- rbit Actions: scratch refs (`refs/scratch/<owner>/<name>`), commits on no branch pushed so
-- CI tests them. Each push records the ref; the sweep deletes it from Git and from here once
-- it expires, and a push past the owner's quota is refused.
CREATE TABLE actions_scratch_refs (
    repo_id    INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL CHECK (name LIKE 'refs/scratch/%' AND length(name) <= 255),
    owner_id   INTEGER REFERENCES accounts (id) ON DELETE CASCADE,
    sha        TEXT    NOT NULL CHECK (length(sha) BETWEEN 40 AND 64),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (repo_id, name)
) STRICT, WITHOUT ROWID;

CREATE INDEX actions_scratch_refs_by_age ON actions_scratch_refs (created_at);
