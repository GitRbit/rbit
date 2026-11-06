#![cfg(feature = "applications")]
//! Independent conditional lifecycle policy, clocks and construction boundaries.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::threads::{
    Issue, IssueId, StateReason, SystemThreadClosure, ThreadLifecycleRepository, ThreadLockError, ThreadLocking,
};
use rbit_contracts::{AccountId, Timestamp};
use std::sync::Mutex;
#[derive(Default)]
struct State {
    calls: Vec<&'static str>,
    changed: bool,
    closed: Option<Issue>,
    fail: bool,
}
struct Port<'a>(&'a Mutex<State>);
fn factory(state: &Mutex<State>) -> Port<'_> {
    state.lock().unwrap().calls.push("construct");
    Port(state)
}
fn clock(state: &Mutex<State>) -> Timestamp {
    state.lock().unwrap().calls.push("clock");
    Timestamp(9)
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ThreadLifecycleRepository for Port<'_> {
    type Error = &'static str;
    type CloseEvent = str;
    async fn close(
        &self,
        issue: IssueId,
        actor: Option<AccountId>,
        reason: StateReason,
        event: &str,
        at: Timestamp,
    ) -> Result<Option<Issue>, Self::Error> {
        assert_eq!(issue, IssueId(1));
        assert_eq!(actor, None);
        assert_eq!(reason, StateReason::Merged);
        assert_eq!(event, "opaque event");
        assert_eq!(at, Timestamp(9));
        let mut state = self.0.lock().unwrap();
        state.calls.push("close");
        if state.fail { Err("SQL failed") } else { Ok(state.closed.clone()) }
    }
    async fn change_lock(&self, issue: IssueId, locked: bool, actor: AccountId, at: Timestamp) -> Result<bool, Self::Error> {
        assert_eq!((issue, actor, at), (IssueId(1), AccountId(3), Timestamp(9)));
        assert!(locked);
        let mut state = self.0.lock().unwrap();
        state.calls.push("lock");
        if state.fail { Err("SQL failed") } else { Ok(state.changed) }
    }
}
#[tokio::test]
async fn lock_authorization_archive_and_unpolled_paths_never_construct_or_read_clock() {
    let state = Mutex::new(State::default());
    assert!(matches!(
        ThreadLocking::new(false, true).set(IssueId(1), AccountId(3), true, || factory(&state), || clock(&state)).await,
        Err(ThreadLockError::Forbidden)
    ));
    assert!(matches!(
        ThreadLocking::new(true, true).set(IssueId(1), AccountId(3), true, || factory(&state), || clock(&state)).await,
        Err(ThreadLockError::Archived)
    ));
    let app = ThreadLocking::new(true, false);
    drop(app.set(IssueId(1), AccountId(3), true, || factory(&state), || clock(&state)));
    drop(SystemThreadClosure::close(IssueId(1), None, StateReason::Merged, "opaque event", || factory(&state), || clock(&state)));
    assert_eq!(state.lock().unwrap().calls, Vec::<&str>::new());
}
#[tokio::test]
async fn lock_winner_and_noop_both_keep_one_store_one_clock_and_one_conditional_mutation() {
    for changed in [false, true] {
        let state = Mutex::new(State { changed, ..State::default() });
        assert_eq!(
            ThreadLocking::new(true, false).set(IssueId(1), AccountId(3), true, || factory(&state), || clock(&state)).await.unwrap(),
            changed
        );
        assert_eq!(state.lock().unwrap().calls, ["construct", "clock", "lock"]);
    }
}
#[tokio::test]
async fn privileged_closure_forwards_event_reason_and_actor_with_no_user_or_archive_gate() {
    let issue = fixtures::issue();
    let state = Mutex::new(State { closed: Some(issue.clone()), ..State::default() });
    assert_eq!(
        SystemThreadClosure::close(IssueId(1), None, StateReason::Merged, "opaque event", || factory(&state), || clock(&state))
            .await
            .unwrap(),
        Some(issue)
    );
    assert_eq!(state.lock().unwrap().calls, ["construct", "clock", "close"]);
}
#[tokio::test]
async fn missing_or_already_closed_has_no_fallback_read_and_errors_remain_errors() {
    let state = Mutex::new(State::default());
    assert!(
        SystemThreadClosure::close(IssueId(1), None, StateReason::Merged, "opaque event", || factory(&state), || clock(&state))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(state.lock().unwrap().calls, ["construct", "clock", "close"]);
    state.lock().unwrap().fail = true;
    assert_eq!(
        SystemThreadClosure::close(IssueId(1), None, StateReason::Merged, "opaque event", || factory(&state), || clock(&state))
            .await
            .unwrap_err(),
        "SQL failed"
    );
    assert!(matches!(
        ThreadLocking::new(true, false).set(IssueId(1), AccountId(3), true, || factory(&state), || clock(&state)).await,
        Err(ThreadLockError::Store("SQL failed"))
    ));
}
