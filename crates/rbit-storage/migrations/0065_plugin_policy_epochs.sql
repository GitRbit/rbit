-- Bounded passive policy epochs with retained immutable audit.
-- Run inside sqlx's existing transaction with foreign_keys=ON. Never toggle it.
-- Deferred edges permit only an atomic retire/new-epoch/receipt/gate transition.
-- This schema supplies no executable authority and no pruning/reopening shortcut.

DROP TRIGGER plugin_policy_gate_delete;

DROP TRIGGER plugin_policy_gate_transition;

DROP TRIGGER plugin_policy_gate_epoch;

DROP TRIGGER plugin_review_history_immutable;

DROP TRIGGER plugin_policy_receipts_immutable;

DROP TRIGGER plugin_review_history_delete;

DROP TRIGGER plugin_policy_receipts_delete;

DROP TRIGGER plugin_review_heads_delete;

DROP TRIGGER plugin_selection_heads_delete;

DROP TRIGGER plugin_review_heads_advance;

DROP TRIGGER plugin_selection_heads_advance;

DROP INDEX plugin_policy_receipt_review_revision;

DROP INDEX plugin_policy_receipt_selection_revision;

ALTER TABLE plugin_policy_receipts RENAME TO plugin_policy_receipts_0064;

ALTER TABLE plugin_selection_heads RENAME TO plugin_selection_heads_0064;

ALTER TABLE plugin_review_heads RENAME TO plugin_review_heads_0064;

ALTER TABLE plugin_review_history RENAME TO plugin_review_history_0064;

ALTER TABLE plugin_policy_gate RENAME TO plugin_policy_gate_0064;

