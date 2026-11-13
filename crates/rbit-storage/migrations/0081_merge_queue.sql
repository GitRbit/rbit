-- Agent loop: the merge queue (agent plan step 4.1). A pull request queued to land waits in
-- line per repository and base branch; the server builds its merge on the current base into
-- a candidate ref, CI tests that commit, and exactly that commit lands by compare-and-swap.
CREATE TABLE merge_queue (
    id            INTEGER PRIMARY KEY,
    repo_id       INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    issue_id      INTEGER NOT NULL REFERENCES issues (id) ON DELETE CASCADE,
    number        INTEGER NOT NULL CHECK (number > 0),
    base_branch   TEXT    NOT NULL CHECK (length(base_branch) BETWEEN 1 AND 255),
    head_sha      TEXT    NOT NULL CHECK (length(head_sha) IN (40, 64)),
    strategy      TEXT    NOT NULL CHECK (strategy IN ('merge', 'squash', 'rebase')),
    queued_by     INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    state         TEXT    NOT NULL CHECK (state IN ('waiting', 'testing', 'landed', 'failed', 'removed')),
    candidate_sha TEXT    CHECK (candidate_sha IS NULL OR length(candidate_sha) IN (40, 64)),
    base_sha      TEXT    CHECK (base_sha IS NULL OR length(base_sha) IN (40, 64)),
    attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    reason        TEXT    CHECK (reason IS NULL OR length(reason) <= 1000),
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
) STRICT;

-- One live entry per pull request.
CREATE UNIQUE INDEX merge_queue_live ON merge_queue (issue_id) WHERE state IN ('waiting', 'testing');
CREATE INDEX merge_queue_line ON merge_queue (repo_id, base_branch, state, id);
