-- Desired plugin state survives a restart independently of deployment defaults.
-- A pending disable must finish revoking workload authority before reactivation.
CREATE TABLE plugin_settings (
    plugin_id TEXT PRIMARY KEY CHECK (length(plugin_id) BETWEEN 1 AND 128),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    revocation_pending INTEGER NOT NULL DEFAULT 0 CHECK (revocation_pending IN (0, 1)),
    updated_by INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Lifecycle reconciliation visits active work without scanning retained run history.
CREATE INDEX actions_runs_unfinished ON actions_runs (id, repo_id) WHERE status IN (5, 6, 7, 8);
