-- Stable account identities make repeated renames resolve in one hop. Reclaiming a
-- name permanently removes its old redirect, even if the new occupant later leaves.
CREATE TABLE account_redirects (
    name TEXT NOT NULL COLLATE NOCASE PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX account_redirects_by_account ON account_redirects (account_id);

CREATE TRIGGER account_redirects_on_create AFTER INSERT ON accounts
BEGIN
    DELETE FROM account_redirects WHERE name = NEW.name;
END;

CREATE TRIGGER account_redirects_on_rename AFTER UPDATE OF name ON accounts
WHEN OLD.name <> NEW.name COLLATE NOCASE
BEGIN
    DELETE FROM account_redirects WHERE name = NEW.name;
    INSERT OR REPLACE INTO account_redirects (name, account_id, created_at)
    VALUES (OLD.name, NEW.id, CAST(strftime('%s', 'now') AS INTEGER));
END;
