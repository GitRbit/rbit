//! The sweep's task passes over a scripted, immediately ready store.
#![allow(clippy::unused_async_trait_impl)]
use rbit_actions::stale_tasks::{StaleSwept, StaleTaskStore, StaleTasks, Timeouts, before};
use std::{
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("scripted ports are always ready"),
    }
}

const TIMEOUTS: Timeouts = Timeouts { zombie_seconds: 60, endless_seconds: 3_600, abandoned_seconds: 86_400 };

/// Silent tasks, long-running tasks, abandoned runs; tasks that are gone or no longer running.
#[derive(Debug, Default)]
struct Store {
    calls: Mutex<Vec<String>>,
    silent: Vec<i64>,
    endless: Vec<i64>,
    abandoned: Vec<i64>,
    gone: Vec<i64>,
    stopped: Vec<i64>,
}

impl Store {
    fn call(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl StaleTaskStore for Store {
    type Error = ();
    type TaskId = i64;
    type Task = i64;
    type Settled = String;
    async fn silent(&self, before: i64) -> Result<Vec<i64>, ()> {
        self.call(format!("silent before {before}"));
        Ok(self.silent.clone())
    }
    async fn long_running(&self, before: i64) -> Result<Vec<i64>, ()> {
        self.call(format!("long running before {before}"));
        Ok(self.endless.clone())
    }
    async fn cancel_abandoned(&self, before: i64, now: i64) -> Result<Vec<String>, ()> {
        self.call(format!("cancel abandoned before {before} at {now}"));
        Ok(self.abandoned.iter().map(|run| format!("run {run}")).collect())
    }
    async fn task(&self, id: i64) -> Result<Option<i64>, ()> {
        Ok((!self.gone.contains(&id)).then_some(id))
    }
    async fn note(&self, id: i64, reason: &str) -> Result<(), ()> {
        self.call(format!("note {id}: {reason}"));
        Ok(())
    }
    async fn fail(&self, id: i64, _: i64) -> Result<Option<String>, ()> {
        self.call(format!("fail {id}"));
        Ok((!self.stopped.contains(&id)).then(|| format!("task {id}")))
    }
    async fn close(&self, id: i64, _: &str) -> Result<(), ()> {
        self.call(format!("close {id}"));
        Ok(())
    }
    async fn settle(&self, settled: &String) -> Result<(), ()> {
        self.call(format!("settle {settled}"));
        Ok(())
    }
    async fn retire_ephemeral_runner(&self, task: &i64) -> Result<(), ()> {
        self.call(format!("retire runner of {task}"));
        Ok(())
    }
}

#[test]
fn passes_run_in_order_with_their_cutoffs_and_reasons() {
    let store = Store { silent: vec![1], endless: vec![2], abandoned: vec![7, 8], ..Store::default() };
    let swept = run(StaleTasks::sweep(&store, &TIMEOUTS, 100_000)).unwrap();
    assert_eq!(swept, StaleSwept { zombies: 1, endless: 1, abandoned: 2 });
    assert_eq!(
        store.calls(),
        [
            "silent before 99940",
            "note 1: the runner sent no word for 60 seconds, so the task failed",
            "fail 1",
            "close 1",
            "settle task 1",
            "retire runner of 1",
            "long running before 96400",
            "note 2: the task ran longer than 3600 seconds, so it failed",
            "fail 2",
            "close 2",
            "settle task 2",
            "retire runner of 2",
            "cancel abandoned before 13600 at 100000",
            "settle run 7",
            "settle run 8",
        ]
    );
}

#[test]
fn tasks_that_are_gone_or_already_stopped_are_not_counted() {
    let store = Store { silent: vec![1, 2, 3], gone: vec![1], stopped: vec![2], ..Store::default() };
    let swept = run(StaleTasks::sweep(&store, &TIMEOUTS, 100_000)).unwrap();
    assert_eq!(swept.zombies, 1);
    let calls = store.calls();
    assert!(!calls.iter().any(|call| call.starts_with("note 1:") || call == "fail 1"), "a gone task gets no note: {calls:?}");
    assert!(
        calls.contains(&"note 2: the runner sent no word for 60 seconds, so the task failed".to_owned()),
        "a stopped task is noted first"
    );
    assert!(!calls.iter().any(|call| call == "close 2" || call == "retire runner of 2"), "but not closed");
}

#[test]
fn cutoffs_never_overflow() {
    assert_eq!(before(1_000, 600), 400);
    assert!(before(1_000, u64::MAX) < 0);
}
