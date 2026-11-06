#![cfg(feature = "applications")]
//! Independent assignee lookup, mutation and non-snapshot replacement contracts.

use rbit_collaboration::{
    assignees::{AssigneeAccess, AssigneeAssignments, AssigneeAttachmentKind, AssigneeAttachmentPort, AssigneeError},
    threads::IssueId,
};
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use std::{collections::VecDeque, sync::Mutex};

fn user(id: AccountId) -> User {
    User {
        account: Account {
            id,
            kind: AccountKind::User,
            name: format!("u{}", id.0).parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: format!("u{}@example.com", id.0).parse().unwrap(),
        is_admin: false,
    }
}
fn row(id: i64) -> (IssueId, Account) {
    (IssueId(1), user(AccountId(id)).account)
}
#[derive(Default)]
struct State {
    current: Vec<(IssueId, Account)>,
    snapshots: VecDeque<Vec<(IssueId, Account)>>,
    denied: Vec<AccountId>,
    calls: Vec<String>,
    fail_record: bool,
}
struct Port<'a>(&'a Mutex<State>);
fn view(state: &Mutex<State>) -> Port<'_> {
    state.lock().unwrap().calls.push("view".into());
    Port(state)
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl AssigneeAttachmentPort for Port<'_> {
    type Error = &'static str;
    async fn current(&self) -> Result<Vec<(IssueId, Account)>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("current".into());
        Ok(if let Some(snapshot) = state.snapshots.pop_front() { snapshot } else { state.current.clone() })
    }
    async fn resolve_new(&self, ids: &[AccountId]) -> Result<Vec<User>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("resolve:{:?}", ids.iter().map(|id| id.0).collect::<Vec<_>>()));
        if ids.iter().any(|id| id.0 < 0 || state.denied.contains(id)) {
            return Err("invalid assignee");
        }
        Ok(ids.iter().copied().map(user).collect())
    }
    async fn remove(&self, id: AccountId) -> Result<bool, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("remove:{}", id.0));
        let count = state.current.len();
        state.current.retain(|(_, account)| account.id != id);
        Ok(count != state.current.len())
    }
    async fn add(&self, id: AccountId, at: Timestamp) -> Result<bool, Self::Error> {
        assert_eq!(at, Timestamp(9));
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("add:{}", id.0));
        if state.current.iter().any(|(_, account)| account.id == id) {
            return Ok(false);
        }
        state.current.push(row(id.0));
        Ok(true)
    }
    async fn record(&self, kind: AssigneeAttachmentKind, id: AccountId) -> Result<(), Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("record:{kind:?}:{}", id.0));
        if state.fail_record { Err("history failed") } else { Ok(()) }
    }
    fn publish(&self, kind: AssigneeAttachmentKind, id: AccountId) {
        self.0.lock().unwrap().calls.push(format!("publish:{kind:?}:{}", id.0));
    }
}
fn access(allowed: bool, archived: bool) -> AssigneeAccess {
    AssigneeAccess { can_triage: allowed, archived }
}

#[tokio::test]
async fn forbidden_and_archived_requests_create_no_store_and_never_read_or_spend() {
    for (allowed, archived) in [(false, true), (true, true)] {
        let result = AssigneeAssignments::new(access(allowed, archived))
            .change(
                &[AccountId(-1)],
                &[],
                || -> Port<'static> { panic!("store") },
                || -> Result<(), ()> { panic!("spend") },
                || panic!("clock"),
            )
            .await;
        if allowed {
            assert!(matches!(result, Err(AssigneeError::Archived)));
        } else {
            assert!(matches!(result, Err(AssigneeError::Forbidden)));
        }
    }
}

#[tokio::test]
async fn current_roster_precedes_new_lookups_and_existing_assignees_are_not_revalidated() {
    let state = Mutex::new(State { current: vec![row(1)], denied: vec![AccountId(1)], ..State::default() });
    AssigneeAssignments::new(access(true, false))
        .change(
            &[AccountId(1), AccountId(1)],
            &[AccountId(1)],
            || view(&state),
            || {
                state.lock().unwrap().calls.push("spend".into());
                Ok::<_, ()>(())
            },
            || {
                state.lock().unwrap().calls.push("clock".into());
                Timestamp(9)
            },
        )
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "resolve:[]", "spend", "clock"]);
}

#[tokio::test]
async fn additions_win_and_removal_effects_follow_current_order_before_ordered_new_users() {
    let state = Mutex::new(State { current: vec![row(3), row(4)], ..State::default() });
    AssigneeAssignments::new(access(true, false))
        .change(
            &[AccountId(2), AccountId(1), AccountId(2)],
            &[AccountId(4), AccountId(3), AccountId(2)],
            || view(&state),
            || Ok::<_, ()>(()),
            || Timestamp(9),
        )
        .await
        .unwrap();
    assert_eq!(
        state.lock().unwrap().calls,
        [
            "view",
            "current",
            "resolve:[2, 1]",
            "remove:3",
            "record:Unassigned:3",
            "publish:Unassigned:3",
            "remove:4",
            "record:Unassigned:4",
            "publish:Unassigned:4",
            "add:2",
            "record:Assigned:2",
            "publish:Assigned:2",
            "add:1",
            "record:Assigned:1",
            "publish:Assigned:1"
        ]
    );
}

#[tokio::test]
async fn new_user_validation_precedes_cap_and_throttle_without_mutating_roster() {
    let state = Mutex::new(State { current: (1..=10).map(row).collect(), ..State::default() });
    let app = AssigneeAssignments::new(access(true, false));
    let result = app.change(&[AccountId(-1)], &[], || view(&state), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
    assert!(matches!(result, Err(AssigneeError::Port("invalid assignee"))));
    state.lock().unwrap().calls.clear();
    let result = app.change(&[AccountId(11)], &[], || view(&state), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
    assert!(matches!(result,Err(AssigneeError::Invalid(message)) if message=="an issue can have at most 10 assignees"));
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "resolve:[11]"]);
}

#[tokio::test]
async fn replacement_keeps_two_reads_and_an_assignment_unseen_by_initial_read() {
    let initial = vec![row(1)];
    let concurrent = vec![row(1), row(2)];
    let state = Mutex::new(State { current: concurrent.clone(), snapshots: VecDeque::from([initial, concurrent]), ..State::default() });
    AssigneeAssignments::new(access(true, false)).set(&[AccountId(1)], || view(&state), || Ok::<_, ()>(()), || Timestamp(9)).await.unwrap();
    assert_eq!(state.lock().unwrap().current.len(), 2);
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "view", "current", "resolve:[]"]);
}

#[tokio::test]
async fn failed_history_retains_association_and_retry_does_not_revalidate_or_repair() {
    let state = Mutex::new(State { fail_record: true, ..State::default() });
    let app = AssigneeAssignments::new(access(true, false));
    let result = app.change(&[AccountId(1)], &[], || view(&state), || Ok::<_, ()>(()), || Timestamp(9)).await;
    assert!(matches!(result, Err(AssigneeError::Port("history failed"))));
    assert_eq!(state.lock().unwrap().current.len(), 1);
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "resolve:[1]", "add:1", "record:Assigned:1"]);
    state.lock().unwrap().calls.clear();
    app.change(&[AccountId(1)], &[], || view(&state), || Ok::<_, ()>(()), || Timestamp(9)).await.unwrap();
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "resolve:[]"]);
}
