-- rbit Actions: annotations, the `::error`, `::warning` and `::notice` lines a task's log carries
-- (`##[kind]text` once the runner printed them), read from the lines as the server stores them,
-- after masking, so the run page lists them without reading logs. At most 50 per task (the first
-- ones; rbit_actions::logs::MAX_ANNOTATIONS_PER_TASK), at most 4 KiB each, deleted with their task.
-- `line` is the task-log line (from 0), which places the annotation in its step.
CREATE TABLE actions_annotations (
    task_id INTEGER NOT NULL REFERENCES actions_tasks (id) ON DELETE CASCADE,
    line    INTEGER NOT NULL CHECK (line >= 0),
    level   TEXT    NOT NULL CHECK (level IN ('error', 'warning', 'notice')),
    text    TEXT    NOT NULL CHECK (length(CAST(text AS BLOB)) BETWEEN 1 AND 4096),
    PRIMARY KEY (task_id, line)
) STRICT, WITHOUT ROWID;
