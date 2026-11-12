//! Test reports, failure fingerprints and flaky tests (agent plan steps 3.4 to 3.6).
//!
//! * An artifact named `rbit-test-report` is read once it is stored: its `JUnit` files are
//!   parsed ([`rbit_actions::reports`]) and the counts, first failed tests and the ids of the
//!   first passed tests are kept for the attempt that uploaded it. A report that cannot be
//!   read is logged and ignored: the artifact itself is kept like any other.
//! * Each failed job has faults: one per failed test of its report, else one for the job
//!   (the step that ended it and its first error). A fault's fingerprint is compared with
//!   the previous finished run of the same workflow and ref (`new` or `recurring`, and the
//!   faults that are gone are `resolved`), and with the newest finished run on the default
//!   branch (`also_failing_on_base`).
//! * A failed test that also passed at the same commit, in another attempt or run of a job
//!   of the same name (that job passed, or its report lists the test as passed), is flaky.

use std::collections::BTreeMap;

use rbit_actions::reports::{TestReport, fingerprint, is_report_artifact, read_report};
use rbit_core::{
    RepoId,
    actions::{Artifact, Job, Run, RunId, Status, Step, TaskId, Termination},
};
use rbit_storage::sqlite::actions_runs::{StoredReport, StoredTest};

use super::logs::{pick_step, termination_of};
use crate::{ActionsService, ServiceError};

/// One thing that failed in a job, as compared across runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub fingerprint: String,
    /// `<job>: <test or step>`, for people.
    pub label: String,
}

/// How a run's faults compare with the runs before it.
#[derive(Debug, Clone, Default)]
pub struct Comparison {
    /// The previous finished run of the same workflow and ref, by fingerprint.
    pub previous: Option<BTreeMap<String, String>>,
    /// The newest finished run of the same workflow on the default branch, by fingerprint.
    pub base: Option<BTreeMap<String, String>>,
}

impl Comparison {
    /// `recurring` when the previous run had any of `faults`, else `new`.
    #[must_use]
    pub fn seen(&self, faults: &[Fault]) -> &'static str {
        let recurring = self.previous.as_ref().is_some_and(|previous| faults.iter().any(|fault| previous.contains_key(&fault.fingerprint)));
        if recurring { "recurring" } else { "new" }
    }

    /// Whether the default branch's newest run had any of `faults`.
    #[must_use]
    pub fn on_base(&self, faults: &[Fault]) -> bool {
        self.base.as_ref().is_some_and(|base| faults.iter().any(|fault| base.contains_key(&fault.fingerprint)))
    }
}

/// The faults of failed job `job_name`: one per failed test of `report`, else one for the
/// step `step` that ended it, said by its first error or else its termination.
#[must_use]
pub fn job_faults(
    job_name: &str,
    step: Option<&str>,
    termination: Termination,
    first_error: Option<&str>,
    report: Option<&StoredReport>,
) -> Vec<Fault> {
    if let Some(report) = report.filter(|report| !report.items.is_empty()) {
        return report
            .items
            .iter()
            .map(|test| Fault { fingerprint: fingerprint(job_name, &test.id, &test.message), label: format!("{job_name}: {}", test.id) })
            .collect();
    }
    let what = step.map_or_else(|| termination.reason.to_string(), str::to_owned);
    let message = first_error.map_or_else(|| termination.reason.to_string(), str::to_owned);
    vec![Fault { fingerprint: fingerprint(job_name, &what, &message), label: format!("{job_name}: {what}") }]
}

/// A parsed report as stored.
fn stored(report: TestReport) -> StoredReport {
    let items =
        report.items.into_iter().map(|test| StoredTest { id: test.id, path: test.path, line: test.line, message: test.message }).collect();
    StoredReport { total: report.total, failed: report.failed, items, passed: report.passed }
}

