//! Private in-memory store composition, not a per-request mode query or DB-owner
//! security. Only the actual accepted pool can mint active write ownership.
use crate::{StorageError, identity_upgrade::active::ActiveSqlPool};
#[derive(Clone, Copy)]
pub(crate) struct SqlProtocol {
    active: bool,
}
impl std::fmt::Debug for SqlProtocol {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("SqlProtocol").finish_non_exhaustive()
    }
}
impl SqlProtocol {
    pub(crate) const fn legacy() -> Self {
        Self { active: false }
    }
    pub(crate) fn accepted(_pool: &ActiveSqlPool) -> Self {
        Self { active: true }
    }
    pub(crate) fn require_legacy(self) -> Result<(), StorageError> {
        if self.active {
            return Err(StorageError::Corrupt("legacy credential mutation is refused on active identity stores".into()));
        }
        Ok(())
    }
}
