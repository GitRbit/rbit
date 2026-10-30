-- Keep cancelled runs pending until their final status reports succeed. A crash after
-- cancel_run commits must not make unfinished-run discovery forget those reports.
CREATE TABLE actions_plugin_revocations (
    run_id INTEGER PRIMARY KEY REFERENCES actions_runs (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX actions_tasks_unfinished ON actions_tasks (job_id, id) WHERE status IN (5, 6, 7, 8);
