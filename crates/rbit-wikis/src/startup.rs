//! Bounded startup projections. These are data, never integrity/readiness receipts.
pub mod objects;
pub mod scratch;
use crate::{WikiId, policy::WikiHead};
use rbit_contracts::RepoId;

pub const MAX_STARTUP_SCOPES_PER_BATCH: u16 = 256;

/// Caller explicitly selects a bounded batch; there is no hidden policy default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiStartupBatchSize(u16);
impl WikiStartupBatchSize {
    pub const fn new(value: u16) -> Option<Self> {
        if value > 0 && value <= MAX_STARTUP_SCOPES_PER_BATCH { Some(Self(value)) } else { None }
    }
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// SQL scope metadata only. Orphan intents remain distinct from a reused numeric
/// repository ID; no current repository/wiki, permission or native object is adopted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiStartupScope {
    pub wiki: WikiId,
    pub repository: RepoId,
    pub enabled: bool,
    pub head: Option<WikiHead>,
    pub pending: bool,
    pub orphaned: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_batch_size_has_no_zero_or_unbounded_configuration() {
        assert_eq!(WikiStartupBatchSize::new(0), None);
        assert_eq!(WikiStartupBatchSize::new(1).unwrap().get(), 1);
        assert_eq!(WikiStartupBatchSize::new(256).unwrap().get(), 256);
        assert_eq!(WikiStartupBatchSize::new(257), None);
        assert_eq!(WikiStartupBatchSize::new(u16::MAX), None);
    }
}
