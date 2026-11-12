-- rbit Actions: test reports (agent plan step 3.4). A job attempt that uploads an
-- `rbit-test-report` artifact gets its counts here and its first failed tests below,
-- bounded by the server; a newer report of the same attempt replaces both.
CREATE TABLE actions_test_reports (
    task_id INTEGER PRIMARY KEY REFERENCES actions_tasks (id) ON DELETE CASCADE,
    total   INTEGER NOT NULL CHECK (total >= 0),
    failed  INTEGER NOT NULL CHECK (failed >= 0)
) STRICT;

CREATE TABLE actions_test_failures (
    task_id INTEGER NOT NULL REFERENCES actions_tasks (id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 99),
    test_id TEXT    NOT NULL CHECK (length(CAST(test_id AS BLOB)) BETWEEN 1 AND 1100),
    path    TEXT    CHECK (path IS NULL OR length(CAST(path AS BLOB)) <= 1100),
    line    INTEGER CHECK (line IS NULL OR line >= 0),
    message TEXT    NOT NULL CHECK (length(CAST(message AS BLOB)) <= 1100),
    PRIMARY KEY (task_id, ordinal)
) STRICT, WITHOUT ROWID;

CREATE INDEX actions_test_failures_by_test ON actions_test_failures (test_id);
