-- rbit Actions: a push considers at most 16 updated branches and tags; the commits of the
-- rest are recorded with the reason `too_many_refs`, so their status says why no run comes.
-- A CHECK constraint cannot be changed in place: the table is rebuilt with every row.
CREATE TABLE actions_commit_decisions_next (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    sha         TEXT    NOT NULL CHECK (length(sha) BETWEEN 7 AND 64),
    workflows   INTEGER NOT NULL CHECK (workflows >= 0),
    started     INTEGER NOT NULL CHECK (started >= 0),
    reason      TEXT    NOT NULL CHECK (reason IN ('started', 'skip_marker', 'no_workflows', 'filtered', 'too_many_refs')),
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (repo_id, sha)
) STRICT, WITHOUT ROWID;

INSERT INTO actions_commit_decisions_next (repo_id, sha, workflows, started, reason, recorded_at)
    SELECT repo_id, sha, workflows, started, reason, recorded_at FROM actions_commit_decisions;

DROP TABLE actions_commit_decisions;

ALTER TABLE actions_commit_decisions_next RENAME TO actions_commit_decisions;
