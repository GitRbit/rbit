//! Where a waiting job stands in line: the jobs with the same labels handed out before it, and
//! how long such jobs have recently taken, for an expected start.
use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::actions::JobId;

/// The sorted label set of job `$id` as one string: jobs with equal sets compete for the
/// same runners.
macro_rules! labels_of {
    ($id:literal) => {
        concat!(
            "(SELECT group_concat(label, char(31)) FROM (SELECT label FROM actions_job_labels WHERE job_id = ",
            $id,
            " ORDER BY label))"
        )
    };
}

/// When job `$job` was queued.
macro_rules! queued {
    ($job:literal) => {
        concat!("COALESCE(", $job, ".queued_at, ", $job, ".created_at)")
    };
}

/// Whether run `$run` is in progress for the queue: started, or cancelled.
macro_rules! in_progress {
    ($run:literal) => {
        concat!("(", $run, ".started_at IS NOT NULL OR ", $run, ".cancel_requested = 1)")
    };
}

/// Waiting jobs with the labels of waiting job `?1` that a runner is given before it
/// (`claim_order`): those of runs in progress, the run that started first going first, then
/// those of runs that have not started, in the order they were queued.
const AHEAD: &str = concat!(
    "SELECT COUNT(*) FROM actions_jobs me JOIN actions_runs mine ON mine.id = me.run_id ",
    "JOIN actions_jobs j ON j.status = 5 JOIN actions_runs theirs ON theirs.id = j.run_id ",
    "WHERE me.id = ?1 AND me.status = 5 AND CASE ",
    "WHEN NOT ",
    in_progress!("theirs"),
    " THEN NOT ",
    in_progress!("mine"),
    " AND (",
    queued!("j"),
    ", j.id) < (",
    queued!("me"),
    ", me.id) WHEN NOT ",
    in_progress!("mine"),
    " THEN 1 ELSE (COALESCE(theirs.started_at, ",
    queued!("j"),
    "), theirs.id, ",
    queued!("j"),
    ", j.id) < (COALESCE(mine.started_at, ",
    queued!("me"),
    "), mine.id, ",
    queued!("me"),
    ", me.id) END AND ",
    labels_of!("j.id"),
    " IS ",
    labels_of!("?1")
);

/// The mean run time of the last 50 finished jobs with the labels of job `?1`.
const MEAN: &str = concat!(
    "SELECT AVG(finished_at - started_at) FROM (SELECT j.finished_at, j.started_at FROM actions_jobs j ",
    "WHERE j.status IN (1, 2) AND j.started_at IS NOT NULL AND j.finished_at IS NOT NULL AND ",
    labels_of!("j.id"),
    " IS ",
    labels_of!("?1"),
    " ORDER BY j.finished_at DESC LIMIT 50)"
);

/// A waiting job's place in line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuePlace {
    /// Waiting jobs with the same labels that a runner is given before it.
    pub ahead: u64,
    /// The mean run time, in seconds, of the last 50 finished jobs with the same labels;
    /// `None` without any.
    pub mean_seconds: Option<u64>,
}

impl ActionsRunStore {
    /// Where waiting job `job` stands; `None` when it is not waiting.
    pub async fn queue_place(&self, job: JobId) -> Result<Option<QueuePlace>, StorageError> {
        let waiting: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM actions_jobs WHERE id = ? AND status = 5").bind(job.0).fetch_optional(&self.pool).await?;
        if waiting.is_none() {
            return Ok(None);
        }
        let ahead: i64 = sqlx::query_scalar(AHEAD).bind(job.0).fetch_one(&self.pool).await?;
        let mean: Option<f64> = sqlx::query_scalar(MEAN).bind(job.0).fetch_one(&self.pool).await?;
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a mean of non-negative whole seconds")]
        let mean_seconds = mean.filter(|mean| mean.is_finite() && *mean >= 0.0).map(|mean| mean.round() as u64);
        Ok(Some(QueuePlace { ahead: u64::try_from(ahead).unwrap_or_default(), mean_seconds }))
    }
}
