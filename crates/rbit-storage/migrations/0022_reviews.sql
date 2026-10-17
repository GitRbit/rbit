-- Pull request reviews, review comments on diff lines and review requests (milestone 2
-- wave B, workstream P).

CREATE TABLE pull_reviews (
    id                 INTEGER PRIMARY KEY,
    pull_id            INTEGER NOT NULL REFERENCES pull_requests (issue_id) ON DELETE CASCADE,
    reviewer_id        INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    state              TEXT    NOT NULL CHECK (state IN ('pending', 'commented', 'approved', 'changes_requested', 'dismissed')),
    body               TEXT    NOT NULL DEFAULT '',
    -- The head commit the review was made on.
    commit_sha         TEXT    NOT NULL,
    created_at         INTEGER NOT NULL,
    -- NULL while pending.
    submitted_at       INTEGER,
    dismissed_by       INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    dismissed_at       INTEGER,
    dismissal_message  TEXT,
    CHECK ((state = 'pending') = (submitted_at IS NULL)),
    CHECK ((state = 'dismissed') = (dismissed_at IS NOT NULL))
) STRICT;
CREATE INDEX pull_reviews_by_pull ON pull_reviews (pull_id, created_at, id);
-- One pending review per user and pull request.
CREATE UNIQUE INDEX pull_reviews_one_pending ON pull_reviews (pull_id, reviewer_id) WHERE state = 'pending';

CREATE TABLE pull_review_comments (
    id           INTEGER PRIMARY KEY,
    review_id    INTEGER NOT NULL REFERENCES pull_reviews (id) ON DELETE CASCADE,
    pull_id      INTEGER NOT NULL REFERENCES pull_requests (issue_id) ON DELETE CASCADE,
    author_id    INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    path         TEXT    NOT NULL CHECK (path <> ''),
    side         TEXT    NOT NULL CHECK (side IN ('left', 'right')),
    line         INTEGER NOT NULL CHECK (line > 0),
    commit_sha   TEXT    NOT NULL,
    -- Up to four lines of the diff ending at the commented line.
    diff_hunk    TEXT    NOT NULL DEFAULT '',
    in_reply_to  INTEGER REFERENCES pull_review_comments (id) ON DELETE SET NULL,
    body         TEXT    NOT NULL,
    outdated     INTEGER NOT NULL DEFAULT 0 CHECK (outdated IN (0, 1)),
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
) STRICT;
CREATE INDEX pull_review_comments_by_path ON pull_review_comments (pull_id, path);
CREATE INDEX pull_review_comments_by_review ON pull_review_comments (review_id);

CREATE TABLE pull_review_requests (
    pull_id       INTEGER NOT NULL REFERENCES pull_requests (issue_id) ON DELETE CASCADE,
    user_id       INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    requested_by  INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    created_at    INTEGER NOT NULL,
    PRIMARY KEY (pull_id, user_id)
) STRICT;
