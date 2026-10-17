-- Email notifications (milestone 3 workstream N), on top of the in-app inbox (0019).

-- Which notifications a user also receives by email. No row means the configured default
-- (`[notifications] default_email`, "participating" unless changed).
CREATE TABLE notification_email_settings (
    account_id  INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    email       TEXT    NOT NULL CHECK (email IN ('off', 'participating', 'all')),
    updated_at  INTEGER NOT NULL
) STRICT;

-- One row per email sent (or deliberately not sent: throttled) about a thread: the dedup
-- key names the activity (`comment-17`, `review-4`, `opened`, `closed-1700000000`), so a
-- notification is mailed at most once. Rows are pruned with the inbox's retention.
CREATE TABLE notification_emails (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    issue_id    INTEGER NOT NULL REFERENCES issues (id) ON DELETE CASCADE,
    dedup_key   TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,
    UNIQUE (account_id, issue_id, dedup_key)
) STRICT;

CREATE INDEX notification_emails_by_age ON notification_emails (created_at);

-- Keys the server generates for itself on first use (for example the key that signs the
-- one-click unsubscribe links in notification emails). Like webhook secrets they are kept
-- raw, because HMAC needs the key itself; they never leave the server.
CREATE TABLE server_secrets (
    name        TEXT    PRIMARY KEY,
    secret      BLOB    NOT NULL,
    created_at  INTEGER NOT NULL
) STRICT;
