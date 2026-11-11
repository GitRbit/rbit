//! How a task ended, as the runner saw it: the termination record sent with the final state
//! call (`TaskTermination`), so the server can tell a failed step from a timeout from a job
//! that never got going.
//!
//! The job tells the [`Ending`] as it goes: the exit code of each step's process, the first
//! step that failed (one `continue-on-error` let pass does not count), and whether the
//! job's timeout fired. The reason follows from that and the task's result: `completed`,
//! `step_failed` (with the step and its exit code), `timeout`, `cancelled`, or
//! `setup_failed` for a failure before or outside any step.

use std::sync::Mutex;

use rbit_workflow::protocol::{StepStatus, TaskResult, TaskTermination};

use crate::{job::Job, steps::Outcome};

/// What the job has told so far.
#[derive(Debug, Default)]
pub struct Ending {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    /// The exit code of the running step's process, once it exited.
    exit_code: Option<i32>,
    /// The first step that failed the job, with its exit code.
    failed: Option<(u32, Option<i32>)>,
    timed_out: bool,
}

impl Ending {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A step starts: no exit code yet.
    pub fn step_started(&self) {
        self.state().exit_code = None;
    }

    /// The running step's process exited with `code`.
    pub fn exited(&self, code: i32) {
        self.state().exit_code = Some(code);
    }

    /// Step `index` ended with `conclusion` (after `continue-on-error`): a failure is the
    /// job's, and only the first one counts.
    pub fn step_ended(&self, index: usize, conclusion: StepStatus) {
        if conclusion == StepStatus::Failure {
            self.step_failed(index);
        }
    }

    fn step_failed(&self, index: usize) {
        let mut state = self.state();
        if state.failed.is_none() {
            let code = state.exit_code;
            state.failed = Some((u32::try_from(index).unwrap_or(u32::MAX), code));
        }
    }

    /// The job's `timeout-minutes` ran out.
    pub fn timed_out(&self) {
        self.state().timed_out = true;
    }

    /// The record for a task that ended with `result`.
    pub fn report(&self, result: TaskResult) -> TaskTermination {
        let state = self.state();
        let (reason, failed) = match result {
            TaskResult::Success => ("completed", None),
            _ if state.timed_out => ("timeout", state.failed),
            TaskResult::Cancelled => ("cancelled", None),
            TaskResult::Failure => match state.failed {
                Some(failed) => ("step_failed", Some(failed)),
                None => ("setup_failed", None),
            },
        };
        TaskTermination { reason: reason.to_owned(), step: failed.map(|(step, _)| step), exit_code: failed.and_then(|(_, code)| code) }
    }
}

/// A step whose process exited with `code` (not 0): the job's failure, with the code kept.
pub fn exited(job: &Job, code: i32) -> Outcome {
    job.reporter.ending.exited(code);
    job.fail(&format!("Process completed with exit code {code}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_failed_step_and_its_exit_code_are_reported() {
        let ending = Ending::default();
        ending.step_started();
        ending.exited(101);
        ending.step_failed(2);
        ending.step_started();
        ending.exited(1);
        ending.step_failed(3);
        let report = ending.report(TaskResult::Failure);
        assert_eq!((report.reason.as_str(), report.step, report.exit_code), ("step_failed", Some(2), Some(101)));
    }

    #[test]
    fn a_failure_outside_the_steps_is_a_setup_failure() {
        let report = Ending::default().report(TaskResult::Failure);
        assert_eq!((report.reason.as_str(), report.step), ("setup_failed", None));
    }

    #[test]
    fn a_timeout_wins_over_the_cancellation_it_caused_and_success_is_completed() {
        let ending = Ending::default();
        ending.timed_out();
        assert_eq!(ending.report(TaskResult::Failure).reason, "timeout");
        assert_eq!(ending.report(TaskResult::Cancelled).reason, "timeout");
        assert_eq!(Ending::default().report(TaskResult::Cancelled).reason, "cancelled");
        assert_eq!(Ending::default().report(TaskResult::Success).reason, "completed");
    }

    #[test]
    fn a_step_killed_by_a_signal_has_no_exit_code() {
        let ending = Ending::default();
        ending.step_started();
        ending.step_failed(0);
        assert_eq!(ending.report(TaskResult::Failure).exit_code, None);
    }
}
