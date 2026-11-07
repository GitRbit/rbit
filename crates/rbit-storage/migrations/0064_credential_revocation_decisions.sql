-- Dormant durable decision for exact revocation replay. No current route activates it.
-- Historical rows stay explicitly unsupported legacy decisions, never implicit keeps.
-- Empty until an explicit accepted-upgrade/prototype composition binds the stores.
-- This datum alone proves neither startup mode nor consistent rollback history.
CREATE TABLE credential_session_pair (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    identity BLOB NOT NULL CHECK(typeof(identity)='blob' AND length(identity)=16)
) STRICT;
CREATE TRIGGER credential_pair_writer_insert BEFORE INSERT ON credential_session_pair BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT,'credential writer version refused') END;
END;
CREATE TRIGGER credential_pair_writer_update BEFORE UPDATE ON credential_session_pair BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT,'credential writer version refused') END;
END;
CREATE TRIGGER credential_pair_writer_delete BEFORE DELETE ON credential_session_pair BEGIN
    SELECT CASE WHEN 'v1' COLLATE rbit_credential_writer_v1 <> 'v1' THEN RAISE(ABORT,'credential writer version refused') END;
END;
CREATE TRIGGER credential_pair_identity_immutable BEFORE UPDATE ON credential_session_pair
WHEN NEW.singleton IS NOT OLD.singleton OR NEW.identity IS NOT OLD.identity BEGIN
    SELECT RAISE(ABORT,'credential session pair identity is immutable');
END;
CREATE TRIGGER credential_pair_identity_retained BEFORE DELETE ON credential_session_pair BEGIN
    SELECT RAISE(ABORT,'credential session pair identity is immutable');
END;

ALTER TABLE credential_revocations ADD COLUMN decision TEXT NOT NULL DEFAULT 'legacy'
    CHECK (decision IN ('legacy', 'end_all', 'keep_current'));
ALTER TABLE credential_revocations ADD COLUMN keep_spec BLOB
    CHECK ((decision = 'keep_current' AND typeof(keep_spec) = 'blob' AND length(keep_spec) = 64)
        OR (decision IN ('legacy', 'end_all') AND keep_spec IS NULL))
    CHECK (decision = 'legacy' OR outcome IS NULL OR outcome = 'ended_all'
        OR (decision = 'keep_current' AND outcome = 'kept_current'));
ALTER TABLE credential_revocations ADD COLUMN superseded_by_epoch INTEGER
    CHECK (superseded_by_epoch IS NULL
        OR (typeof(superseded_by_epoch) = 'integer' AND superseded_by_epoch > target_epoch));
ALTER TABLE credential_revocations ADD COLUMN cleanup_pair BLOB
    CHECK(cleanup_pair IS NULL OR (typeof(cleanup_pair)='blob' AND length(cleanup_pair)=16
        AND completed_at IS NOT NULL AND decision<>'legacy'));
CREATE INDEX credential_recovery_pending ON credential_revocations(id)
    WHERE completed_at IS NULL AND superseded_by_epoch IS NULL;

CREATE TRIGGER credential_revocation_decision_immutable BEFORE UPDATE OF decision, keep_spec ON credential_revocations
WHEN NEW.decision IS NOT OLD.decision OR NEW.keep_spec IS NOT OLD.keep_spec BEGIN
    SELECT RAISE(ABORT, 'credential revocation decision is immutable');
END;
CREATE TRIGGER credential_revocation_identity_immutable BEFORE UPDATE ON credential_revocations
WHEN NEW.id IS NOT OLD.id OR NEW.account_id IS NOT OLD.account_id
    OR NEW.target_epoch IS NOT OLD.target_epoch OR NEW.created_at IS NOT OLD.created_at BEGIN
    SELECT RAISE(ABORT, 'credential revocation identity is immutable');
END;
CREATE TRIGGER credential_revocation_supersession_immutable BEFORE UPDATE OF superseded_by_epoch ON credential_revocations
WHEN OLD.superseded_by_epoch IS NOT NULL AND NEW.superseded_by_epoch IS NOT OLD.superseded_by_epoch BEGIN
    SELECT RAISE(ABORT, 'credential revocation supersession is immutable');
END;
-- Keep the completed current witness until older unresolved rows are superseded.
-- Deleting the owning login still cascades normally: its authority row is already gone.
CREATE TRIGGER credential_revocation_cleanup_witness BEFORE DELETE ON credential_revocations
WHEN OLD.decision <> 'legacy' AND OLD.completed_at IS NOT NULL
    AND EXISTS (SELECT 1 FROM credential_session_pair pair WHERE pair.singleton=1 AND OLD.cleanup_pair=pair.identity)
    AND EXISTS (SELECT 1 FROM user_logins l WHERE l.account_id = OLD.account_id AND l.authentication_epoch = OLD.target_epoch)
    AND EXISTS (SELECT 1 FROM credential_revocations r WHERE r.account_id = OLD.account_id
        AND r.target_epoch < OLD.target_epoch AND r.completed_at IS NULL AND r.superseded_by_epoch IS NULL) BEGIN
    SELECT RAISE(ABORT, 'credential cleanup witness is still required');
END;
CREATE TRIGGER credential_revocation_supersession_witness BEFORE UPDATE OF superseded_by_epoch ON credential_revocations
WHEN NEW.superseded_by_epoch IS NOT NULL AND OLD.superseded_by_epoch IS NULL
    AND (OLD.completed_at IS NOT NULL OR NOT EXISTS (
        SELECT 1 FROM user_logins l JOIN credential_revocations witness
            ON witness.account_id = l.account_id AND witness.target_epoch = l.authentication_epoch
        JOIN credential_session_pair pair ON pair.singleton=1 AND witness.cleanup_pair=pair.identity
        WHERE l.account_id = OLD.account_id AND l.authentication_epoch = NEW.superseded_by_epoch
            AND witness.id <> OLD.id AND witness.decision <> 'legacy'
            AND witness.completed_at IS NOT NULL AND witness.outcome IS NOT NULL
            AND witness.superseded_by_epoch IS NULL)) BEGIN
    SELECT RAISE(ABORT, 'newer committed credential cleanup is required');
END;
CREATE TRIGGER credential_revocation_completion_immutable BEFORE UPDATE OF outcome, completed_at, cleanup_pair ON credential_revocations
WHEN (OLD.completed_at IS NOT NULL AND (NEW.outcome IS NOT OLD.outcome OR NEW.completed_at IS NOT OLD.completed_at OR NEW.cleanup_pair IS NOT OLD.cleanup_pair))
    OR (OLD.superseded_by_epoch IS NOT NULL AND (NEW.outcome IS NOT OLD.outcome OR NEW.completed_at IS NOT OLD.completed_at OR NEW.cleanup_pair IS NOT OLD.cleanup_pair)) BEGIN
    SELECT RAISE(ABORT, 'resolved credential revocation is immutable');
END;
CREATE TRIGGER credential_revocation_unresolved_retained BEFORE DELETE ON credential_revocations
WHEN OLD.completed_at IS NULL AND OLD.superseded_by_epoch IS NULL
    AND EXISTS (SELECT 1 FROM user_logins WHERE account_id=OLD.account_id) BEGIN
    SELECT RAISE(ABORT, 'unresolved credential revocation is retained');
END;