impl ActionsService {
    /// Reads and keeps the test report artifact `artifact` uploaded by task `task`; any other
    /// artifact is left alone. Never fails the upload.
    pub(crate) async fn take_test_report(&self, task: TaskId, artifact: &Artifact) {
        if !is_report_artifact(&artifact.name) {
            return;
        }
        let path = self.artifact_path(artifact);
        let parsed = tokio::task::spawn_blocking(move || std::fs::File::open(path).map_err(Into::into).and_then(read_report)).await;
        let report = match parsed {
            Ok(Ok(report)) => report,
            Ok(Err(error)) => {
                tracing::warn!(%error, task = task.0, "a test report was not read");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, task = task.0, "reading a test report stopped");
                return;
            }
        };
        let (total, failed) = (report.total, report.failed);
        match self.ctx().database.actions_runs().record_test_report(task, &stored(report)).await {
            Ok(()) => tracing::info!(task = task.0, total, failed, "test report kept"),
            Err(error) => tracing::warn!(%error, task = task.0, "a test report was not kept"),
        }
    }

    /// The faults of run `run`'s failed jobs, label by fingerprint.
    async fn run_faults(&self, run: RunId) -> Result<BTreeMap<String, String>, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let jobs: Vec<Job> =
            store.jobs(run).await?.into_iter().filter(|job| job.status == Status::Failure && job.task_id.is_some()).collect();
        let (ends, steps) = (self.task_ends(&jobs).await?, self.steps(&jobs).await?);
        let tasks: Vec<TaskId> = jobs.iter().filter_map(|job| job.task_id).collect();
        let (annotations, reports) = (store.annotations_of_tasks(&tasks).await?, store.test_reports(&tasks).await?);
        let mut faults = BTreeMap::new();
        for job in &jobs {
            let Some(task) = job.task_id else { continue };
            let termination = termination_of(job, ends.get(&task));
            let own: Vec<&Step> = steps.iter().filter(|step| step.task_id == task).collect();
            let step = pick_step(&own, termination).map(|step| step.name.as_str());
            let error = annotations.iter().find(|annotation| annotation.task == task && annotation.level == "error");
            let text = error.map(|annotation| rbit_actions::logs::annotation_text(&annotation.text));
            let found = job_faults(&job.name, step, termination, text.as_ref().map(|text| text.message), reports.get(&task));
            faults.extend(found.into_iter().map(|fault| (fault.fingerprint, fault.label)));
        }
        Ok(faults)
    }

    /// How the faults of `run` (of `repo`, whose default branch is `default_branch`) compare
    /// with the runs before it.
    pub(crate) async fn compare_faults(&self, repo: RepoId, run: &Run, default_branch: &str) -> Result<Comparison, ServiceError> {
        let store = self.ctx().database.actions_runs();
        let previous = store.previous_run(repo, &run.workflow_path, &run.reference, run.id).await?;
        let base_ref = format!("refs/heads/{default_branch}");
        let base = store.previous_run(repo, &run.workflow_path, &base_ref, run.id).await?;
        let previous = match previous {
            Some(id) => Some(self.run_faults(id).await?),
            None => None,
        };
        let base = match base {
            Some(id) => Some(self.run_faults(id).await?),
            None => None,
        };
        Ok(Comparison { previous, base })
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::actions::TerminationReason;

    use super::*;

    #[test]
    fn a_job_has_a_fault_per_failed_test_else_one_for_its_step() {
        let ended = Termination { reason: TerminationReason::StepFailed, step: Some(3), exit_code: Some(101) };
        let one = job_faults("Tests", Some("Run tests"), ended, Some("error[E0308]: mismatched types"), None);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].label, "Tests: Run tests");
        let test = StoredTest { id: "a b".into(), path: None, line: None, message: "boom at 12".into() };
        let report = StoredReport { total: 3, failed: 1, items: vec![test], passed: Vec::new() };
        let tests = job_faults("Tests", Some("Run tests"), ended, None, Some(&report));
        assert_eq!(tests[0].label, "Tests: a b");
        let previous = BTreeMap::from([(tests[0].fingerprint.clone(), tests[0].label.clone())]);
        let comparison = Comparison { previous: Some(previous), base: None };
        assert_eq!((comparison.seen(&tests), comparison.seen(&one), comparison.on_base(&tests)), ("recurring", "new", false));
    }
}
