//! Which waiting job a runner is given: the order of the queue.
//!
//! A run that has started keeps its place. Its next jobs (the ones its `needs` just
//! released, or a job run again) are handed out before any job of a run that has not
//! started, the run that started first going first. Runs that have not started wait in
//! the order their jobs were queued. So a run a runner has begun is finished rather than
//! left behind every run pushed after it, and newer runs start when a runner has nothing
//! left to do for the ones in progress.
//!
//! A cancelled run counts as in progress even when no runner took any of it: what is left
//! of it are the jobs that run whatever happened (`if: always()`), and until they are done
//! the run is not finished, cannot be run again, and holds its concurrency group.
use super::ClaimRequest;
use crate::StorageError;
use crate::sqlite::actions_rows::{labels_json, runner_columns};
use sqlx::SqliteConnection;

/// A waiting job the runner may take: the repository has Actions on, the runner's scope
/// covers it, and the job asks for labels, all of which the runner has. `$from` chooses
/// which table to walk first, `$more` narrows the jobs, and `$order` sorts them.
macro_rules! candidate {
    ($from:literal, $more:literal, $order:literal) => {
        concat!(
            "SELECT j.id FROM ",
            $from,
            "JOIN repositories r ON r.id = j.repo_id ",
            "JOIN actions_repositories a ON a.repo_id = j.repo_id AND a.enabled = 1 ",
            "WHERE j.status = 5 ",
            $more,
            "AND CASE ?1 ",
            "WHEN 'instance' THEN ?4 = 1 ",
            "OR EXISTS (SELECT 1 FROM actions_owner_policies p WHERE p.owner_id = r.owner_id AND p.instance_runners = 1) ",
            "WHEN 'owner' THEN r.owner_id = ?2 ",
            "ELSE j.repo_id = ?3 END ",
            "AND EXISTS (SELECT 1 FROM actions_job_labels l WHERE l.job_id = j.id) ",
            "AND NOT EXISTS (SELECT 1 FROM actions_job_labels l WHERE l.job_id = j.id ",
            "AND l.label NOT IN (SELECT value FROM json_each(?5))) ",
            "AND (run.trusted = 1 OR NOT EXISTS (SELECT 1 FROM actions_job_labels l WHERE l.job_id = j.id ",
            "AND l.label IN (SELECT value FROM json_each(?6)))) ",
            "AND (j.max_parallel IS NULL OR (SELECT COUNT(*) FROM actions_jobs s ",
            "WHERE s.run_id = j.run_id AND s.job_key = j.job_key AND s.status = 6) < j.max_parallel) ",
            "AND (j.job_group IS NULL OR NOT EXISTS (SELECT 1 FROM actions_jobs g ",
            "WHERE g.repo_id = j.repo_id AND g.job_group = j.job_group AND g.status = 6)) ",
            "AND (j.environment IS NULL OR NOT EXISTS (SELECT 1 FROM actions_environments e ",
            "WHERE e.repo_id = j.repo_id AND e.name = j.environment AND e.wait_timer > 0 ",
            "AND j.queued_at + e.wait_timer * 60 > ?7)) ",
            "ORDER BY ",
            $order,
            " LIMIT 1"
        )
    };
}

/// The next job of the runs in progress (started, or cancelled): the run that started
/// first, then its jobs in the order they were queued. A cancelled run that never started
/// takes its place by when its job was queued. Start from the partial index of unfinished
/// runs in progress so a backlog of new runs does not lengthen this scan.
const IN_PROGRESS: &str = candidate!(
    "actions_runs run INDEXED BY actions_runs_claim_priority \
     CROSS JOIN actions_jobs j INDEXED BY actions_jobs_by_run ON j.run_id = run.id ",
    "AND run.finished_at IS NULL AND (run.started_at IS NOT NULL OR run.cancel_requested = 1) ",
    "COALESCE(run.started_at, j.queued_at), run.id, j.queued_at, j.id"
);

/// The oldest waiting job, walked in queue order from the index of waiting jobs. It is
/// asked for only when no run in progress has a job for the runner, so what it finds
/// belongs to a run that has not started.
const QUEUED: &str = candidate!("actions_jobs j CROSS JOIN actions_runs run ON run.id = j.run_id ", "", "j.queued_at, j.id");

/// The waiting job to hand the runner of `request`, if there is one.
pub(super) async fn candidate(tx: &mut SqliteConnection, request: &ClaimRequest<'_>) -> Result<Option<i64>, StorageError> {
    let (kind, owner, repo) = runner_columns(request.scope);
    let labels = labels_json(request.labels)?;
    let host_labels = labels_json(request.host_labels)?;
    for query in [IN_PROGRESS, QUEUED] {
        let found: Option<i64> = sqlx::query_scalar(query)
            .bind(kind)
            .bind(owner)
            .bind(repo)
            .bind(request.instance_runners_for_all)
            .bind(&labels)
            .bind(&host_labels)
            // Actions: environments. A wait timer holds a job until it is over.
            .bind(request.now.0)
            .fetch_optional(&mut *tx)
            .await?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn plan(query: &'static str) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::Database::open(&dir.path().join("rbit.db")).await.unwrap();
        sqlx::query("ANALYZE").execute(&database.pool).await.unwrap();
        let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {query}")))
            .bind("repository")
            .bind(None::<i64>)
            .bind(1)
            .bind(false)
            .bind("[\"linux\"]")
            .bind("[]")
            .bind(0)
            .fetch_all(&database.pool)
            .await
            .unwrap();
        plan.into_iter().map(|row| row.3).collect()
    }

    /// The queue must be walked from the partial index of waiting jobs, in queue order,
    /// rather than by scanning every job ever run.
    #[tokio::test]
    async fn the_queue_walks_the_waiting_index_in_order() {
        let plan = plan(QUEUED).await;
        assert!(plan.iter().any(|step| step.contains("USING INDEX actions_jobs_waiting")), "{plan:#?}");
        assert!(!plan.iter().any(|step| step.contains("TEMP B-TREE FOR ORDER BY")), "{plan:#?}");
    }

    /// The priority pass visits only unfinished runs in progress, then looks up their
    /// jobs by run ID. It must never walk the backlog of waiting jobs.
    #[tokio::test]
    async fn the_priority_pass_walks_unfinished_runs_then_looks_up_their_jobs() {
        let plan = plan(IN_PROGRESS).await;
        assert!(plan.iter().any(|step| step.starts_with("SCAN run USING INDEX actions_runs_claim_priority")), "{plan:#?}");
        assert!(plan.iter().any(|step| step.starts_with("SEARCH j USING INDEX actions_jobs_by_run")), "{plan:#?}");
    }
}
