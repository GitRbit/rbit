//! Long-poll decisions over a scripted, immediately ready port.
#![allow(clippy::unused_async_trait_impl)]
use rbit_actions::{
    claims::{Claim, Resumed},
    polling::{LongPoll, PollEnd, PollQueue, PollRefusal, PollSlots, Wakeup, admit},
};
use rbit_workflow::protocol::PollRequest;
use std::{
    collections::VecDeque,
    future::Future,
    pin::pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("scripted ports are always ready"),
    }
}

#[derive(Debug, Default)]
struct Queue {
    calls: Mutex<Vec<String>>,
    version: AtomicI64,
    idle: AtomicI64,
    resumed: Mutex<Option<Resumed<u32>>>,
    claims: Mutex<VecDeque<Claim<u32>>>,
    wakeups: Mutex<VecDeque<(i64, Wakeup)>>,
}

impl Queue {
    fn call(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl PollQueue for Queue {
    type Error = ();
    type Spec = u32;
    fn version(&self) -> i64 {
        self.version.load(Ordering::SeqCst)
    }
    fn idle_version(&self) -> i64 {
        self.idle.load(Ordering::SeqCst)
    }
    fn set_idle_version(&self, version: i64) {
        self.call(format!("idle {version}"));
        self.idle.store(version, Ordering::SeqCst);
    }
    async fn resume(&self, key: &str) -> Result<Resumed<u32>, ()> {
        self.call(format!("resume {key}"));
        Ok(self.resumed.lock().unwrap().take().unwrap_or(Resumed::Unknown))
    }
    async fn claim(&self) -> Result<Claim<u32>, ()> {
        self.call("claim");
        Ok(self.claims.lock().unwrap().pop_front().unwrap_or(Claim::Nothing))
    }
    async fn wait(&self, seen: i64) -> Wakeup {
        self.call(format!("wait {seen}"));
        let (version, wakeup) = self.wakeups.lock().unwrap().pop_front().unwrap_or((seen, Wakeup::Ended));
        self.version.store(version, Ordering::SeqCst);
        wakeup
    }
}

fn wait_for_work(queue: &Queue, key: Option<&str>) -> Option<u32> {
    run(LongPoll::wait_for_work(queue, key)).unwrap()
}

#[test]
fn a_retried_key_resends_or_ends_before_any_claim() {
    let resent = Queue { resumed: Mutex::new(Some(Resumed::Task(Box::new(4)))), ..Queue::default() };
    assert_eq!(wait_for_work(&resent, Some("k")), Some(4));
    assert_eq!(resent.calls(), ["resume k"]);
    let spent = Queue { resumed: Mutex::new(Some(Resumed::Spent)), ..Queue::default() };
    assert_eq!(wait_for_work(&spent, Some("k")), None);
    assert_eq!(spent.calls(), ["resume k"]);
}

#[test]
fn an_idle_runner_waits_without_claiming_until_the_version_moves() {
    let queue = Queue { version: AtomicI64::new(5), idle: AtomicI64::new(5), ..Queue::default() };
    queue.wakeups.lock().unwrap().push_back((6, Wakeup::Moved));
    queue.claims.lock().unwrap().push_back(Claim::Task(Box::new(9)));
    assert_eq!(wait_for_work(&queue, None), Some(9));
    assert_eq!(queue.calls(), ["wait 5", "claim"]);
}

#[test]
fn an_empty_claim_remembers_the_version_and_a_spent_key_ends_the_poll() {
    let queue = Queue { version: AtomicI64::new(3), ..Queue::default() };
    assert_eq!(wait_for_work(&queue, Some("k")), None);
    assert_eq!(queue.calls(), ["resume k", "claim", "idle 3", "wait 3"]);
    let spent = Queue { version: AtomicI64::new(3), ..Queue::default() };
    spent.claims.lock().unwrap().push_back(Claim::Spent);
    assert_eq!(wait_for_work(&spent, Some("k")), None);
    assert_eq!(spent.calls(), ["resume k", "claim"]);
}

#[test]
fn polls_are_refused_in_order() {
    let request = PollRequest { free_slots: 1 };
    let invalid = admit(&PollRequest { free_slots: 0 }, Some("bad key!"), true);
    assert!(matches!(invalid, Err(PollRefusal::Invalid(message)) if message.contains("free_slots")), "the request first");
    assert_eq!(admit(&request, Some("bad key!"), true), Err(PollRefusal::BadKey));
    assert_eq!(admit(&request, Some("good-key_1"), true), Err(PollRefusal::Disabled));
    assert_eq!(admit(&request, None, false), Ok(()));
    assert_eq!(PollRefusal::Full.message(), "the server holds as many polls as it may; try again shortly");
}

#[derive(Debug, Clone, Default)]
struct Ended(Arc<AtomicBool>);

impl PollEnd for Ended {
    fn new() -> Self {
        Self::default()
    }
    fn end(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn polls_are_one_per_runner_and_bounded() {
    let polls = PollSlots::<u8, Ended>::default();
    let first = polls.enter(1, 2).unwrap();
    let second = polls.enter(1, 2).unwrap();
    assert!(first.ended.0.load(Ordering::SeqCst), "a second poll ends the first");
    assert!(!second.ended.0.load(Ordering::SeqCst));
    drop(first);
    assert_eq!(polls.len(), 1, "the ended poll does not free the newer one's place");
    let _other = polls.enter(2, 2).unwrap();
    assert!(polls.enter(3, 2).is_none(), "the instance holds at most max polls");
    drop(second);
    assert!(polls.enter(3, 2).is_some());
}
