//! Admission caps, concurrency groups, release order and task leases.
use std::{
    future::{Future, ready},
    pin::pin,
    task::{Context, Poll, Waker},
};

use super::leases::{Refusal, TaskLeases, leased, running};
use super::*;

#[test]
fn an_event_takes_runs_and_jobs_from_its_allowance() {
    let mut allowance = Allowance { runs: 1, jobs: 3 };
    assert!(allowance.take_run());
    assert!(!allowance.take_run(), "the second run of a one-run event");
    assert_eq!(check_jobs("ci.yml", 2, &mut allowance, 3), Ok(()));
    assert_eq!(allowance.jobs, 1);
    assert_eq!(
        check_jobs("ci.yml", 2, &mut allowance, 3),
        Err("ci.yml: this event already started its share of jobs; one event may start at most 3".to_owned())
    );
    assert_eq!(allowance.jobs, 1, "a refused run takes nothing");
}

#[test]
fn labels_and_the_waiting_cap_name_the_problem() {
    assert_eq!(check_labels("ci.yml", ["linux", "x64"]), Ok(()));
    assert_eq!(check_labels("ci.yml", ["linux", ""]), Err("ci.yml: `runs-on` label `` must be 1 to 255 characters".to_owned()));
    let long = "é".repeat(256);
    let error = check_labels("ci.yml", [long.as_str()]).unwrap_err();
    assert!(error.starts_with(&format!("ci.yml: `runs-on` label `{}`", "é".repeat(64))), "{error}");
    assert!(check_labels("ci.yml", ["é".repeat(255).as_str()]).is_ok(), "characters, not bytes");
    assert_eq!(check_waiting("ci.yml", 8, 2, 10), Ok(()));
    assert_eq!(
        check_waiting("ci.yml", 9, 2, 10),
        Err("ci.yml: the repository already has 9 jobs waiting for a runner; at most 10 may wait".to_owned())
    );
}

#[test]
fn groups_cancel_or_hold_as_github_does() {
    use Member::{Active, AwaitingApproval, Held};
    let others = [(1, Active), (2, Held), (3, AwaitingApproval), (4, Held)];
    assert_eq!(enter_group(&others, None, false), Entry { cancel: vec![2, 4], start: Start::HeldForConcurrency });
    assert_eq!(enter_group(&others, None, true), Entry { cancel: vec![1, 2, 3, 4], start: Start::Runnable });
    assert_eq!(enter_group(&others, Some(1), false), Entry { cancel: vec![2, 4], start: Start::Runnable }, "a run never holds itself");
    assert_eq!(enter_group(&[(3, AwaitingApproval)], None, false), Entry { cancel: vec![], start: Start::Runnable });
    assert_eq!(enter_group::<i64>(&[], None, false), Entry { cancel: vec![], start: Start::Runnable });
    // A run still ending its tasks holds the group, even for `cancel-in-progress`, and is not
    // cancelled again.
    for cancel in [false, true] {
        assert_eq!(enter_group(&[(5, Member::Draining)], None, cancel), Entry { cancel: vec![], start: Start::HeldForConcurrency });
    }
}

#[test]
fn the_oldest_held_run_starts_once_nothing_is_active() {
    use Member::{Active, AwaitingApproval, Held};
    assert_eq!(next_to_release(&[(1, AwaitingApproval), (2, Held), (3, Held)]), Some(2));
    assert_eq!(next_to_release(&[(1, Held), (2, Active)]), None);
    assert_eq!(next_to_release(&[(1, Member::Draining), (2, Held)]), None, "a cancelled run's tasks are still ending");
    assert_eq!(next_to_release(&[(1, AwaitingApproval)]), None);
    assert_eq!(next_to_release::<i64>(&[]), None);
}

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

/// Task 1 of runner 7 runs job 10, whose current attempt is task `current`.
struct Store {
    current: Option<i64>,
    done: bool,
    job: bool,
}

#[derive(Debug, PartialEq)]
struct Task;

impl TaskLeases for Store {
    type Task = Task;
    type Job = Option<i64>;
    type Error = ();

    fn task(&self, id: i64) -> impl Future<Output = Result<Option<Task>, ()>> + Send {
        ready(Ok((id == 1).then_some(Task)))
    }
    fn job_of(&self, _: &Task) -> impl Future<Output = Result<Option<Option<i64>>, ()>> + Send {
        ready(Ok(self.job.then_some(self.current)))
    }
    fn runner_of(&self, _: &Task) -> Option<i64> {
        Some(7)
    }
    fn is_done(&self, _: &Task) -> bool {
        self.done
    }
    fn task_id(&self, _: &Task) -> i64 {
        1
    }
    fn current_task(&self, job: &Option<i64>) -> Option<i64> {
        *job
    }
}

#[test]
fn a_lease_needs_the_runner_a_running_task_and_the_current_attempt() {
    let held = Store { current: Some(1), done: false, job: true };
    assert!(matches!(run(leased(&held, 7, 1)), Ok(Ok((Task, Some(1))))));
    assert_eq!(run(running(&held, 7, 2)), Ok(Err(Refusal::NoTask)));
    assert_eq!(run(running(&held, 8, 1)), Ok(Err(Refusal::OtherRunner)));
    let ended = Store { done: true, ..held };
    assert_eq!(run(running(&ended, 7, 1)), Ok(Err(Refusal::Ended)));
    let gone = Store { current: Some(1), done: false, job: false };
    assert!(matches!(run(leased(&gone, 7, 1)), Ok(Err(Refusal::NoJob))));
    let rerun = Store { current: Some(2), done: false, job: true };
    assert!(matches!(run(leased(&rerun, 7, 1)), Ok(Err(Refusal::Rerun))));
    assert_eq!(Refusal::Rerun.message(), "the job has been rerun; this task no longer belongs to it");
}
