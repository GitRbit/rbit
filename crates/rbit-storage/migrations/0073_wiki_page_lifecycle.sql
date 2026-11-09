-- Wiki page lifecycle: deletion keeps a page's history as a tombstone that can be
-- restored, and a rename leaves an alias so links to the old path keep working.
-- Requires the Wiki writer protocol 0069 and revision metadata 0071.
CREATE TEMP TABLE wiki_page_lifecycle_prerequisite (
    present INTEGER NOT NULL CONSTRAINT wiki_page_lifecycle_requires_0069_0071 CHECK (present = 2)
) STRICT;
INSERT INTO wiki_page_lifecycle_prerequisite
SELECT COUNT(*) FROM _sqlx_migrations WHERE version IN (69, 71) AND success = 1;
DROP TABLE wiki_page_lifecycle_prerequisite;

-- Per-page history no longer depends on the page being current: its rows stay when the
-- page is deleted and go only with their whole-wiki generation (the wiki itself). The
-- table is rebuilt without the cascading reference to wiki_pages; nothing refers to it.
CREATE TABLE wiki_page_revisions_kept (
    repo_id INTEGER NOT NULL,
    page_id INTEGER NOT NULL CHECK (page_id > 0),
    generation INTEGER NOT NULL,
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200 AND octet_length(title) <= 1024 AND instr(title, char(0)) = 0),
    PRIMARY KEY (repo_id, page_id, generation),
    FOREIGN KEY (repo_id, generation) REFERENCES wiki_revisions(repo_id, generation) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
INSERT INTO wiki_page_revisions_kept(repo_id, page_id, generation, path, title)
SELECT repo_id, page_id, generation, path, title FROM wiki_page_revisions;
DROP TABLE wiki_page_revisions;
ALTER TABLE wiki_page_revisions_kept RENAME TO wiki_page_revisions;

-- A deleted page: its last names, when it was created and the generation that deleted
-- it. Its ID is never reused, so restoring brings the same page and history back.
CREATE TABLE wiki_page_tombstones (
    repo_id INTEGER NOT NULL,
    page_id INTEGER NOT NULL CHECK (page_id > 0),
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 200 AND octet_length(title) <= 1024 AND instr(title, char(0)) = 0),
    created_at INTEGER NOT NULL,
    deleted_generation INTEGER NOT NULL,
    PRIMARY KEY (repo_id, page_id),
    FOREIGN KEY (repo_id, deleted_generation) REFERENCES wiki_revisions(repo_id, generation) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE INDEX wiki_page_tombstones_recent ON wiki_page_tombstones(repo_id, deleted_generation);

-- A path a page had before a rename. A current page at the same path always wins, and
-- taking a path removes its alias in the same transaction, so a redirect never loops.
CREATE TABLE wiki_page_aliases (
    repo_id INTEGER NOT NULL,
    path TEXT COLLATE BINARY NOT NULL CHECK (octet_length(path) BETWEEN 1 AND 512 AND instr(path, char(0)) = 0),
    page_id INTEGER NOT NULL CHECK (page_id > 0),
    generation INTEGER NOT NULL,
    PRIMARY KEY (repo_id, path),
    FOREIGN KEY (repo_id, generation) REFERENCES wiki_revisions(repo_id, generation) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
CREATE INDEX wiki_page_aliases_page ON wiki_page_aliases(repo_id, page_id);

-- The same writer protocol guard as every other wiki catalog table (0069).
CREATE TRIGGER wiki_protocol_wiki_page_revisions_insert BEFORE INSERT ON wiki_page_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_revisions_update BEFORE UPDATE ON wiki_page_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_revisions_delete BEFORE DELETE ON wiki_page_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_tombstones_insert BEFORE INSERT ON wiki_page_tombstones BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_tombstones_update BEFORE UPDATE ON wiki_page_tombstones BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_tombstones_delete BEFORE DELETE ON wiki_page_tombstones BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_aliases_insert BEFORE INSERT ON wiki_page_aliases BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_aliases_update BEFORE UPDATE ON wiki_page_aliases BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_wiki_page_aliases_delete BEFORE DELETE ON wiki_page_aliases BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
