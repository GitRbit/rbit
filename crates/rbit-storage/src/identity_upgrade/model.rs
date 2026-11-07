//! Fixed stopped checkpoint data. None of these parsed bytes are runtime authority.
use crate::StorageError;
use std::num::{NonZeroU32, NonZeroU64};
#[derive(Clone, Copy, Debug)]
pub struct IdentityUpgradeLimits {
    pub max_record_bytes: NonZeroU32,
    pub max_entries: NonZeroU64,
}
#[derive(Debug, thiserror::Error)]
pub enum IdentityUpgradeError {
    #[error("identity upgrade storage failure: {0}")]
    Storage(#[from] StorageError),
    #[error("stopped identity upgrade is unsupported on this platform")]
    Unsupported,
    #[error("stopped operation requires an active runtime")]
    NoRuntime,
    #[error("stopped operation ended abnormally; retain the stores and reconcile explicitly")]
    Abnormal,
    #[error("identity scan is incomplete; increase explicit offline limits after reviewing the stores")]
    IncompleteScan,
    #[error("identity stores or checkpoints do not match; preserve the stopped pair and reconcile offline")]
    Mismatch,
    #[error("identity state is malformed or unknown; preserve both stores, never reset markers or counters")]
    Invalid,
    #[error("credential epoch space is exhausted; preserve both stores")]
    Exhausted,
    #[error("identity pair is not prepared; the complete activation bundle is still required")]
    NotPrepared,
    #[error("legacy access is refused after identity preparation; use explicit stopped recovery")]
    LegacyModeRequired,
}
impl From<sqlx::Error> for IdentityUpgradeError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error.into())
    }
}
impl From<tokio::task::JoinError> for IdentityUpgradeError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Storage(error.into())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    pub generation: [u8; 16],
    pub pair: [u8; 16],
    pub floor: i64,
    pub retired: i64,
    pub accounts: i64,
}
impl Checkpoint {
    pub fn final_floor(self) -> Result<i64, IdentityUpgradeError> {
        self.floor.checked_add(self.accounts).filter(|n| *n < i64::MAX).ok_or(IdentityUpgradeError::Exhausted)
    }
    pub fn encode(self) -> [u8; 64] {
        let mut b = [0; 64];
        b[0] = 2;
        b[8..24].copy_from_slice(&self.generation);
        b[24..40].copy_from_slice(&self.pair);
        b[40..48].copy_from_slice(&self.floor.to_be_bytes());
        b[48..56].copy_from_slice(&self.retired.to_be_bytes());
        b[56..64].copy_from_slice(&self.accounts.to_be_bytes());
        b
    }
    pub fn decode(b: &[u8; 64]) -> Result<Self, IdentityUpgradeError> {
        if b[0] != 2 || b[1..8] != [0; 7] {
            return Err(IdentityUpgradeError::Invalid);
        }
        let get = |start| i64::from_be_bytes(b[start..start + 8].try_into().expect("fixed checkpoint slice"));
        let state = Self {
            generation: b[8..24].try_into().expect("fixed generation"),
            pair: b[24..40].try_into().expect("fixed pair"),
            floor: get(40),
            retired: get(48),
            accounts: get(56),
        };
        if state.generation == [0; 16] || state.floor < 0 || state.retired < 0 || state.accounts < 0 {
            return Err(IdentityUpgradeError::Invalid);
        }
        state.final_floor()?;
        Ok(state)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IdentityUpgradeSummary {
    pub observed_epoch_floor: i64,
    pub retired_intents_through: i64,
    pub rebased_accounts: i64,
    pub final_epoch_floor: i64,
}
impl From<Checkpoint> for IdentityUpgradeSummary {
    fn from(c: Checkpoint) -> Self {
        Self {
            observed_epoch_floor: c.floor,
            retired_intents_through: c.retired,
            rebased_accounts: c.accounts,
            final_epoch_floor: c.floor + c.accounts,
        }
    }
}
