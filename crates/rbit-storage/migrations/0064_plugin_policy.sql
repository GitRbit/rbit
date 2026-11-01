-- Passive managed-package policy only. No executable authorization or activation.
CREATE UNIQUE INDEX plugin_installations_exact ON plugin_installations(plugin_id, version, digest);
CREATE TABLE plugin_policy_gate (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    state INTEGER NOT NULL CHECK (state BETWEEN 0 AND 2),
    epoch TEXT CHECK (epoch IS NULL OR (length(CAST(epoch AS BLOB)) = 64 AND epoch NOT GLOB '*[^0-9a-f]*')),
    fingerprint TEXT CHECK (fingerprint IS NULL OR (length(CAST(fingerprint AS BLOB)) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*')),
    prepared_by INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    prepared_at INTEGER,
    seal_reason INTEGER CHECK (seal_reason BETWEEN 0 AND 4),
    seal_actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    seal_operation TEXT CHECK (seal_operation IS NULL OR (length(CAST(seal_operation AS BLOB)) BETWEEN 1 AND 64 AND seal_operation NOT GLOB '*[^a-z0-9-]*')),
    seal_request TEXT CHECK (seal_request IS NULL OR length(CAST(seal_request AS BLOB)) BETWEEN 1 AND 65536),
    sealed_at INTEGER,
    CHECK ((state = 0 AND epoch IS NULL AND fingerprint IS NULL AND prepared_by IS NULL AND prepared_at IS NULL)
        OR (state <> 0 AND epoch IS NOT NULL AND fingerprint IS NOT NULL AND prepared_at IS NOT NULL)),
    CHECK ((state = 2 AND seal_reason IS NOT NULL AND seal_operation IS NOT NULL AND seal_request IS NOT NULL AND sealed_at IS NOT NULL)
        OR (state <> 2 AND seal_reason IS NULL AND seal_actor IS NULL AND seal_operation IS NULL AND seal_request IS NULL AND sealed_at IS NULL))
) STRICT;
INSERT INTO plugin_policy_gate(id,state) VALUES(1,0);
CREATE TABLE plugin_review_history (
    digest TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 9223372036854775807),
    plugin_id TEXT NOT NULL,
    version TEXT NOT NULL,
    intent INTEGER NOT NULL CHECK (intent BETWEEN 0 AND 3),
    request TEXT NOT NULL CHECK (length(CAST(request AS BLOB)) BETWEEN 1 AND 65536),
    catalog_version TEXT NOT NULL CHECK (catalog_version = 'empty-v1'),
    epoch TEXT NOT NULL CHECK (length(CAST(epoch AS BLOB)) = 64 AND epoch NOT GLOB '*[^0-9a-f]*'),
    fingerprint TEXT NOT NULL CHECK (length(CAST(fingerprint AS BLOB)) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(digest,revision),
    FOREIGN KEY(plugin_id,version,digest) REFERENCES plugin_installations(plugin_id,version,digest)
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_review_heads (
    digest TEXT PRIMARY KEY,
    revision INTEGER NOT NULL,
    FOREIGN KEY(digest,revision) REFERENCES plugin_review_history(digest,revision)
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_selection_heads (
    plugin_id TEXT PRIMARY KEY CHECK (length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128),
    revision INTEGER NOT NULL CHECK (revision BETWEEN 1 AND 9223372036854775807),
    version TEXT,
    digest TEXT,
    review_revision INTEGER,
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(plugin_id,version,digest) REFERENCES plugin_installations(plugin_id,version,digest),
    FOREIGN KEY(digest,review_revision) REFERENCES plugin_review_history(digest,revision),
    CHECK ((version IS NULL AND digest IS NULL AND review_revision IS NULL)
        OR (version IS NOT NULL AND digest IS NOT NULL AND review_revision IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE TABLE plugin_policy_receipts (
    id TEXT PRIMARY KEY CHECK (length(CAST(id AS BLOB)) BETWEEN 1 AND 64 AND id NOT GLOB '*[^a-z0-9-]*'),
    kind INTEGER NOT NULL CHECK (kind BETWEEN 0 AND 2),
    request TEXT NOT NULL CHECK (length(CAST(request AS BLOB)) BETWEEN 1 AND 65536),
    actor INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
    plugin_id TEXT NOT NULL CHECK (length(CAST(plugin_id AS BLOB)) BETWEEN 3 AND 128),
    digest TEXT REFERENCES plugin_objects(digest),
    result_revision INTEGER NOT NULL CHECK (result_revision BETWEEN 1 AND 9223372036854775807),
    epoch TEXT NOT NULL CHECK (length(CAST(epoch AS BLOB)) = 64 AND epoch NOT GLOB '*[^0-9a-f]*'),
    fingerprint TEXT NOT NULL CHECK (length(CAST(fingerprint AS BLOB)) = 64 AND fingerprint NOT GLOB '*[^0-9a-f]*'),
    created_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX plugin_policy_receipt_review_revision ON plugin_policy_receipts(digest,result_revision) WHERE kind=0;
CREATE UNIQUE INDEX plugin_policy_receipt_selection_revision ON plugin_policy_receipts(plugin_id,result_revision) WHERE kind IN (1,2);
CREATE TRIGGER plugin_policy_gate_delete BEFORE DELETE ON plugin_policy_gate BEGIN SELECT RAISE(ABORT,'policy gate cannot be deleted'); END;
CREATE TRIGGER plugin_policy_gate_transition BEFORE UPDATE ON plugin_policy_gate
WHEN NOT ((OLD.state = 0 AND NEW.state = 1) OR (OLD.state = 1 AND NEW.state = 2 AND OLD.prepared_by IS NEW.prepared_by)
    OR (OLD.state = NEW.state AND OLD.epoch IS NEW.epoch AND OLD.fingerprint IS NEW.fingerprint
        AND OLD.prepared_at IS NEW.prepared_at AND OLD.seal_reason IS NEW.seal_reason
        AND OLD.seal_operation IS NEW.seal_operation AND OLD.seal_request IS NEW.seal_request AND OLD.sealed_at IS NEW.sealed_at
        AND (OLD.prepared_by IS NEW.prepared_by OR NEW.prepared_by IS NULL)
        AND (OLD.seal_actor IS NEW.seal_actor OR NEW.seal_actor IS NULL)))
BEGIN SELECT RAISE(ABORT,'policy gate cannot reopen or change identity'); END;
CREATE TRIGGER plugin_policy_gate_epoch BEFORE UPDATE ON plugin_policy_gate
WHEN OLD.state <> 0 AND (OLD.epoch IS NOT NEW.epoch OR OLD.fingerprint IS NOT NEW.fingerprint OR OLD.prepared_at IS NOT NEW.prepared_at)
BEGIN SELECT RAISE(ABORT,'policy epoch is immutable'); END;
CREATE TRIGGER plugin_review_history_immutable BEFORE UPDATE ON plugin_review_history
WHEN NOT (OLD.digest IS NEW.digest AND OLD.revision IS NEW.revision AND OLD.plugin_id IS NEW.plugin_id AND OLD.version IS NEW.version
    AND OLD.intent IS NEW.intent AND OLD.request IS NEW.request AND OLD.catalog_version IS NEW.catalog_version
    AND OLD.epoch IS NEW.epoch AND OLD.fingerprint IS NEW.fingerprint AND OLD.created_at IS NEW.created_at
    AND (OLD.actor IS NEW.actor OR NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'policy review history is immutable'); END;
CREATE TRIGGER plugin_policy_receipts_immutable BEFORE UPDATE ON plugin_policy_receipts
WHEN NOT (OLD.id IS NEW.id AND OLD.kind IS NEW.kind AND OLD.request IS NEW.request AND OLD.plugin_id IS NEW.plugin_id
    AND OLD.digest IS NEW.digest AND OLD.result_revision IS NEW.result_revision AND OLD.epoch IS NEW.epoch
    AND OLD.fingerprint IS NEW.fingerprint AND OLD.created_at IS NEW.created_at AND (OLD.actor IS NEW.actor OR NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'policy receipts are immutable'); END;
CREATE TRIGGER plugin_review_history_delete BEFORE DELETE ON plugin_review_history BEGIN SELECT RAISE(ABORT,'policy history cannot be deleted'); END;
CREATE TRIGGER plugin_policy_receipts_delete BEFORE DELETE ON plugin_policy_receipts BEGIN SELECT RAISE(ABORT,'policy receipts cannot be deleted'); END;
CREATE TRIGGER plugin_review_heads_delete BEFORE DELETE ON plugin_review_heads BEGIN SELECT RAISE(ABORT,'review revision cannot reset'); END;
CREATE TRIGGER plugin_selection_heads_delete BEFORE DELETE ON plugin_selection_heads BEGIN SELECT RAISE(ABORT,'selection revision cannot reset'); END;
CREATE TRIGGER plugin_review_heads_advance BEFORE UPDATE ON plugin_review_heads
WHEN NEW.digest <> OLD.digest OR NEW.revision <= OLD.revision BEGIN SELECT RAISE(ABORT,'review revision must advance'); END;
CREATE TRIGGER plugin_selection_heads_advance BEFORE UPDATE ON plugin_selection_heads
WHEN NOT ((NEW.plugin_id = OLD.plugin_id AND NEW.revision > OLD.revision)
    OR (NEW.plugin_id IS OLD.plugin_id AND NEW.revision IS OLD.revision AND NEW.version IS OLD.version
        AND NEW.digest IS OLD.digest AND NEW.review_revision IS OLD.review_revision AND NEW.updated_at IS OLD.updated_at
        AND NEW.actor IS NULL))
BEGIN SELECT RAISE(ABORT,'selection revision must advance'); END;
