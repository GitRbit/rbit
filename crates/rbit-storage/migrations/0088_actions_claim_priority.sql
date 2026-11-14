-- The priority pass must skip runs that have not started and finished run history.
CREATE INDEX actions_runs_claim_priority ON actions_runs (id)
    WHERE finished_at IS NULL AND (started_at IS NOT NULL OR cancel_requested = 1);
