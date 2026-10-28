-- no-transaction
-- rbit Actions: runs started by `workflow_run` (another run's activity) and
-- `repository_dispatch` (the REST API). SQLite cannot alter a CHECK constraint, so
-- actions_runs is rebuilt as https://www.sqlite.org/lang_altertable.html#otheralter
-- describes: with foreign keys off, so that dropping the old table does not cascade into
-- the jobs and artifacts that reference it, in one transaction, every row copied with its
-- id (so every reference still holds), then the indexes recreated.
PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE actions_runs_v2 (
    id                  INTEGER PRIMARY KEY,
    repo_id             INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    run_number          INTEGER NOT NULL CHECK (run_number > 0),
    workflow_path       TEXT    NOT NULL CHECK (length(workflow_path) BETWEEN 1 AND 1024),
    workflow_name       TEXT    NOT NULL CHECK (length(workflow_name) <= 1024),
    title               TEXT    NOT NULL CHECK (length(title) <= 1024),
    event               TEXT    NOT NULL CHECK (event IN ('push', 'pull_request', 'workflow_dispatch', 'schedule',
                                                          'workflow_run', 'repository_dispatch')),
    ref                 TEXT    NOT NULL CHECK (length(ref) BETWEEN 1 AND 1024),
    sha                 TEXT    NOT NULL CHECK (length(sha) IN (40, 64) AND sha NOT GLOB '*[^0-9a-f]*'),
    actor_id            INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    trigger_actor_id    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    pull_number         INTEGER CHECK (pull_number IS NULL OR pull_number > 0),
    is_fork             INTEGER NOT NULL DEFAULT 0 CHECK (is_fork IN (0, 1)),
    -- Untrusted runs get a read-only token, no secrets and no variables (§11.1).
    trusted             INTEGER NOT NULL CHECK (trusted IN (0, 1)),
    status              INTEGER NOT NULL CHECK (status BETWEEN 1 AND 8),
    attempt             INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    concurrency_group   TEXT    CHECK (concurrency_group IS NULL OR length(concurrency_group) BETWEEN 1 AND 1024),
    cancel_in_progress  INTEGER NOT NULL DEFAULT 0 CHECK (cancel_in_progress IN (0, 1)),
    -- The GitHub-shaped event payload (JSON).
    event_payload       TEXT    NOT NULL CHECK (length(event_payload) <= 262144 AND json_valid(event_payload)),
    -- workflow_dispatch: the checked inputs (JSON object).
    dispatch_inputs     TEXT    CHECK (dispatch_inputs IS NULL OR json_valid(dispatch_inputs)),
    -- Why the workflow produced no jobs.
    error               TEXT    CHECK (error IS NULL OR length(error) <= 2048),
    approved_by         INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    approved_at         INTEGER,
    created_at          INTEGER NOT NULL,
    started_at          INTEGER,
    finished_at         INTEGER,
    -- When the run's logs were removed (retention or the instance log budget).
    logs_expired_at     INTEGER,
    -- The `workflow_run` action last announced to webhooks, so each is sent once.
    announced           TEXT    CHECK (announced IN ('requested', 'in_progress', 'completed')),
    UNIQUE (repo_id, run_number)
) STRICT;

INSERT INTO actions_runs_v2 (id, repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, actor_id,
                             trigger_actor_id, pull_number, is_fork, trusted, status, attempt, concurrency_group,
                             cancel_in_progress, event_payload, dispatch_inputs, error, approved_by, approved_at,
                             created_at, started_at, finished_at, logs_expired_at, announced)
SELECT id, repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, actor_id,
       trigger_actor_id, pull_number, is_fork, trusted, status, attempt, concurrency_group,
       cancel_in_progress, event_payload, dispatch_inputs, error, approved_by, approved_at,
       created_at, started_at, finished_at, logs_expired_at, announced
FROM actions_runs;

DROP TABLE actions_runs;
ALTER TABLE actions_runs_v2 RENAME TO actions_runs;

CREATE INDEX actions_runs_by_repo ON actions_runs (repo_id, id DESC);
CREATE INDEX actions_runs_by_workflow ON actions_runs (repo_id, workflow_path, id DESC);
CREATE INDEX actions_runs_by_group ON actions_runs (repo_id, concurrency_group, status)
    WHERE concurrency_group IS NOT NULL AND status IN (5, 6, 7, 8);
CREATE INDEX actions_runs_by_pull ON actions_runs (repo_id, pull_number) WHERE pull_number IS NOT NULL;
-- Log pruning, oldest finished runs first.
CREATE INDEX actions_runs_with_logs ON actions_runs (finished_at) WHERE finished_at IS NOT NULL AND logs_expired_at IS NULL;

COMMIT;

PRAGMA foreign_keys = ON;
