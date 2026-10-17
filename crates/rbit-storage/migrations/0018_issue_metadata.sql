-- Milestone 2 wave B, workstream I: labels, milestones, assignees, and the tables that
-- de-duplicate commit references and cross-references between threads.
-- Everything is keyed by `issues (id)`, so it works for issues and pull requests alike.

CREATE TABLE labels (
    id          INTEGER PRIMARY KEY,
    repo_id     INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL COLLATE NOCASE CHECK (length(name) BETWEEN 1 AND 50),
    color       TEXT    NOT NULL CHECK (color GLOB '#[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'),
    description TEXT    NOT NULL DEFAULT '' CHECK (length(description) <= 100),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (repo_id, name)
) STRICT;

CREATE TABLE issue_labels (
    issue_id   INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (issue_id, label_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX issue_labels_by_label ON issue_labels (label_id);

CREATE TABLE milestones (
    id          INTEGER PRIMARY KEY,
    repo_id     INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    title       TEXT    NOT NULL COLLATE NOCASE CHECK (length(title) BETWEEN 1 AND 100),
    description TEXT    NOT NULL DEFAULT '',
    -- A calendar day, `YYYY-MM-DD`.
    due_on      TEXT    CHECK (due_on IS NULL OR due_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    state       TEXT    NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    closed_at   INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (repo_id, title),
    CHECK ((state = 'open') = (closed_at IS NULL))
) STRICT;

-- At most one milestone per thread.
CREATE TABLE issue_milestones (
    issue_id     INTEGER PRIMARY KEY REFERENCES issues(id) ON DELETE CASCADE,
    milestone_id INTEGER NOT NULL REFERENCES milestones(id) ON DELETE CASCADE
) STRICT;
CREATE INDEX issue_milestones_by_milestone ON issue_milestones (milestone_id);

CREATE TABLE issue_assignees (
    issue_id    INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id     INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    assigned_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (issue_id, user_id)
) STRICT;
CREATE INDEX issue_assignees_by_user ON issue_assignees (user_id);

-- A commit's `CommitReferenced` event is recorded once per thread, whichever branch it
-- arrives on.
CREATE TABLE issue_commit_refs (
    issue_id   INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    commit_sha TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (issue_id, commit_sha)
) STRICT, WITHOUT ROWID;

-- `CrossReferenced { source }` is recorded once per (target, source).
CREATE TABLE issue_cross_refs (
    target_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    source_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (target_id, source_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX issue_cross_refs_by_source ON issue_cross_refs (source_id);
