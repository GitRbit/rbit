//! The delivery worker's bookkeeping: draining due deliveries in bounded batches and
//! settling each attempt. Spawning, the concurrency permits, shutdown and the HTTP
//! request itself are runtime work that composition owns.
use super::policy::{NextStep, next_step};
use rbit_contracts::Timestamp;
use std::future::Future;

/// What one attempt reported: whether the receiver answered 2xx, whether retrying cannot
/// help (the target is refused or invalid), and the record to keep in the delivery log.
#[derive(Debug)]
pub struct AttemptReport<R> {
    pub succeeded: bool,
    pub permanent: bool,
    pub record: R,
}

/// The delivery queue as the worker sees it, bound by composition.
pub trait DeliveryQueue: Sync {
    type Error: Send;
    type Claimed: Send;
    type Record: Send;
    /// Claims up to `limit` deliveries due at `now`.
    fn claim_due(&self, now: Timestamp, limit: u32) -> impl Future<Output = Result<Vec<Self::Claimed>, Self::Error>> + Send;
    /// Attempts every claimed delivery (concurrently, as composition sees fit) and
    /// settles each; returns once all have finished.
    fn run_all(&self, claimed: Vec<Self::Claimed>) -> impl Future<Output = ()> + Send;
    /// Stores the attempt and what becomes of the delivery.
    fn record(&self, claimed: &Self::Claimed, record: Self::Record, step: NextStep) -> impl Future<Output = ()> + Send;
    /// Counts and logs the attempt under `step.label()`, before it is recorded.
    fn observe(&self, claimed: &Self::Claimed, record: &Self::Record, step: NextStep);
}

/// Attempts every delivery due at `now`, retries included, in batches of `batch` (at
/// least one), until none is left; returns how many attempts were made.
pub async fn deliver_due<Q: DeliveryQueue>(queue: &Q, now: Timestamp, batch: u32) -> Result<usize, Q::Error> {
    let mut attempts = 0;
    loop {
        let claimed = queue.claim_due(now, batch.max(1)).await?;
        if claimed.is_empty() {
            return Ok(attempts);
        }
        attempts += claimed.len();
        queue.run_all(claimed).await;
    }
}

/// Settles one attempt of `claimed`, which had `previous_attempts` before it: success,
/// a retry at a jittered time, or failure once attempts run out or retrying cannot help.
pub async fn settle<Q: DeliveryQueue>(
    queue: &Q,
    claimed: &Q::Claimed,
    previous_attempts: u32,
    max_attempts: u32,
    report: AttemptReport<Q::Record>,
    now: impl FnOnce() -> Timestamp,
    random: impl FnOnce() -> f64,
) -> NextStep {
    let step = next_step(report.succeeded, report.permanent, previous_attempts, max_attempts, now, random);
    queue.observe(claimed, &report.record, step);
    queue.record(claimed, report.record, step).await;
    step
}
