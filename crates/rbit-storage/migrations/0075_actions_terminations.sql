-- rbit Actions: how a task ended (its termination record), so an agent learns "timed out",
-- "the runner was lost" or "step 6 exited 101" without reading the log. The runner reports
-- completed, step_failed, timeout, cancelled or setup_failed with the step and its exit code;
-- the server adds runner_lost, superseded and server_shutdown. The first record of a task is
-- kept. A task without one reads by its status (unknown for an unexplained failure).
CREATE TABLE actions_terminations (
    task_id     INTEGER PRIMARY KEY REFERENCES actions_tasks (id) ON DELETE CASCADE,
    reason      TEXT    NOT NULL CHECK (reason IN ('completed', 'step_failed', 'timeout', 'cancelled', 'superseded',
                                                   'runner_lost', 'setup_failed', 'server_shutdown', 'unknown')),
    step        INTEGER CHECK (step IS NULL OR step >= 0),
    exit_code   INTEGER,
    recorded_at INTEGER NOT NULL
) STRICT;
