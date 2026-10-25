-- Email digests, on top of email notifications (0034): instead of one email per
-- notification, a user can collect them into one email a day or a week.

-- When a user's notification emails are sent ('immediate' keeps the behaviour from before
-- digests, so existing rows need no change) and, for digests, the hour (0-23, UTC) to send
-- them at; NULL follows the instance's default digest hour.
ALTER TABLE notification_email_settings
    ADD COLUMN delivery TEXT NOT NULL DEFAULT 'immediate' CHECK (delivery IN ('immediate', 'daily', 'weekly'));
ALTER TABLE notification_email_settings
    ADD COLUMN digest_hour INTEGER CHECK (digest_hour BETWEEN 0 AND 23);

-- One digest per user and period, claimed before it is sent. The unique period is what
-- makes sending idempotent: a digest that was claimed (and possibly sent) is never sent
-- again, even after a crash or a restart. `period` is the Unix second the period starts at
-- (the user's digest hour on that day, or on that week's Monday).
CREATE TABLE notification_digests (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    period      INTEGER NOT NULL,
    state       TEXT    NOT NULL DEFAULT 'claimed' CHECK (state IN ('claimed', 'sent', 'failed', 'empty')),
    items       INTEGER NOT NULL DEFAULT 0 CHECK (items >= 0),
    created_at  INTEGER NOT NULL,
    finished_at INTEGER,
    UNIQUE (account_id, period)
) STRICT;

CREATE INDEX notification_digests_by_age ON notification_digests (created_at);

-- Notifications waiting for a digest: what the email would have said. A digest claims them
-- by setting `digest_id`; rows still unclaimed belong to the next digest. Repository names
-- and thread titles are read when the digest is sent, so renames are followed.
CREATE TABLE notification_digest_items (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    repo_id     INTEGER NOT NULL REFERENCES repositories (id) ON DELETE CASCADE,
    issue_id    INTEGER NOT NULL REFERENCES issues (id) ON DELETE CASCADE,
    reason      TEXT    NOT NULL CHECK (reason IN ('author', 'comment', 'assigned', 'mention', 'review_requested', 'reviewed', 'manual')),
    headline    TEXT    NOT NULL,
    excerpt     TEXT    NOT NULL,
    anchor      TEXT,
    created_at  INTEGER NOT NULL,
    digest_id   INTEGER REFERENCES notification_digests (id) ON DELETE CASCADE
) STRICT;

CREATE INDEX notification_digest_items_pending ON notification_digest_items (account_id, created_at) WHERE digest_id IS NULL;
CREATE INDEX notification_digest_items_by_digest ON notification_digest_items (digest_id) WHERE digest_id IS NOT NULL;
CREATE INDEX notification_digest_items_by_age ON notification_digest_items (created_at);
