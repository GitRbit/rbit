-- rbit Actions: whether a run was cancelled as a whole (by a person, a newer push, its
-- concurrency group, or the abandoned-job sweep). A run whose only cancelled jobs are the
-- matrix siblings `fail-fast` stopped after one failed concludes `failure`, as on GitHub;
-- a run cancelled as a whole stays `cancelled` even when a job had failed before. A new
-- attempt clears it.
ALTER TABLE actions_runs ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1));
