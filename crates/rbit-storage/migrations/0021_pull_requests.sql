-- Pull requests (milestone 2 wave B, workstream P): the branch data attached to an
-- `issues` row of kind `pull`, the last trial merge and the merge result.
--
-- The head commit is kept in the base repository as `refs/pull/{number}/head`, so a
-- pull request keeps working after its head repository is deleted (`head_repo_id`
-- becomes NULL).
CREATE TABLE pull_requests (
    issue_id            INTEGER PRIMARY KEY REFERENCES issues (id) ON DELETE CASCADE,
    -- The base repository (the issue row's repository).
    repo_id             INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    head_repo_id        INTEGER REFERENCES repositories (id) ON DELETE SET NULL,
    head_branch         TEXT    NOT NULL CHECK (head_branch <> ''),
    base_branch         TEXT    NOT NULL CHECK (base_branch <> ''),
    head_sha            TEXT    NOT NULL,
    is_draft            INTEGER NOT NULL DEFAULT 0 CHECK (is_draft IN (0, 1)),
    mergeable           TEXT    NOT NULL DEFAULT 'unknown' CHECK (mergeable IN ('unknown', 'clean', 'conflict', 'error')),
    -- The ids the stored mergeability was computed on, so stale results are recognisable.
    mergeable_base_sha  TEXT,
    mergeable_head_sha  TEXT,
    -- A JSON array of at most 100 paths, for `conflict`.
    conflict_paths      TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(conflict_paths)),
    merged_at           INTEGER,
    merged_by           INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    -- NULL for a pull request merged outside Rbit.
    merge_strategy      TEXT    CHECK (merge_strategy IN ('merge', 'squash', 'rebase')),
    merge_commit_sha    TEXT,
    merged_base_sha     TEXT,
    merged_head_sha     TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    CHECK (merge_strategy IS NULL OR merged_at IS NOT NULL)
) STRICT;

-- Head pushes find the open pull requests of a branch; base pushes those into a branch.
CREATE INDEX pull_requests_by_head ON pull_requests (head_repo_id, head_branch);
CREATE INDEX pull_requests_by_base ON pull_requests (repo_id, base_branch);
-- The mergeability worker picks up `unknown` rows.
CREATE INDEX pull_requests_by_mergeable ON pull_requests (repo_id, mergeable);
