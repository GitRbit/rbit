-- rbit Actions (docs/design/actions.md §12.6): what steps wrote to GITHUB_STEP_SUMMARY, Markdown the
-- runner and the server masked, at most 64 KiB each (the protocol's MAX_STEP_SUMMARY_BYTES). Kept
-- apart from `actions_steps`, a WITHOUT ROWID table of small rows, and deleted with their task.
CREATE TABLE actions_step_summaries (
    task_id  INTEGER NOT NULL REFERENCES actions_tasks (id) ON DELETE CASCADE,
    number   INTEGER NOT NULL CHECK (number >= 0),
    summary  TEXT    NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 65536),
    PRIMARY KEY (task_id, number)
) STRICT;
