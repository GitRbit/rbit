#![cfg(feature = "applications")]
//! State policy ordering and authoritative conditional-store results.

#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::threads::{
    IssueId, IssueKind, StateChange, StateReason, ThreadStateAccess, ThreadStateChanger, ThreadStateError, ThreadStateRepository,
    ThreadStateResult,
};
use rbit_contracts::{AccountId, Timestamp};
use std::{future, sync::Mutex};

struct Store {
    calls: Mutex<Vec<&'static str>>,
    result: Option<ThreadStateResult>,
    fail: bool,
}
impl Store {
    fn new(changed: bool) -> Self {
        Self { calls: Mutex::new(Vec::new()), result: Some(ThreadStateResult { issue: fixtures::issue(), changed }), fail: false }
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ThreadStateRepository for Store {
    type Error = &'static str;
    async fn change_state(
        &self,
        id: IssueId,
        _: StateChange,
        actor: AccountId,
        at: Timestamp,
    ) -> Result<Option<ThreadStateResult>, Self::Error> {
        self.calls.lock().unwrap().push("state");
        assert_eq!(id, IssueId(1));
        assert_eq!(actor, AccountId(4));
        assert_eq!(at, Timestamp(9));
        if self.fail { Err("state failed") } else { Ok(self.result.clone()) }
    }
}
fn access(allowed: bool, archived: bool) -> ThreadStateAccess {
    ThreadStateAccess { can_set_state: allowed, archived }
}

#[tokio::test]
async fn permission_archive_and_merged_reason_refusal_precede_reopen_clock_and_store() {
    let store = Store::new(true);
    let issue = fixtures::issue();
    for (allowed, archived) in [(false, true), (true, true)] {
        let result = ThreadStateChanger::new(&store, AccountId(4), access(allowed, archived))
            .set_state(
                &issue,
                StateChange::Close(StateReason::Merged),
                || -> future::Ready<Result<(), ()>> { panic!("reopen") },
                || panic!("clock"),
            )
            .await;
        if allowed {
            assert!(matches!(result, Err(ThreadStateError::Archived)));
        } else {
            assert!(matches!(result, Err(ThreadStateError::Forbidden)));
        }
    }
    let result = ThreadStateChanger::new(&store, AccountId(4), access(true, false))
        .set_state(
            &issue,
            StateChange::Close(StateReason::Merged),
            || -> future::Ready<Result<(), ()>> { panic!("reopen") },
            || panic!("clock"),
        )
        .await;
    assert!(matches!(result,Err(ThreadStateError::Invalid(message)) if message=="only merging a pull request closes it as merged"));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
}

#[tokio::test]
async fn pull_reopen_gate_runs_even_when_snapshot_is_open_and_wins_before_clock_or_sql() {
    let store = Store::new(false);
    let mut issue = fixtures::issue();
    issue.kind = IssueKind::Pull;
    let changer = ThreadStateChanger::new(&store, AccountId(4), access(true, false));
    let result = changer.set_state(&issue, StateChange::Reopen, || future::ready(Err("merged pull")), || panic!("clock")).await;
    assert!(matches!(result, Err(ThreadStateError::Reopen("merged pull"))));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
    let result = changer
        .set_state(
            &issue,
            StateChange::Reopen,
            || async {
                store.calls.lock().unwrap().push("reopen");
                Ok::<_, ()>(())
            },
            || {
                store.calls.lock().unwrap().push("clock");
                Timestamp(9)
            },
        )
        .await
        .unwrap();
    assert!(!result.changed);
    assert_eq!(*store.calls.lock().unwrap(), ["reopen", "clock", "state"]);
}

#[tokio::test]
async fn plain_issue_does_not_run_pull_gate_and_store_decides_changed_instead_of_snapshot() {
    let store = Store::new(true);
    let issue = fixtures::issue();
    let result = ThreadStateChanger::new(&store, AccountId(4), access(true, false))
        .set_state(&issue, StateChange::Reopen, || -> future::Ready<Result<(), ()>> { panic!("not a pull") }, || Timestamp(9))
        .await
        .unwrap();
    assert!(result.changed, "the supplied snapshot does not short-circuit current database state");
    assert_eq!(*store.calls.lock().unwrap(), ["state"]);
}

#[tokio::test]
async fn missing_and_store_errors_remain_distinct_and_do_not_invent_success() {
    let mut store = Store::new(false);
    store.result = None;
    let issue = fixtures::issue();
    let result = ThreadStateChanger::new(&store, AccountId(4), access(true, false))
        .set_state(&issue, StateChange::Close(StateReason::Completed), || future::ready(Ok::<_, ()>(())), || Timestamp(9))
        .await;
    assert!(matches!(result, Err(ThreadStateError::NotFound)));
    store.fail = true;
    let result = ThreadStateChanger::new(&store, AccountId(4), access(true, false))
        .set_state(&issue, StateChange::Close(StateReason::Completed), || future::ready(Ok::<_, ()>(())), || Timestamp(9))
        .await;
    assert!(matches!(result, Err(ThreadStateError::Store("state failed"))));
}
