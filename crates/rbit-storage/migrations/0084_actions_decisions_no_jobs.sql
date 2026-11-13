-- rbit Actions: a scratch push whose `Rbit-CI-Jobs` trailer names no job of its workflows
-- starts no run; its commit is recorded with the reason `no_jobs`, so its status says why
-- instead of waiting for runs that never come.
-- A CHECK constraint cannot be changed in place: the table is rebuilt with every row.
CREATE TABLE actions_commit_decisions_next (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    sha         TEXT    NOT NULL CHECK (length(sha) BETWEEN 7 AND 64),
    workflows   INTEGER NOT NULL CHECK (workflows >= 0),
    started     INTEGER NOT NULL CHECK (started >= 0),
    reason      TEXT    NOT NULL CHECK (reason IN ('started', 'skip_marker', 'no_workflows', 'filtered', 'too_many_refs', 'no_jobs')),
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (repo_id, sha)
) STRICT, WITHOUT ROWID;

INSERT INTO actions_commit_decisions_next (repo_id, sha, workflows, started, reason, recorded_at)
    SELECT repo_id, sha, workflows, started, reason, recorded_at FROM actions_commit_decisions;

DROP TABLE actions_commit_decisions;

ALTER TABLE actions_commit_decisions_next RENAME TO actions_commit_decisions;
