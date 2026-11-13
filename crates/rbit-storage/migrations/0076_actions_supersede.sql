-- rbit Actions: superseded runs. A repository can let a newer push to a branch (other than
-- its default branch) cancel that branch's older runs of the same workflow, without each
-- workflow declaring `concurrency`. Off unless a row says `enabled = 1`.
CREATE TABLE actions_supersede_settings (
    repo_id INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1))
) STRICT;

-- The commit whose run superseded a cancelled run, by the setting above or by a
-- `concurrency` group with `cancel-in-progress`: "superseded by <sha>", not just "cancelled".
CREATE TABLE actions_supersessions (
    run_id     INTEGER PRIMARY KEY REFERENCES actions_runs (id) ON DELETE CASCADE,
    newer_sha  TEXT    NOT NULL CHECK (length(newer_sha) BETWEEN 7 AND 64),
    recorded_at INTEGER NOT NULL
) STRICT;
