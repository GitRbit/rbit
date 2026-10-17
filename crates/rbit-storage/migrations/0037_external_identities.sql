-- External sign-in (milestone 3 workstream O): accounts linked to an identity at an
-- OpenID Connect provider or GitHub, configured as `[[auth.providers]]`.
--
-- `provider_id` is the configured provider's `id` (a slug, not a foreign key: providers
-- live in the configuration file). `subject` is the provider's stable user id (the ID
-- token's `sub`, or GitHub's numeric user id), never an email address or a login name,
-- which can change hands. One identity belongs to one account, and an account links at
-- most one identity per provider. `email` and `username` are what the provider last
-- reported, shown in settings only; linking never trusts them.
CREATE TABLE external_identities (
    id            INTEGER PRIMARY KEY,
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    provider_id   TEXT    NOT NULL,
    subject       TEXT    NOT NULL,
    email         TEXT,
    username      TEXT,
    created_at    INTEGER NOT NULL,
    last_used_at  INTEGER,
    UNIQUE (provider_id, subject),
    UNIQUE (account_id, provider_id)
) STRICT;