CREATE TABLE plugin_policy_epochs (
    epoch TEXT PRIMARY KEY CHECK (length(CAST(epoch AS BLOB))=64 AND epoch NOT GLOB '*[^0-9a-f]*'),
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal BETWEEN 0 AND 2),
    fingerprint TEXT NOT NULL CHECK (length(CAST(fingerprint AS BLOB))=64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    descriptor_version INTEGER NOT NULL CHECK (descriptor_version IN (1,2)),
    predecessor TEXT REFERENCES plugin_policy_epochs(epoch),
    created_by_renewal TEXT UNIQUE REFERENCES plugin_policy_renewals(id) DEFERRABLE INITIALLY DEFERRED,
    prepared_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    prepared_at INTEGER NOT NULL,
    retired INTEGER NOT NULL CHECK (retired IN (0,1)),
    retirement_operation TEXT UNIQUE REFERENCES plugin_policy_renewals(id) DEFERRABLE INITIALLY DEFERRED,
    retired_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    retired_at INTEGER,
    seal_reason INTEGER CHECK (seal_reason BETWEEN 0 AND 4),
    seal_actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    seal_operation TEXT UNIQUE CHECK (seal_operation IS NULL OR (length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64 AND seal_operation NOT GLOB '*[^a-z0-9-]*')),
    seal_request TEXT CHECK (seal_request IS NULL OR length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536),
    sealed_at INTEGER,
    UNIQUE(epoch,fingerprint),
    UNIQUE(epoch,retired),
    CHECK ((ordinal=0 AND predecessor IS NULL AND created_by_renewal IS NULL)
        OR (ordinal>0 AND predecessor IS NOT NULL AND created_by_renewal IS NOT NULL)),
    CHECK ((retired=0 AND retirement_operation IS NULL AND retired_by IS NULL AND retired_at IS NULL
        AND seal_reason IS NULL AND seal_actor IS NULL AND seal_operation IS NULL AND seal_request IS NULL AND sealed_at IS NULL)
        OR (retired=1 AND retirement_operation IS NOT NULL AND retired_at IS NOT NULL
        AND seal_reason IS NOT NULL AND seal_operation IS NOT NULL AND seal_request IS NOT NULL AND sealed_at IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX plugin_policy_one_active_epoch ON plugin_policy_epochs(retired) WHERE retired=0;
CREATE TABLE plugin_policy_renewals (
    id TEXT PRIMARY KEY CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 64 AND id NOT GLOB '*[^a-z0-9-]*'),
    request TEXT NOT NULL CHECK (length(CAST(request AS BLOB)) BETWEEN 1 AND 1024),
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    source_epoch TEXT NOT NULL UNIQUE,
    source_fingerprint TEXT NOT NULL,
    destination_epoch TEXT NOT NULL UNIQUE,
    destination_fingerprint TEXT NOT NULL,
    expected_seal_operation TEXT NOT NULL CHECK (length(CAST(expected_seal_operation AS BLOB)) BETWEEN 1 AND 64 AND expected_seal_operation NOT GLOB '*[^a-z0-9-]*'),
    created_at INTEGER NOT NULL,
    FOREIGN KEY(source_epoch,source_fingerprint) REFERENCES plugin_policy_epochs(epoch,fingerprint),
    FOREIGN KEY(destination_epoch,destination_fingerprint) REFERENCES plugin_policy_epochs(epoch,fingerprint),
    CHECK (source_epoch<>destination_epoch),
    CHECK (id<>expected_seal_operation)
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_policy_gate (
    id INTEGER PRIMARY KEY CHECK (id=1),
    state INTEGER NOT NULL CHECK (state BETWEEN 0 AND 2),
    epoch TEXT,
    fingerprint TEXT,
    prepared_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    prepared_at INTEGER,
    seal_reason INTEGER CHECK (seal_reason BETWEEN 0 AND 4),
    seal_actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    seal_operation TEXT CHECK (seal_operation IS NULL OR (length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64 AND seal_operation NOT GLOB '*[^a-z0-9-]*')),
    seal_request TEXT CHECK (seal_request IS NULL OR length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536),
    sealed_at INTEGER,
    active_marker INTEGER GENERATED ALWAYS AS (CASE WHEN epoch IS NULL THEN NULL ELSE 0 END) VIRTUAL,
    FOREIGN KEY(epoch,fingerprint) REFERENCES plugin_policy_epochs(epoch,fingerprint),
    FOREIGN KEY(epoch,active_marker) REFERENCES plugin_policy_epochs(epoch,retired) DEFERRABLE INITIALLY DEFERRED,
    CHECK ((state=0 AND epoch IS NULL AND fingerprint IS NULL AND prepared_by IS NULL AND prepared_at IS NULL)
        OR (state<>0 AND epoch IS NOT NULL AND fingerprint IS NOT NULL AND prepared_at IS NOT NULL)),
    CHECK ((state=2 AND seal_reason IS NOT NULL AND seal_operation IS NOT NULL AND seal_request IS NOT NULL AND sealed_at IS NOT NULL)
        OR (state<>2 AND seal_reason IS NULL AND seal_actor IS NULL AND seal_operation IS NULL AND seal_request IS NULL AND sealed_at IS NULL))
) STRICT;
CREATE TABLE plugin_review_history (
    digest TEXT NOT NULL CHECK (length(CAST(digest AS BLOB))=64 AND digest NOT GLOB '*[^0-9a-f]*'),
    revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 9223372036854775807),
    plugin_id TEXT NOT NULL CHECK (length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128),
    version TEXT NOT NULL CHECK (length(CAST(version AS BLOB)) BETWEEN 1 AND 128),
    intent INTEGER NOT NULL CHECK (intent BETWEEN 0 AND 3),
    request TEXT NOT NULL CHECK (length(CAST(request AS BLOB)) BETWEEN 1 AND 65536),
    catalog_version TEXT NOT NULL CHECK (catalog_version='empty-v1'),
    epoch TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(epoch,digest,revision),
    FOREIGN KEY(epoch,fingerprint) REFERENCES plugin_policy_epochs(epoch,fingerprint),
    FOREIGN KEY(plugin_id,version,digest) REFERENCES plugin_installations(plugin_id,version,digest)
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_review_heads (
    epoch TEXT NOT NULL,
    digest TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY(epoch,digest),
    FOREIGN KEY(epoch,digest,revision) REFERENCES plugin_review_history(epoch,digest,revision)
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_selection_heads (
    epoch TEXT NOT NULL REFERENCES plugin_policy_epochs(epoch),
    plugin_id TEXT NOT NULL CHECK (length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128),
    revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 9223372036854775807),
    version TEXT,
    digest TEXT,
    review_revision INTEGER,
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(epoch,plugin_id),
    FOREIGN KEY(plugin_id,version,digest) REFERENCES plugin_installations(plugin_id,version,digest),
    FOREIGN KEY(epoch,digest,review_revision) REFERENCES plugin_review_history(epoch,digest,revision),
    CHECK ((version IS NULL AND digest IS NULL AND review_revision IS NULL)
        OR (version IS NOT NULL AND digest IS NOT NULL AND review_revision IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_policy_receipts (
    id TEXT PRIMARY KEY CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 64 AND id NOT GLOB '*[^a-z0-9-]*'),
    kind INTEGER NOT NULL CHECK (kind BETWEEN 0 AND 2),
    request TEXT NOT NULL CHECK (length(CAST(request AS BLOB)) BETWEEN 1 AND 65536),
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    plugin_id TEXT NOT NULL CHECK (length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128),
    version TEXT,
    digest TEXT REFERENCES plugin_objects(digest),
    result_revision INTEGER NOT NULL CHECK (result_revision BETWEEN 1 AND 9223372036854775807),
    epoch TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    review_result_revision INTEGER GENERATED ALWAYS AS (CASE WHEN kind=0 THEN result_revision ELSE NULL END) VIRTUAL,
    FOREIGN KEY(epoch,fingerprint) REFERENCES plugin_policy_epochs(epoch,fingerprint),
    FOREIGN KEY(plugin_id,version,digest) REFERENCES plugin_installations(plugin_id,version,digest),
    FOREIGN KEY(epoch,digest,review_result_revision) REFERENCES plugin_review_history(epoch,digest,revision),
    CHECK ((kind IN (0,1) AND version IS NOT NULL AND digest IS NOT NULL) OR (kind=2 AND version IS NULL AND digest IS NULL))
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX plugin_policy_receipt_review_revision ON plugin_policy_receipts(epoch,digest,result_revision) WHERE kind=0;
CREATE UNIQUE INDEX plugin_policy_receipt_selection_revision ON plugin_policy_receipts(epoch,plugin_id,result_revision) WHERE kind IN (1,2);

-- Every legacy data column is copied unchanged; only epoch scope/version FK
-- columns are derived from the authoritative current gate/installation mapping.
INSERT INTO plugin_policy_epochs(epoch,ordinal,fingerprint,descriptor_version,prepared_by,prepared_at,retired)
SELECT epoch,0,fingerprint,1,prepared_by,prepared_at,0 FROM plugin_policy_gate_0064 WHERE state<>0;
INSERT INTO plugin_policy_gate(id,state,epoch,fingerprint,prepared_by,prepared_at,seal_reason,seal_actor,seal_operation,seal_request,sealed_at)
SELECT id,state,epoch,fingerprint,prepared_by,prepared_at,seal_reason,seal_actor,seal_operation,seal_request,sealed_at FROM plugin_policy_gate_0064;
INSERT INTO plugin_review_history(digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at)
SELECT digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at FROM plugin_review_history_0064;
INSERT INTO plugin_review_heads(epoch,digest,revision)
SELECT g.epoch,h.digest,h.revision FROM plugin_review_heads_0064 h CROSS JOIN plugin_policy_gate_0064 g;
INSERT INTO plugin_selection_heads(epoch,plugin_id,revision,version,digest,review_revision,actor,updated_at)
SELECT g.epoch,h.plugin_id,h.revision,h.version,h.digest,h.review_revision,h.actor,h.updated_at FROM plugin_selection_heads_0064 h CROSS JOIN plugin_policy_gate_0064 g;
INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,version,digest,result_revision,epoch,fingerprint,created_at)
SELECT r.id,r.kind,r.request,r.actor,r.plugin_id,i.version,r.digest,r.result_revision,r.epoch,r.fingerprint,r.created_at
FROM plugin_policy_receipts_0064 r LEFT JOIN plugin_installations i ON i.plugin_id=r.plugin_id AND i.digest=r.digest;
-- Valid pre-renewal data has at most one current epoch. Fail migration rather
-- than silently materializing missing singleton, oversized legacy ledgers or keys.
CREATE TABLE plugin_policy_0065_assert (ok INTEGER NOT NULL CHECK (ok=1)) STRICT;
INSERT INTO plugin_policy_0065_assert SELECT CASE WHEN
    (SELECT COUNT(*) FROM plugin_policy_gate)=1
    AND (SELECT COUNT(*) FROM plugin_review_history)<=4096
    AND (SELECT COUNT(*) FROM plugin_policy_receipts)<=4096
    AND (SELECT COUNT(*) FROM plugin_review_heads)<=1024
    AND (SELECT COUNT(*) FROM plugin_selection_heads)<=1024
    AND NOT EXISTS (SELECT 1 FROM plugin_policy_gate g JOIN plugin_policy_receipts r ON r.id=g.seal_operation)
    AND ((SELECT state FROM plugin_policy_gate)=0 AND (SELECT COUNT(*) FROM plugin_policy_epochs)=0
        AND (SELECT COUNT(*) FROM plugin_review_history)=0 AND (SELECT COUNT(*) FROM plugin_policy_receipts)=0
        AND (SELECT COUNT(*) FROM plugin_review_heads)=0 AND (SELECT COUNT(*) FROM plugin_selection_heads)=0
      OR (SELECT state FROM plugin_policy_gate)<>0 AND (SELECT COUNT(*) FROM plugin_policy_epochs)=1)
THEN 1 ELSE 0 END;
DROP TABLE plugin_policy_0065_assert;
-- Drop old CHILDREN before old history while FK enforcement stays ON.
DROP TABLE plugin_policy_receipts_0064;
DROP TABLE plugin_selection_heads_0064;
DROP TABLE plugin_review_heads_0064;
DROP TABLE plugin_review_history_0064;
DROP TABLE plugin_policy_gate_0064;

-- Registry controls: one bounded chain; new runtime bootstrap uses descriptor2.
CREATE TRIGGER plugin_policy_epochs_insert BEFORE INSERT ON plugin_policy_epochs
WHEN (SELECT COUNT(*) FROM plugin_policy_epochs)>=3
 OR (NEW.ordinal=0 AND (NEW.descriptor_version<>2 OR EXISTS(SELECT 1 FROM plugin_policy_epochs) OR NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=0)))
 OR (NEW.ordinal>0 AND (NEW.descriptor_version<>2 OR NEW.retired<>0 OR NOT EXISTS(
    SELECT 1 FROM plugin_policy_epochs p JOIN plugin_policy_gate g ON g.epoch=p.epoch
    WHERE p.epoch=NEW.predecessor AND p.retired=1 AND p.ordinal+1=NEW.ordinal AND g.state=2
    AND p.retirement_operation=NEW.created_by_renewal)))
BEGIN SELECT RAISE(ABORT,'invalid or exhausted policy epoch chain'); END;
CREATE TRIGGER plugin_policy_epochs_update BEFORE UPDATE ON plugin_policy_epochs
WHEN NOT (
    (OLD.epoch IS NEW.epoch AND OLD.ordinal IS NEW.ordinal AND OLD.fingerprint IS NEW.fingerprint
     AND OLD.descriptor_version IS NEW.descriptor_version AND OLD.predecessor IS NEW.predecessor
     AND OLD.created_by_renewal IS NEW.created_by_renewal AND OLD.prepared_by IS NEW.prepared_by
     AND OLD.prepared_at IS NEW.prepared_at AND OLD.retired=0 AND NEW.retired=1
     AND EXISTS(SELECT 1 FROM plugin_policy_gate g WHERE g.state=2 AND g.epoch=OLD.epoch
        AND g.fingerprint=OLD.fingerprint AND g.seal_reason=NEW.seal_reason AND g.seal_actor IS NEW.seal_actor
        AND g.seal_operation=NEW.seal_operation AND g.seal_request=NEW.seal_request AND g.sealed_at=NEW.sealed_at))
 OR (OLD.epoch IS NEW.epoch AND OLD.ordinal IS NEW.ordinal AND OLD.fingerprint IS NEW.fingerprint
     AND OLD.descriptor_version IS NEW.descriptor_version AND OLD.predecessor IS NEW.predecessor
     AND OLD.created_by_renewal IS NEW.created_by_renewal AND OLD.prepared_at IS NEW.prepared_at
     AND OLD.retired IS NEW.retired AND OLD.retirement_operation IS NEW.retirement_operation
     AND OLD.retired_at IS NEW.retired_at AND OLD.seal_reason IS NEW.seal_reason
     AND OLD.seal_operation IS NEW.seal_operation AND OLD.seal_request IS NEW.seal_request AND OLD.sealed_at IS NEW.sealed_at
     AND (OLD.prepared_by IS NEW.prepared_by OR NEW.prepared_by IS NULL)
     AND (OLD.retired_by IS NEW.retired_by OR NEW.retired_by IS NULL)
     AND (OLD.seal_actor IS NEW.seal_actor OR NEW.seal_actor IS NULL)))
BEGIN SELECT RAISE(ABORT,'policy epoch metadata is immutable'); END;

CREATE TRIGGER plugin_policy_epochs_delete BEFORE DELETE ON plugin_policy_epochs BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_policy_renewals_insert BEFORE INSERT ON plugin_policy_renewals
WHEN (SELECT COUNT(*) FROM plugin_policy_renewals)>=2
 OR EXISTS(SELECT 1 FROM plugin_policy_receipts WHERE id=NEW.id)
 OR EXISTS(SELECT 1 FROM plugin_policy_epochs WHERE seal_operation=NEW.id)
 OR EXISTS(SELECT 1 FROM plugin_policy_gate WHERE seal_operation=NEW.id)
 OR NOT EXISTS(SELECT 1 FROM plugin_policy_epochs s JOIN plugin_policy_epochs d ON d.predecessor=s.epoch
    JOIN plugin_policy_gate g ON g.epoch=s.epoch
    WHERE s.epoch=NEW.source_epoch AND s.fingerprint=NEW.source_fingerprint AND s.retired=1
    AND s.retirement_operation=NEW.id AND s.retired_by IS NEW.actor AND s.retired_at=NEW.created_at
    AND s.seal_operation=NEW.expected_seal_operation AND g.state=2 AND g.seal_operation=NEW.expected_seal_operation
    AND d.epoch=NEW.destination_epoch AND d.fingerprint=NEW.destination_fingerprint AND d.retired=0
    AND d.ordinal=s.ordinal+1 AND d.created_by_renewal=NEW.id AND d.prepared_by IS NEW.actor AND d.prepared_at=NEW.created_at)
BEGIN SELECT RAISE(ABORT,'invalid policy renewal or globally used operation key'); END;

CREATE TRIGGER plugin_policy_renewals_immutable BEFORE UPDATE ON plugin_policy_renewals
WHEN NOT (OLD.id IS NEW.id
    AND OLD.request IS NEW.request
    AND OLD.source_epoch IS NEW.source_epoch
    AND OLD.source_fingerprint IS NEW.source_fingerprint
    AND OLD.destination_epoch IS NEW.destination_epoch
    AND OLD.destination_fingerprint IS NEW.destination_fingerprint
    AND OLD.expected_seal_operation IS NEW.expected_seal_operation
    AND OLD.created_at IS NEW.created_at
    AND (OLD.actor IS NEW.actor OR NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'policy audit is immutable'); END;

CREATE TRIGGER plugin_policy_renewals_delete BEFORE DELETE ON plugin_policy_renewals BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_policy_gate_transition BEFORE UPDATE ON plugin_policy_gate
WHEN NOT (
    (OLD.state=0 AND NEW.state=1 AND EXISTS(SELECT 1 FROM plugin_policy_epochs e WHERE e.epoch=NEW.epoch
        AND e.ordinal=0 AND e.retired=0 AND e.fingerprint=NEW.fingerprint AND e.prepared_by IS NEW.prepared_by AND e.prepared_at=NEW.prepared_at))
 OR (OLD.state=1 AND NEW.state=2 AND OLD.epoch IS NEW.epoch AND OLD.fingerprint IS NEW.fingerprint
        AND OLD.prepared_by IS NEW.prepared_by AND OLD.prepared_at IS NEW.prepared_at
        AND NOT EXISTS(SELECT 1 FROM plugin_policy_receipts WHERE id=NEW.seal_operation)
        AND NOT EXISTS(SELECT 1 FROM plugin_policy_renewals WHERE id=NEW.seal_operation)
        AND NOT EXISTS(SELECT 1 FROM plugin_policy_epochs WHERE seal_operation=NEW.seal_operation))
 OR (OLD.state=2 AND NEW.state=1 AND OLD.epoch<>NEW.epoch AND EXISTS(
        SELECT 1 FROM plugin_policy_renewals r JOIN plugin_policy_epochs d ON d.epoch=r.destination_epoch
        WHERE r.source_epoch=OLD.epoch AND r.source_fingerprint=OLD.fingerprint
        AND r.expected_seal_operation=OLD.seal_operation AND r.destination_epoch=NEW.epoch
        AND r.destination_fingerprint=NEW.fingerprint AND d.retired=0
        AND d.prepared_by IS NEW.prepared_by AND d.prepared_at=NEW.prepared_at))
 OR (OLD.state IS NEW.state AND OLD.epoch IS NEW.epoch AND OLD.fingerprint IS NEW.fingerprint
        AND OLD.prepared_at IS NEW.prepared_at AND OLD.seal_reason IS NEW.seal_reason
        AND OLD.seal_operation IS NEW.seal_operation AND OLD.seal_request IS NEW.seal_request AND OLD.sealed_at IS NEW.sealed_at
        AND (OLD.prepared_by IS NEW.prepared_by OR NEW.prepared_by IS NULL)
        AND (OLD.seal_actor IS NEW.seal_actor OR NEW.seal_actor IS NULL)))
BEGIN SELECT RAISE(ABORT,'unaudited policy gate transition'); END;

CREATE TRIGGER plugin_policy_gate_delete BEFORE DELETE ON plugin_policy_gate BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_review_history_insert BEFORE INSERT ON plugin_review_history
WHEN NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch)
 OR (SELECT COUNT(*) FROM plugin_review_history WHERE epoch=NEW.epoch)>=4096
BEGIN SELECT RAISE(ABORT,'inactive epoch, capacity or globally used operation key'); END;

CREATE TRIGGER plugin_review_heads_insert BEFORE INSERT ON plugin_review_heads
WHEN NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch)
 OR (SELECT COUNT(*) FROM plugin_review_heads WHERE epoch=NEW.epoch)>=1024
BEGIN SELECT RAISE(ABORT,'inactive epoch, capacity or globally used operation key'); END;

CREATE TRIGGER plugin_selection_heads_insert BEFORE INSERT ON plugin_selection_heads
WHEN NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch)
 OR (SELECT COUNT(*) FROM plugin_selection_heads WHERE epoch=NEW.epoch)>=1024
BEGIN SELECT RAISE(ABORT,'inactive epoch, capacity or globally used operation key'); END;

CREATE TRIGGER plugin_policy_receipts_insert BEFORE INSERT ON plugin_policy_receipts
WHEN NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch)
 OR (SELECT COUNT(*) FROM plugin_policy_receipts WHERE epoch=NEW.epoch)>=4096
 OR EXISTS(SELECT 1 FROM plugin_policy_renewals WHERE id=NEW.id)
 OR EXISTS(SELECT 1 FROM plugin_policy_epochs WHERE seal_operation=NEW.id)
 OR EXISTS(SELECT 1 FROM plugin_policy_gate WHERE seal_operation=NEW.id)
BEGIN SELECT RAISE(ABORT,'inactive epoch, capacity or globally used operation key'); END;

CREATE TRIGGER plugin_review_history_immutable BEFORE UPDATE ON plugin_review_history
WHEN NOT (OLD.digest IS NEW.digest
    AND OLD.revision IS NEW.revision
    AND OLD.plugin_id IS NEW.plugin_id
    AND OLD.version IS NEW.version
    AND OLD.intent IS NEW.intent
    AND OLD.request IS NEW.request
    AND OLD.catalog_version IS NEW.catalog_version
    AND OLD.epoch IS NEW.epoch
    AND OLD.fingerprint IS NEW.fingerprint
    AND OLD.created_at IS NEW.created_at
    AND (OLD.actor IS NEW.actor OR NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'policy audit is immutable'); END;

CREATE TRIGGER plugin_policy_receipts_immutable BEFORE UPDATE ON plugin_policy_receipts
WHEN NOT (OLD.id IS NEW.id
    AND OLD.kind IS NEW.kind
    AND OLD.request IS NEW.request
    AND OLD.plugin_id IS NEW.plugin_id
    AND OLD.version IS NEW.version
    AND OLD.digest IS NEW.digest
    AND OLD.result_revision IS NEW.result_revision
    AND OLD.epoch IS NEW.epoch
    AND OLD.fingerprint IS NEW.fingerprint
    AND OLD.created_at IS NEW.created_at
    AND (OLD.actor IS NEW.actor OR NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'policy audit is immutable'); END;

CREATE TRIGGER plugin_review_history_delete BEFORE DELETE ON plugin_review_history BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_policy_receipts_delete BEFORE DELETE ON plugin_policy_receipts BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_review_heads_delete BEFORE DELETE ON plugin_review_heads BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_selection_heads_delete BEFORE DELETE ON plugin_selection_heads BEGIN SELECT RAISE(ABORT,'policy audit cannot be deleted'); END;

CREATE TRIGGER plugin_review_heads_advance BEFORE UPDATE ON plugin_review_heads
WHEN NEW.epoch IS NOT OLD.epoch OR NEW.digest IS NOT OLD.digest OR NEW.revision<=OLD.revision
 OR NOT EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch)
BEGIN SELECT RAISE(ABORT,'current epoch review revision must advance'); END;
CREATE TRIGGER plugin_selection_heads_advance BEFORE UPDATE ON plugin_selection_heads
WHEN NOT (
    (NEW.epoch IS OLD.epoch AND NEW.plugin_id IS OLD.plugin_id AND NEW.revision>OLD.revision
        AND EXISTS(SELECT 1 FROM plugin_policy_gate WHERE state=1 AND epoch=NEW.epoch))
 OR (NEW.epoch IS OLD.epoch AND NEW.plugin_id IS OLD.plugin_id AND NEW.revision IS OLD.revision
        AND NEW.version IS OLD.version AND NEW.digest IS OLD.digest AND NEW.review_revision IS OLD.review_revision
        AND NEW.updated_at IS OLD.updated_at AND (NEW.actor IS OLD.actor OR NEW.actor IS NULL)))
BEGIN SELECT RAISE(ABORT,'current epoch selection revision must advance'); END;
