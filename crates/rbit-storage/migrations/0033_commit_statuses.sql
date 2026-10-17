-- Commit statuses (milestone 3, workstream S): what external CI reports about a commit,
-- shaped like GitHub's commit status API, and the status checks a branch protection
-- rule requires before a pull request may be merged.

-- Every report is kept; for each (repository, commit, context) the row with the highest
-- id is the current one. The service caps the rows per triple (GitHub's 1000).
CREATE TABLE commit_statuses (
    id           INTEGER PRIMARY KEY,
    repo_id      INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    -- A full, lower-case commit id (SHA-1 or SHA-256).
    sha          TEXT    NOT NULL CHECK (length(sha) IN (40, 64) AND sha NOT GLOB '*[^0-9a-f]*'),
    state        TEXT    NOT NULL CHECK (state IN ('pending', 'success', 'failure', 'error')),
    context      TEXT    NOT NULL CHECK (length(context) BETWEEN 1 AND 255),
    description  TEXT    CHECK (description IS NULL OR length(description) <= 140),
    target_url   TEXT    CHECK (target_url IS NULL OR length(target_url) <= 2048),
    creator_id   INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
) STRICT;

-- The latest report per context, the history of one commit, and the count per triple.
CREATE INDEX commit_statuses_by_context ON commit_statuses (repo_id, sha, context, id);

-- A rule with this flag lets a pull request merge only when every required context's
-- latest status on the head commit is `success`.
ALTER TABLE branch_protections
    ADD COLUMN require_status_checks INTEGER NOT NULL DEFAULT 0 CHECK (require_status_checks IN (0, 1));

-- The contexts a rule requires (kept only while `require_status_checks` is set).
CREATE TABLE branch_protection_required_checks (
    rule_id  INTEGER NOT NULL REFERENCES branch_protections (id) ON DELETE CASCADE,
    context  TEXT    NOT NULL CHECK (length(context) BETWEEN 1 AND 255),
    PRIMARY KEY (rule_id, context)
) STRICT;
