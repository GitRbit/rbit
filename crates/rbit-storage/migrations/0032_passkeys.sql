-- Passkeys and security keys (WebAuthn) registered as a second factor. An account has
-- two-factor authentication when it has a confirmed `two_factor` row, a passkey, or both;
-- the recovery codes in `recovery_codes` belong to the account, not to one factor, and
-- are removed only when its last factor is.
CREATE TABLE passkeys (
    id             INTEGER PRIMARY KEY,
    account_id     INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- Chosen by the authenticator; unique across every account.
    credential_id  BLOB    NOT NULL UNIQUE,
    -- The COSE key exactly as registered, and its COSE algorithm (-7, -8 or -257).
    public_key     BLOB    NOT NULL,
    algorithm      INTEGER NOT NULL,
    -- The authenticator's signature counter: each sign-in must report a higher one,
    -- unless the authenticator keeps none (always 0).
    sign_count     INTEGER NOT NULL DEFAULT 0,
    -- Comma-separated transport hints (`usb`, `nfc`, `internal`, `hybrid`, ...).
    transports     TEXT    NOT NULL DEFAULT '',
    name           TEXT    NOT NULL,
    created_at     INTEGER NOT NULL,
    last_used_at   INTEGER
) STRICT;

CREATE INDEX passkeys_by_account ON passkeys (account_id, id);
