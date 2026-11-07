-- Startup scratch only: persisted rows/phase are NEVER integrity/readiness proof.
-- Owner restart must remove old work in bounded batches and rescan actual bytes.
CREATE TEMP TABLE wiki_scan_prerequisite(present INTEGER NOT NULL CHECK(present=1)) STRICT;
INSERT INTO wiki_scan_prerequisite SELECT COUNT(*) FROM _sqlx_migrations WHERE version=69 AND success=1;
DROP TABLE wiki_scan_prerequisite;

CREATE TABLE wiki_scan_control (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    epoch INTEGER NOT NULL CHECK(epoch>=0),
    phase TEXT NOT NULL CHECK(phase IN ('idle','scan','edges','agreement')),
    max_scopes INTEGER NOT NULL CHECK(max_scopes>=0),
    max_objects INTEGER NOT NULL CHECK(max_objects>=0),
    max_edges INTEGER NOT NULL CHECK(max_edges>=0),
    max_bytes INTEGER NOT NULL CHECK(max_bytes>=0),
    scopes INTEGER NOT NULL CHECK(scopes BETWEEN 0 AND max_scopes),
    objects INTEGER NOT NULL CHECK(objects BETWEEN 0 AND max_objects),
    edges INTEGER NOT NULL CHECK(edges BETWEEN 0 AND max_edges),
    logical_bytes INTEGER NOT NULL CHECK(logical_bytes BETWEEN 0 AND max_bytes)
) STRICT;
INSERT INTO wiki_scan_control VALUES(1,0,'idle',0,0,0,0,0,0,0,0);

CREATE TABLE wiki_scan_scopes (
    epoch INTEGER NOT NULL CHECK(epoch>0),
    wiki_id INTEGER NOT NULL CHECK(wiki_id>0),
    repo_id INTEGER NOT NULL CHECK(repo_id>0),
    scanned INTEGER NOT NULL DEFAULT 0 CHECK(scanned IN (0,1)),
    PRIMARY KEY(epoch,wiki_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX wiki_scan_unfinished_scopes ON wiki_scan_scopes(epoch,wiki_id) WHERE scanned=0;

CREATE TABLE wiki_scan_objects (
    epoch INTEGER NOT NULL,
    wiki_id INTEGER NOT NULL,
    oid BLOB NOT NULL CHECK(length(oid)=20 AND oid<>zeroblob(20)),
    kind INTEGER NOT NULL CHECK(kind IN (1,2,3)),
    bytes INTEGER NOT NULL CHECK(bytes BETWEEN 0 AND 1048576),
    complete INTEGER NOT NULL DEFAULT 0 CHECK(complete IN (0,1)),
    next_ordinal INTEGER NOT NULL DEFAULT 0 CHECK(next_ordinal BETWEEN 0 AND 10000),
    actor_id INTEGER CHECK(actor_id>0),
    actor_incarnation INTEGER CHECK(actor_incarnation>0),
    committed_at INTEGER,
    PRIMARY KEY(epoch,wiki_id,oid),
    FOREIGN KEY(epoch,wiki_id) REFERENCES wiki_scan_scopes(epoch,wiki_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX wiki_scan_unfinished_objects ON wiki_scan_objects(epoch,wiki_id,oid) WHERE complete=0;

CREATE TABLE wiki_scan_edges (
    epoch INTEGER NOT NULL,
    wiki_id INTEGER NOT NULL,
    source BLOB NOT NULL CHECK(length(source)=20 AND source<>zeroblob(20)),
    ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 0 AND 9999),
    target BLOB NOT NULL CHECK(length(target)=20 AND target<>zeroblob(20)),
    expected_kind INTEGER NOT NULL CHECK(expected_kind IN (1,2,3)),
    page_id INTEGER CHECK(page_id>0),
    PRIMARY KEY(epoch,wiki_id,source,ordinal),
    FOREIGN KEY(epoch,wiki_id,source) REFERENCES wiki_scan_objects(epoch,wiki_id,oid)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX wiki_scan_tree_page_once ON wiki_scan_edges(epoch,wiki_id,source,page_id) WHERE page_id IS NOT NULL;

-- Default, old, cached and unrelated writers cannot manufacture scan facts.
-- There are no cascading deletes: cleanup must delete bounded edge/object/scope
-- batches in that order; a whole-history cascade is not a cleanup shortcut.
CREATE TRIGGER wiki_scan_control_insert BEFORE INSERT ON wiki_scan_control BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_control_update BEFORE UPDATE ON wiki_scan_control BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_control_delete BEFORE DELETE ON wiki_scan_control BEGIN
    SELECT RAISE(ABORT,'wiki scratch control cannot be deleted');
END;
CREATE TRIGGER wiki_scan_scopes_insert BEFORE INSERT ON wiki_scan_scopes BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_scopes_update BEFORE UPDATE ON wiki_scan_scopes BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_scopes_delete BEFORE DELETE ON wiki_scan_scopes BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_objects_insert BEFORE INSERT ON wiki_scan_objects BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_objects_update BEFORE UPDATE ON wiki_scan_objects BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_objects_delete BEFORE DELETE ON wiki_scan_objects BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_edges_insert BEFORE INSERT ON wiki_scan_edges BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_edges_update BEFORE UPDATE ON wiki_scan_edges BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
CREATE TRIGGER wiki_scan_edges_delete BEFORE DELETE ON wiki_scan_edges BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1<>'permit' THEN RAISE(ABORT,'wiki scratch writer refused') END;
END;
