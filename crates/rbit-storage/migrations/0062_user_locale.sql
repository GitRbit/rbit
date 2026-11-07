-- Null follows browser/default negotiation. Persist only production catalog identities;
-- diagnostics and arbitrary header strings never become account preferences.
-- The session projection already reads user_logins. Keeping this user-only value on
-- that row avoids another indexed lookup on every authenticated request.
ALTER TABLE user_logins ADD COLUMN locale TEXT CHECK (locale IS NULL OR locale IN ('en', 'es'));
