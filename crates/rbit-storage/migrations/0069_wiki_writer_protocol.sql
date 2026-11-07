-- Stopped Wiki protocol only; no native integrity, readiness or route activation.
-- Requires reserved identity ACTIVE migration0068 and immutable Wiki0066.
CREATE TEMP TABLE wiki_protocol_prerequisites (
    present INTEGER NOT NULL CONSTRAINT wiki_protocol_requires_0066_0068 CHECK (present = 2)
) STRICT;
INSERT INTO wiki_protocol_prerequisites
SELECT COUNT(*) FROM _sqlx_migrations WHERE version IN (66, 68) AND success = 1;
DROP TABLE wiki_protocol_prerequisites;

CREATE TABLE wiki_writer_protocol (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    mode TEXT NOT NULL CHECK (mode IN ('legacy','preparing','managed')),
    epoch INTEGER NOT NULL CHECK (epoch >= 0)
) STRICT;
INSERT INTO wiki_writer_protocol VALUES(1,'legacy',0);

-- Control always requires the private owned-FULL callback, even in legacy mode.
CREATE TRIGGER wiki_protocol_control_insert BEFORE INSERT ON wiki_writer_protocol BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;
CREATE TRIGGER wiki_protocol_control_update BEFORE UPDATE ON wiki_writer_protocol BEGIN
    SELECT CASE WHEN 'managed' COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
    SELECT CASE WHEN NEW.epoch < OLD.epoch OR (NEW.mode='preparing' AND NEW.epoch <= OLD.epoch)
        OR (OLD.mode <> 'legacy' AND NEW.mode='legacy')
        THEN RAISE(ABORT, 'wiki protocol epoch or downgrade refused') END;
END;
CREATE TRIGGER wiki_protocol_control_delete BEFORE DELETE ON wiki_writer_protocol BEGIN
    SELECT RAISE(ABORT, 'wiki writer protocol cannot be deleted');
END;

-- A missing control row fails closed. New official general connections have only
-- the LEGACY comparison; old unregistered connections refuse on reprepare already.
-- Private FULL alone can write preparing/managed. These triggers also cover FK
-- cascades: destructive route bridges must be installed BEFORE mode activation.

CREATE TRIGGER wiki_protocol_wiki_store_identity_insert BEFORE INSERT ON wiki_store_identity BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_store_identity_update BEFORE UPDATE ON wiki_store_identity BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_store_identity_delete BEFORE DELETE ON wiki_store_identity BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_repositories_insert BEFORE INSERT ON wiki_repositories BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_repositories_update BEFORE UPDATE ON wiki_repositories BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_repositories_delete BEFORE DELETE ON wiki_repositories BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_revisions_insert BEFORE INSERT ON wiki_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_revisions_update BEFORE UPDATE ON wiki_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_revisions_delete BEFORE DELETE ON wiki_revisions BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_pages_insert BEFORE INSERT ON wiki_pages BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_pages_update BEFORE UPDATE ON wiki_pages BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_pages_delete BEFORE DELETE ON wiki_pages BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

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

CREATE TRIGGER wiki_protocol_wiki_actor_incarnations_insert BEFORE INSERT ON wiki_actor_incarnations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_actor_incarnations_update BEFORE UPDATE ON wiki_actor_incarnations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_actor_incarnations_delete BEFORE DELETE ON wiki_actor_incarnations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_page_allocations_insert BEFORE INSERT ON wiki_page_allocations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_page_allocations_update BEFORE UPDATE ON wiki_page_allocations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_page_allocations_delete BEFORE DELETE ON wiki_page_allocations BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_mutation_intents_insert BEFORE INSERT ON wiki_mutation_intents BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_mutation_intents_update BEFORE UPDATE ON wiki_mutation_intents BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

CREATE TRIGGER wiki_protocol_wiki_mutation_intents_delete BEFORE DELETE ON wiki_mutation_intents BEGIN
    SELECT CASE WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing')
        COLLATE rbit_wiki_writer_v1 <> 'permit' THEN RAISE(ABORT, 'wiki writer protocol refused') END;
END;

-- Logical store identity cannot be restamped under an activated writer owner.
CREATE TRIGGER wiki_protocol_store_identity_update BEFORE UPDATE ON wiki_store_identity
WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing') <> 'legacy' BEGIN
    SELECT RAISE(ABORT, 'managed wiki identity is immutable');
END;
CREATE TRIGGER wiki_protocol_store_identity_delete BEFORE DELETE ON wiki_store_identity
WHEN COALESCE((SELECT mode FROM wiki_writer_protocol WHERE singleton=1),'missing') <> 'legacy' BEGIN
    SELECT RAISE(ABORT, 'managed wiki identity is immutable');
END;

