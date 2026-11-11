-- rbit Actions: what the trigger decided for each pushed commit, so "no run will come" is a
-- recorded fact and not a guess. `workflows` is how many workflow files the commit has,
-- `started` how many of them started a run; `reason` says why none did (`skip_marker`,
-- `no_workflows`, `filtered` by `branches`/`paths`) or `started`. A later evaluation of the
-- same commit (pushed to another branch) keeps the larger `started`.
CREATE TABLE actions_commit_decisions (
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    sha         TEXT    NOT NULL CHECK (length(sha) BETWEEN 7 AND 64),
    workflows   INTEGER NOT NULL CHECK (workflows >= 0),
    started     INTEGER NOT NULL CHECK (started >= 0),
    reason      TEXT    NOT NULL CHECK (reason IN ('started', 'skip_marker', 'no_workflows', 'filtered')),
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (repo_id, sha)
) STRICT, WITHOUT ROWID;
