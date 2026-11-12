-- rbit Actions: the tests a job attempt's report says passed (agent plan step 3.6), at most
-- 2,000 ids per attempt, so a failed test that passed in another attempt or run of the
-- same job at the same commit reads as flaky even when that job failed for another test.
-- A newer report of the same attempt replaces them.
CREATE TABLE actions_test_passes (
    task_id INTEGER NOT NULL REFERENCES actions_tasks (id) ON DELETE CASCADE,
    test_id TEXT    NOT NULL CHECK (length(CAST(test_id AS BLOB)) BETWEEN 1 AND 1100),
    PRIMARY KEY (task_id, test_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX actions_test_passes_by_test ON actions_test_passes (test_id);
