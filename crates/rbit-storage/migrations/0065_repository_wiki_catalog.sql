-- W1 read foundation only. No application publisher or wiki routes are enabled.
-- MUST follow credential allocator 0063 and durable recovery 0064. Never install
-- this file alone into an earlier schema or renumber existing migrations.
CREATE TEMP TABLE wiki_prerequisites (
    present INTEGER NOT NULL CONSTRAINT wiki_requires_migrations_0063_and_0064 CHECK (present = 2)
) STRICT;
INSERT INTO wiki_prerequisites
SELECT COUNT(*) FROM _sqlx_migrations WHERE version IN (63, 64) AND success = 1;
DROP TABLE wiki_prerequisites;

CREATE TABLE wiki_store_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    identity BLOB NOT NULL CHECK (length(identity) = 16 AND identity <> zeroblob(16))
) STRICT;
INSERT INTO wiki_store_identity VALUES (1, randomblob(16));

CREATE TABLE wiki_repositories (
    repo_id INTEGER PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    generation INTEGER CHECK (generation > 0),
    FOREIGN KEY (repo_id, generation) REFERENCES wiki_revisions(repo_id, generation)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

-- Immutable finalized head history. An unpublished prepared Git object does not
-- belong here. A future intent/CAS publisher must enforce this transaction boundary.
CREATE TABLE wiki_revisions (
    repo_id INTEGER NOT NULL REFERENCES wiki_repositories(repo_id) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation > 0),
    revision TEXT NOT NULL CHECK (
        length(revision) IN (40, 64) AND revision NOT GLOB '*[^0-9a-f]*'
        AND ltrim(revision, '0') <> ''
    ),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (repo_id, generation)
) STRICT, WITHOUT ROWID;

CREATE TABLE wiki_pages (
    id INTEGER PRIMARY KEY AUTOINCREMENT CHECK (id > 0),
    repo_id INTEGER NOT NULL REFERENCES wiki_repositories(repo_id) ON DELETE CASCADE,
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200 AND octet_length(title) <= 1024 AND instr(title, char(0)) = 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (repo_id, path),
    UNIQUE (repo_id, id)
) STRICT;

-- The page ID is stable across renames. SQL binary namespace matches the domain's
-- NFC/case-sensitive bytes; all official writes must use its validated name types.
CREATE TABLE wiki_page_revisions (
    repo_id INTEGER NOT NULL,
    page_id INTEGER NOT NULL,
    generation INTEGER NOT NULL,
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200 AND octet_length(title) <= 1024 AND instr(title, char(0)) = 0),
    PRIMARY KEY (repo_id, page_id, generation),
    FOREIGN KEY (repo_id, page_id) REFERENCES wiki_pages(repo_id, id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id, generation) REFERENCES wiki_revisions(repo_id, generation) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
