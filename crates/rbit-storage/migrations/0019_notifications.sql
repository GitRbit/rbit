-- Milestone 2 wave B, workstream I: in-app notifications.
--
-- `issue_subscriptions` lists who takes part in a thread and why; `subscribed = 0` is an
-- explicit unsubscribe, which sticks until the user subscribes again. `notifications`
-- holds one row per (user, thread): the inbox.

CREATE TABLE issue_subscriptions (
    issue_id   INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    reason     TEXT    NOT NULL CHECK (reason IN ('author', 'comment', 'assigned', 'mention', 'review_requested', 'reviewed', 'manual')),
    subscribed INTEGER NOT NULL DEFAULT 1 CHECK (subscribed IN (0, 1)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (issue_id, user_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX issue_subscriptions_by_user ON issue_subscriptions (user_id);

CREATE TABLE notifications (
    id            INTEGER PRIMARY KEY,
    user_id       INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    issue_id      INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    repo_id       INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    reason        TEXT    NOT NULL CHECK (reason IN ('author', 'comment', 'assigned', 'mention', 'review_requested', 'reviewed', 'manual')),
    unread        INTEGER NOT NULL DEFAULT 1 CHECK (unread IN (0, 1)),
    last_actor_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE (user_id, issue_id)
) STRICT;
CREATE INDEX notifications_inbox ON notifications (user_id, unread, updated_at DESC);
CREATE INDEX notifications_by_issue ON notifications (issue_id);
