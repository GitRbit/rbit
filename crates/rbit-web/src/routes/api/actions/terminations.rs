//! Termination records in the jobs API (rbit's own fields beside GitHub's): how each job's
//! task ended, so an agent reads "timed out at step 6" or "runner lost, log incomplete"
//! instead of a bare `failure`.
//!
//! A job gains `termination`: `{reason, step, step_name, exit_code, log_complete}` once its
//! task has ended. A task without a record reads by its status: `completed`, `cancelled`,
//! or `unknown` for a failure nobody explained. A job that ran out of time reads
//! `conclusion: timed_out`, as on GitHub.

use std::collections::BTreeMap;

use rbit_core::actions::{Job, Status, TaskId, Termination, TerminationReason};
use rbit_storage::sqlite::actions_runs::{QueuePlace, TaskEnd};
use serde_json::{Value, json};

/// Adds `job`'s termination record to `item`, its JSON, once the job has ended.
pub fn annotate(item: &mut Value, job: &Job, ends: &BTreeMap<TaskId, TaskEnd>) {
    let Some(end) = job.task_id.and_then(|task| ends.get(&task)) else { return };
    let reason = match (end.termination, job.status) {
        (Some(termination), _) => termination.reason,
        (None, Status::Success) => TerminationReason::Completed,
        (None, Status::Cancelled) => TerminationReason::Cancelled,
        (None, Status::Failure) => TerminationReason::Unknown,
        // Still running or waiting: nothing has ended yet.
        (None, _) => return,
    };
    let termination = end.termination.unwrap_or(Termination::because(reason));
    let step = termination.step.map(|index| i64::from(index) + 1).and_then(|number| {
        let steps = item["steps"].as_array()?;
        let name = steps.iter().find(|candidate| candidate["number"] == number)?["name"].as_str()?.to_owned();
        Some((number, name))
    });
    describe(item, termination, step.as_ref().map(|(number, name)| (*number, name.as_str())), end.log_complete);
    if reason == TerminationReason::Timeout {
        item["conclusion"] = json!("timed_out");
    }
}

// Agent loop: queue position.
/// Adds `queue` to a waiting job's JSON: how many jobs with the same labels are ahead of it
/// and, from how long such jobs recently took, when it may start (one runner assumed).
pub fn queue(item: &mut Value, place: Option<&QueuePlace>) {
    if let Some(place) = place {
        item["queue"] = json!({
            "position": place.ahead + 1,
            "ahead": place.ahead,
            "expected_start_seconds": place.mean_seconds.map(|mean| mean.saturating_mul(place.ahead)),
        });
    }
}

/// Sets `item["termination"]` to `termination`, naming `step` (its number and name) when
/// the record names one.
pub fn describe(item: &mut Value, termination: Termination, step: Option<(i64, &str)>, log_complete: bool) {
    let named = termination.step.and(step);
    item["termination"] = json!({
        "reason": termination.reason.as_str(),
        "step": named.map(|(number, _)| number),
        "step_name": named.map(|(_, name)| name),
        "exit_code": termination.exit_code,
        "log_complete": log_complete,
    });
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        RepoId, Timestamp,
        actions::{JobId, RunId, Termination},
    };

    use super::*;

    fn job(status: Status, task: Option<i64>) -> Job {
        Job {
            id: JobId(1),
            run_id: RunId(1),
            repo_id: RepoId(1),
            key: "test".to_owned(),
            name: "test".to_owned(),
            matrix: None,
            needs: Vec::new(),
            runs_on: Vec::new(),
            permissions: None,
            status,
            attempt: 1,
            task_id: task.map(TaskId),
            runner_id: None,
            created_at: Timestamp(0),
            queued_at: None,
            started_at: None,
            finished_at: None,
            environment: None,
            environment_url: None,
            awaiting_review: false,
        }
    }

    #[test]
    fn a_record_names_the_step_and_a_timeout_reads_timed_out() {
        let ends = BTreeMap::from([(
            TaskId(7),
            TaskEnd {
                termination: Some(Termination { reason: TerminationReason::Timeout, step: Some(1), exit_code: None }),
                log_complete: true,
            },
        )]);
        let mut item =
            json!({ "conclusion": "failure", "steps": [{ "number": 1, "name": "Set up" }, { "number": 2, "name": "Run shard" }] });
        annotate(&mut item, &job(Status::Failure, Some(7)), &ends);
        assert_eq!(item["conclusion"], "timed_out");
        assert_eq!(
            item["termination"],
            json!({ "reason": "timeout", "step": 2, "step_name": "Run shard", "exit_code": null, "log_complete": true })
        );
    }

    #[test]
    fn without_a_record_the_status_speaks_and_a_running_job_has_none() {
        let ends = BTreeMap::from([(TaskId(7), TaskEnd { termination: None, log_complete: false })]);
        for (status, reason) in [(Status::Success, "completed"), (Status::Cancelled, "cancelled"), (Status::Failure, "unknown")] {
            let mut item = json!({ "steps": [] });
            annotate(&mut item, &job(status, Some(7)), &ends);
            assert_eq!(item["termination"]["reason"], reason);
            assert_eq!(item["termination"]["log_complete"], false);
        }
        let mut running = json!({});
        annotate(&mut running, &job(Status::Running, Some(7)), &ends);
        assert!(running.get("termination").is_none());
        let mut queued = json!({});
        annotate(&mut queued, &job(Status::Waiting, None), &ends);
        assert!(queued.get("termination").is_none());
    }
}
