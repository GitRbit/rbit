-- Existing accounts keep their passwords; newly provisioned accounts opt in atomically.
ALTER TABLE user_logins ADD COLUMN password_change_required INTEGER NOT NULL DEFAULT 0 CHECK (password_change_required IN (0, 1));
