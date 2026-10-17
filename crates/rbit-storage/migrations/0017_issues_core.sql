-- The shared core of issues and pull requests (milestone 2 wave B, T0): one number
-- sequence per repository, the thread row, its comments and its timeline events.
--
-- Only `sqlite/issues.rs` writes these tables; other stores may read them and reference
-- `issues (id)`. Pull requests are rows of kind `pull`; their branch data lives in the
-- pull request workstream's own tables, keyed by `issue_id`.

-- The last number handed out per repository. Numbers are taken with an upsert inside the
-- inserting transaction, so a deleted issue's number is never reused.
CREATE TABLE issue_counters (
    repo_id      INTEGER PRIMARY KEY REFERENCES repositories (id) ON DELETE CASCADE,
    last_number  INTEGER NOT NULL CHECK (last_number > 0)
) STRICT;

CREATE TABLE issues (
    id             INTEGER PRIMARY KEY,
    repo_id        INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    -- Shared by issues and pull requests.
    number         INTEGER NOT NULL CHECK (number > 0),
    kind           TEXT    NOT NULL CHECK (kind IN ('issue', 'pull')),
    title          TEXT    NOT NULL,
    body           TEXT    NOT NULL DEFAULT '',
    author_id      INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    state          TEXT    NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'closed')),
    state_reason   TEXT    CHECK (state_reason IN ('completed', 'not_planned', 'merged')),
    closed_at      INTEGER,
    closed_by      INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    locked         INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
    comment_count  INTEGER NOT NULL DEFAULT 0 CHECK (comment_count >= 0),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    UNIQUE (repo_id, number),
    CHECK ((state = 'open') = (closed_at IS NULL)),
    CHECK ((state = 'open') = (state_reason IS NULL)),
    CHECK (state_reason IS NOT 'merged' OR kind = 'pull')
) STRICT;

-- Lists by repository, kind and state (newest first) and the open counts of the header.
CREATE INDEX issues_by_repo ON issues (repo_id, kind, state, number DESC);
CREATE INDEX issues_by_author ON issues (author_id, state);

CREATE TABLE issue_comments (
    id          INTEGER PRIMARY KEY,
    issue_id    INTEGER NOT NULL REFERENCES issues (id) ON DELETE CASCADE,
    author_id   INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    body        TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    -- NULL until the comment is edited.
    edited_at   INTEGER
) STRICT;

CREATE INDEX issue_comments_by_issue ON issue_comments (issue_id, created_at, id);

-- Everything the timeline shows besides comments: `kind` is the `TimelineEvent` variant
-- in snake_case, `data` its fields as a JSON object (`{}` for variants without fields).
-- Kinds the running version does not know are skipped when reading.
CREATE TABLE issue_events (
    id          INTEGER PRIMARY KEY,
    issue_id    INTEGER NOT NULL REFERENCES issues (id) ON DELETE CASCADE,
    actor_id    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    kind        TEXT    NOT NULL,
    data        TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(data)),
    created_at  INTEGER NOT NULL
) STRICT;

CREATE INDEX issue_events_by_issue ON issue_events (issue_id, created_at, id);
