//! Fixed SQL for bounded recovery; no PHC, credential or identity text projection.
macro_rules! candidate {
    ($tail:literal)=>{concat!(
"SELECT id,account_id,target_epoch, \
    CASE decision WHEN 'legacy' THEN 0 WHEN 'end_all' THEN 1 WHEN 'keep_current' THEN 2 ELSE 3 END AS decision_code, \
    CASE WHEN keep_spec IS NULL THEN 0 WHEN typeof(keep_spec)='blob' THEN CASE WHEN length(keep_spec)=64 THEN 1 ELSE 2 END ELSE 2 END AS keep_shape, \
    CASE WHEN typeof(keep_spec)='blob' THEN CASE WHEN length(keep_spec)=64 THEN keep_spec END END AS keep_spec \
    FROM credential_revocations", $tail)};
}
pub(super) const PENDING: &str = candidate!(" WHERE id>? AND completed_at IS NULL AND superseded_by_epoch IS NULL ORDER BY id LIMIT ?");
pub(super) const FIND: &str = candidate!(" WHERE id=? AND account_id=? AND target_epoch=?");
pub(super) const STATE: &str = "SELECT p.completed_at,p.superseded_by_epoch,l.authentication_epoch AS current_epoch, \
    c.id AS current_id, CASE c.decision WHEN 'end_all' THEN 1 WHEN 'keep_current' THEN 2 ELSE 0 END AS current_decision, \
    CASE c.outcome WHEN 'ended_all' THEN 1 WHEN 'kept_current' THEN 2 ELSE 0 END AS current_outcome, \
    c.completed_at AS current_completed,c.superseded_by_epoch AS current_superseded, \
    CASE WHEN pair.identity IS NOT NULL AND p.cleanup_pair=pair.identity \
        AND (SELECT count(*) FROM (SELECT 1 FROM credential_session_pair LIMIT 2))=1 THEN 1 ELSE 0 END AS own_pair_matches, \
    CASE WHEN pair.identity IS NOT NULL AND c.cleanup_pair=pair.identity \
        AND (SELECT count(*) FROM (SELECT 1 FROM credential_session_pair LIMIT 2))=1 THEN 1 ELSE 0 END AS current_pair_matches \
    FROM credential_revocations p LEFT JOIN user_logins l ON l.account_id=p.account_id \
    LEFT JOIN credential_revocations c ON c.account_id=l.account_id AND c.target_epoch=l.authentication_epoch \
    LEFT JOIN credential_session_pair pair ON pair.singleton=1 \
    WHERE p.id=?1 AND p.account_id=?2 AND p.target_epoch=?3";
pub(super) const ACK: &str = "UPDATE credential_revocations SET outcome=?6,completed_at=?7,cleanup_pair=?8 \
    WHERE id=?1 AND account_id=?2 AND target_epoch=?3 AND decision=?4 AND keep_spec IS ?5 \
    AND completed_at IS NULL AND superseded_by_epoch IS NULL \
    AND EXISTS (SELECT 1 FROM user_logins WHERE account_id=?2 AND authentication_epoch=?3) \
    AND EXISTS (SELECT 1 FROM credential_session_pair WHERE singleton=1 AND identity=?8 \
        AND (SELECT count(*) FROM (SELECT 1 FROM credential_session_pair LIMIT 2))=1)";
pub(super) const SUPERSEDE: &str = "UPDATE credential_revocations SET superseded_by_epoch=?4 \
    WHERE id=?1 AND account_id=?2 AND target_epoch=?3 AND completed_at IS NULL AND superseded_by_epoch IS NULL \
    AND EXISTS (SELECT 1 FROM user_logins l JOIN credential_revocations w \
        ON w.account_id=l.account_id AND w.target_epoch=l.authentication_epoch \
        JOIN credential_session_pair pair ON pair.singleton=1 AND w.cleanup_pair=pair.identity \
        WHERE l.account_id=?2 AND l.authentication_epoch=?4 AND w.id=?5 AND w.decision<>'legacy' \
        AND w.completed_at IS NOT NULL AND w.outcome IS NOT NULL AND w.superseded_by_epoch IS NULL)";
