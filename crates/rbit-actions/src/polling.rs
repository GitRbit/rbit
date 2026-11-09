//! `POST poll` (§12.6): who may hold a long poll, and how one waits for work.
//!
//! * A poll is refused when its request or `Idempotency-Key` is malformed, its runner is
//!   disabled, or the instance already holds `max_polls` polls of other runners. One poll
//!   per runner: a newer one ends the older, which answers empty ([`PollSlots`]).
//! * A poll with a key first resumes the task that key already claimed: it is resent, or,
//!   when it stopped, the poll answers empty and the key never claims again.
//! * Then it loops ([`LongPoll::wait_for_work`]): when the scheduler's version moved since
//!   this runner's last empty answer, it runs the claim once; a claim answers, a key spent
//!   on an unrunnable job answers empty, and nothing remembers the version as idle.
//!   Otherwise it waits for the version to move, and answers empty once the long poll is
//!   over, a newer poll ended it, or the server shuts down. An idle runner costs no query.
//!
//! The runtime (timers, the wake-up notification, cancellation) is composition's, behind
//! [`PollQueue::wait`] and [`PollEnd`].

use std::{
    collections::HashMap,
    future::Future,
    hash::Hash,
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use rbit_workflow::protocol::{PollRequest, valid_idempotency_key};

use crate::claims::{Claim, Resumed};

/// Why a poll is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollRefusal {
    /// The request is malformed: `bad_request` with the validation's message.
    Invalid(String),
    /// `bad_request`.
    BadKey,
    /// `forbidden`.
    Disabled,
    /// `unavailable`: the instance holds `max_polls` polls.
    Full,
}

impl PollRefusal {
    /// The runner's message.
    pub fn message(&self) -> &str {
        match self {
            Self::Invalid(message) => message,
            Self::BadKey => "the Idempotency-Key must be 1 to 64 letters, digits, '-' or '_'",
            Self::Disabled => "this runner is disabled and takes no new jobs; an administrator can enable it again",
            Self::Full => "the server holds as many polls as it may; try again shortly",
        }
    }
}

/// Checks a poll before it takes a place: the request, the key and the runner's state.
pub fn admit(request: &PollRequest, key: Option<&str>, runner_disabled: bool) -> Result<(), PollRefusal> {
    request.validate().map_err(|error| PollRefusal::Invalid(error.message))?;
    if key.is_some_and(|key| !valid_idempotency_key(key)) {
        return Err(PollRefusal::BadKey);
    }
    if runner_disabled {
        return Err(PollRefusal::Disabled);
    }
    Ok(())
}

/// How a poll is told it was replaced by a newer one of its runner.
pub trait PollEnd: Clone {
    fn new() -> Self;
    fn end(&self);
}

/// The polls held open, one per runner, and how many there are.
#[derive(Debug)]
pub struct PollSlots<R, E> {
    by_runner: Mutex<HashMap<R, (u64, E)>>,
    next: AtomicU64,
}

impl<R, E> Default for PollSlots<R, E> {
    fn default() -> Self {
        Self { by_runner: Mutex::new(HashMap::new()), next: AtomicU64::new(0) }
    }
}

/// A poll's place in [`PollSlots`]; dropping it frees the place.
#[derive(Debug)]
pub struct PollSlot<'a, R: Eq + Hash + Copy, E> {
    slots: &'a PollSlots<R, E>,
    runner: R,
    id: u64,
    /// Ended when a newer poll of the runner takes its place.
    pub ended: E,
}

impl<R: Eq + Hash + Copy, E: PollEnd> PollSlots<R, E> {
    /// Takes a place for `runner`'s poll, ending the poll it may already have. `None` when
    /// `max` polls are already held open by other runners.
    pub fn enter(&self, runner: R, max: usize) -> Option<PollSlot<'_, R, E>> {
        let mut polls = self.by_runner.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((_, previous)) = polls.get(&runner) {
            previous.end();
        } else if polls.len() >= max {
            return None;
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let ended = E::new();
        polls.insert(runner, (id, ended.clone()));
        Some(PollSlot { slots: self, runner, id, ended })
    }
}

impl<R, E> PollSlots<R, E> {
    /// Polls held open now.
    pub fn len(&self) -> usize {
        self.by_runner.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<R: Eq + Hash + Copy, E> Drop for PollSlot<'_, R, E> {
    fn drop(&mut self) {
        let mut polls = self.slots.by_runner.lock().unwrap_or_else(PoisonError::into_inner);
        if polls.get(&self.runner).is_some_and(|(id, _)| *id == self.id) {
            polls.remove(&self.runner);
        }
    }
}

/// What ended a wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakeup {
    /// The scheduler's version moved.
    Moved,
    /// The long poll is over, a newer poll replaced this one, or the server shuts down.
    Ended,
}

/// One runner's poll: its claims and the scheduler's version.
pub trait PollQueue: Sync {
    type Error: Send;
    type Spec: Send;

    /// The scheduler's version now.
    fn version(&self) -> i64;
    /// The version of this runner's last poll that found nothing.
    fn idle_version(&self) -> i64;
    fn set_idle_version(&self, version: i64);
    /// The task the key already claimed, with a fresh token, if it still runs here.
    fn resume(&self, key: &str) -> impl Future<Output = Result<Resumed<Self::Spec>, Self::Error>> + Send;
    /// Runs the claim once.
    fn claim(&self) -> impl Future<Output = Result<Claim<Self::Spec>, Self::Error>> + Send;
    /// Waits until the version is no longer `seen`, or the poll ends. A move after `seen`
    /// was read, even before this is called, must answer [`Wakeup::Moved`].
    fn wait(&self, seen: i64) -> impl Future<Output = Wakeup> + Send;
}

/// The long poll.
#[derive(Debug)]
pub struct LongPoll;

impl LongPoll {
    /// A task for the runner, or `None` once the poll ends without one.
    pub async fn wait_for_work<P: PollQueue>(port: &P, key: Option<&str>) -> Result<Option<P::Spec>, P::Error> {
        if let Some(key) = key {
            match port.resume(key).await? {
                Resumed::Task(spec) => return Ok(Some(*spec)),
                // The key's task has stopped: there is nothing to resend, and the key may
                // not claim again. The runner polls anew with a new key.
                Resumed::Spent => return Ok(None),
                Resumed::Unknown => {}
            }
        }
        loop {
            let version = port.version();
            if version != port.idle_version() {
                match port.claim().await? {
                    Claim::Task(spec) => return Ok(Some(*spec)),
                    Claim::Nothing => port.set_idle_version(version),
                    // The key went to a task that could not run: the runner polls again
                    // with a new one, and finds the rest of the queue then.
                    Claim::Spent => return Ok(None),
                }
            }
            if port.wait(version).await == Wakeup::Ended {
                return Ok(None);
            }
        }
    }
}
