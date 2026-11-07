//! Private in-memory store composition, not a per-request mode query or DB-owner
//! security. Only the actual accepted pool can mint active write ownership.
use crate::{StorageError, identity_upgrade::active::ActiveSqlPool};
use rbit_identity_models::sessions::PendingSessionRevocation;
#[derive(Clone, Copy)]
pub(crate) struct SqlProtocol {
    retired: Option<RetiredRange>,
}
#[derive(Clone, Copy)]
struct RetiredRange {
    through_id: i64,
    through_epoch: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntentRetirement {
    OutsideCutoff,
    Candidate,
    Invalid,
}
impl std::fmt::Debug for SqlProtocol {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SqlProtocol").finish_non_exhaustive()
    }
}
impl SqlProtocol {
    pub(crate) const fn legacy() -> Self {
        Self { retired: None }
    }
    pub(crate) fn accepted(pool: &ActiveSqlPool) -> Self {
        let checkpoint = pool.checkpoint();
        Self { retired: Some(RetiredRange { through_id: checkpoint.retired, through_epoch: checkpoint.floor }) }
    }
    pub(crate) fn require_legacy(self) -> Result<(), StorageError> {
        if self.retired.is_some() {
            return Err(StorageError::Corrupt("legacy credential mutation is refused on active identity stores".into()));
        }
        Ok(())
    }
    pub(crate) fn recovery_cursor(self, after: i64) -> Result<i64, StorageError> {
        match self.retired {
            None => Ok(after),
            Some(range) if after >= 0 => Ok(after.max(range.through_id)),
            Some(_) => Err(StorageError::Corrupt("credential recovery cursor is invalid".into())),
        }
    }
    pub(crate) fn retirement(self, pending: &PendingSessionRevocation) -> IntentRetirement {
        let Some(range) = self.retired else { return IntentRetirement::OutsideCutoff };
        if pending.id() > range.through_id {
            return IntentRetirement::OutsideCutoff;
        }
        if pending.target().get() > range.through_epoch {
            return IntentRetirement::Invalid;
        }
        IntentRetirement::Candidate
    }
}
