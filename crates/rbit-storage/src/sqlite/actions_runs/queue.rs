//! Where a waiting job stands in line: the jobs with the same labels queued before it, and
//! how long such jobs have recently taken, for an expected start.
use super::ActionsRunStore;
use crate::StorageError;
use rbit_core::actions::JobId;

/// The sorted label set of job `?1` as one string: jobs with equal sets compete for the
/// same runners.
const LABELS_OF: &str = "(SELECT group_concat(label, char(31)) FROM (SELECT label FROM actions_job_labels WHERE job_id = {} ORDER BY label))";

/// A waiting job's place in line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuePlace {
    /// Waiting jobs with the same labels queued before it.
    pub ahead: u64,
    /// The mean run time, in seconds, of the last 50 finished jobs with the same labels;
    /// `None` without any.
    pub mean_seconds: Option<u64>,
}

impl ActionsRunStore {
    /// Where waiting job `job` stands; `None` when it is not waiting.
    pub async fn queue_place(&self, job: JobId) -> Result<Option<QueuePlace>, StorageError> {
        let mine = LABELS_OF.replace("{}", "?1");
        let theirs = LABELS_OF.replace("{}", "j.id");
        let ahead_sql = format!(
            "SELECT COUNT(*) FROM actions_jobs j, actions_jobs me WHERE me.id = ?1 AND me.status = 5 AND j.status = 5 \
             AND (COALESCE(j.queued_at, j.created_at) < COALESCE(me.queued_at, me.created_at) \
                  OR (COALESCE(j.queued_at, j.created_at) = COALESCE(me.queued_at, me.created_at) AND j.id < me.id)) \
             AND {theirs} IS {mine}"
        );
        let waiting: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM actions_jobs WHERE id = ? AND status = 5").bind(job.0).fetch_optional(&self.pool).await?;
        if waiting.is_none() {
            return Ok(None);
        }
        let ahead: i64 = sqlx::query_scalar(&ahead_sql).bind(job.0).fetch_one(&self.pool).await?;
        let mean_sql = format!(
            "SELECT AVG(finished_at - started_at) FROM (SELECT j.finished_at, j.started_at FROM actions_jobs j \
             WHERE j.status IN (1, 2) AND j.started_at IS NOT NULL AND j.finished_at IS NOT NULL AND {theirs} IS {mine} \
             ORDER BY j.finished_at DESC LIMIT 50)"
        );
        let mean: Option<f64> = sqlx::query_scalar(&mean_sql).bind(job.0).fetch_one(&self.pool).await?;
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a mean of non-negative whole seconds")]
        let mean_seconds = mean.filter(|mean| mean.is_finite() && *mean >= 0.0).map(|mean| mean.round() as u64);
        Ok(Some(QueuePlace { ahead: u64::try_from(ahead).unwrap_or_default(), mean_seconds }))
    }
}
