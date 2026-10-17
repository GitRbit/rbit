-- Administrator overrides of the instance settings. Keys absent here fall back to the
-- static configuration (see `rustit_core::InstanceSettings::defaults`).
CREATE TABLE instance_settings (
    key         TEXT    PRIMARY KEY,
    value       TEXT    NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- When set, the user can no longer sign in, resume sessions, use access tokens or use Git.
ALTER TABLE user_logins ADD COLUMN suspended_at INTEGER;

CREATE INDEX user_logins_admins ON user_logins (is_admin) WHERE is_admin = 1;
