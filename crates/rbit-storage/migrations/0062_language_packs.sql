-- Language packs are immutable compressed data at rest in the same SQLite snapshot as
-- installation metadata. No installer filesystem paths or executable hooks are stored.
CREATE TABLE language_packs (
    id TEXT PRIMARY KEY CHECK (length(id) BETWEEN 1 AND 128 AND id NOT GLOB '*[^a-z0-9.-]*'),
    locale TEXT NOT NULL CHECK (length(locale) BETWEEN 1 AND 64 AND locale NOT GLOB '*[^A-Za-z0-9-]*'),
    version TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 128),
    bytes BLOB NOT NULL CHECK (length(bytes) BETWEEN 1 AND 2097152),
    digest TEXT NOT NULL CHECK (length(digest) = 64 AND digest NOT GLOB '*[^0-9a-f]*'),
    message_count INTEGER NOT NULL CHECK (message_count BETWEEN 0 AND 65536),
    message_total INTEGER NOT NULL CHECK (message_total BETWEEN 1 AND 65536),
    installed_by INTEGER REFERENCES accounts (id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TRIGGER language_packs_capacity BEFORE INSERT ON language_packs
WHEN (SELECT COUNT(*) FROM language_packs) >= 64
    AND NOT EXISTS (SELECT 1 FROM language_packs WHERE id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'language pack capacity exhausted');
END;

-- Preferences are independent of plugin availability and survive removal of a pack.
CREATE TABLE account_locales (
    account_id INTEGER PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    locale TEXT NOT NULL CHECK (length(locale) BETWEEN 1 AND 64 AND locale NOT GLOB '*[^A-Za-z0-9-]*')
) STRICT;
