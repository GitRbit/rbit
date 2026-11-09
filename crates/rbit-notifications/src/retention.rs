//! Read-notification retention: the cutoff policy and its single store deletion.
use crate::port::DeliveryRepository;
use rbit_contracts::Timestamp;

const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// The instant `days` before `now`; read rows last updated before it are pruned.
/// Saturates rather than wrapping for extreme clocks or retention settings.
pub fn cutoff(now: Timestamp, days: u32) -> Timestamp {
    Timestamp(now.0.saturating_sub(i64::from(days).saturating_mul(SECONDS_PER_DAY)))
}

/// Deletes read rows last updated before `cutoff`; unread rows are never pruned.
pub async fn prune_read<R: DeliveryRepository>(repository: &R, cutoff: Timestamp) -> Result<u64, R::Error> {
    repository.prune_read(cutoff).await
}
