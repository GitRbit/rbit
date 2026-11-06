#![cfg(feature = "applications")]
//! Independent assignment ordering, no-op work and non-snapshot replacement semantics.

use rbit_collaboration::{
    labels::{
        Label, LabelAttachmentAccess, LabelAttachmentError, LabelAttachmentKind, LabelAttachmentMutation, LabelAttachmentPort,
        LabelAttachments, LabelId, LabelSnapshot,
    },
    threads::IssueId,
};
use rbit_contracts::{RepoId, Timestamp};
use std::{collections::VecDeque, sync::Mutex};

#[derive(Default)]
struct State {
    current: Vec<(IssueId, LabelSnapshot)>,
    snapshots: VecDeque<Vec<(IssueId, LabelSnapshot)>>,
    calls: Vec<String>,
    fail_record: bool,
}
struct Port<'a>(&'a Mutex<State>);
struct Mutation<'a>(&'a Mutex<State>);
fn label(id: LabelId) -> Label {
    Label {
        id,
        repo_id: RepoId(3),
        name: format!("l{}", id.0),
        color: "#aabbcc".into(),
        description: String::new(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}
fn snapshot(id: i64) -> (IssueId, LabelSnapshot) {
    (IssueId(1), label(LabelId(id)).snapshot())
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl<'a> LabelAttachmentPort for Port<'a> {
    type Error = &'static str;
    type Mutation = Mutation<'a>;
    async fn resolve(&self, ids: &[LabelId]) -> Result<Vec<Label>, Self::Error> {
        self.0.lock().unwrap().calls.push(format!("resolve:{:?}", ids.iter().map(|id| id.0).collect::<Vec<_>>()));
        if ids.iter().any(|id| id.0 < 0) {
            return Err("unknown label");
        }
        Ok(ids.iter().copied().map(label).collect())
    }
    fn mutation(&self) -> Self::Mutation {
        self.0.lock().unwrap().calls.push("view".into());
        Mutation(self.0)
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl LabelAttachmentMutation for Mutation<'_> {
    type Error = &'static str;
    async fn current(&self) -> Result<Vec<(IssueId, LabelSnapshot)>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("current".into());
        Ok(if let Some(snapshot) = state.snapshots.pop_front() { snapshot } else { state.current.clone() })
    }
    async fn remove(&self, id: LabelId) -> Result<bool, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("remove:{}", id.0));
        let before = state.current.len();
        state.current.retain(|(_, label)| label.id != id);
        Ok(before != state.current.len())
    }
    async fn add(&self, id: LabelId, at: Timestamp) -> Result<bool, Self::Error> {
        assert_eq!(at, Timestamp(9));
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("add:{}", id.0));
        if state.current.iter().any(|(_, label)| label.id == id) {
            return Ok(false);
        }
        state.current.push(snapshot(id.0));
        Ok(true)
    }
    async fn record(&self, kind: LabelAttachmentKind, label: &LabelSnapshot) -> Result<(), Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("record:{kind:?}:{}", label.id.0));
        if state.fail_record { Err("timeline failed") } else { Ok(()) }
    }
    fn publish(&self, kind: LabelAttachmentKind, label: LabelSnapshot) {
        self.0.lock().unwrap().calls.push(format!("publish:{kind:?}:{}", label.id.0));
    }
}
fn access(can_triage: bool, archived: bool) -> LabelAttachmentAccess {
    LabelAttachmentAccess { can_triage, archived }
}

#[tokio::test]
async fn denied_and_archived_requests_create_no_view_or_reads_and_unknown_adds_win_before_current() {
    let state = Mutex::new(State::default());
    let port = Port(&state);
    for (allowed, archived) in [(false, true), (true, true)] {
        let result = LabelAttachments::new(&port, access(allowed, archived))
            .change(&[LabelId(-1)], &[], || -> Result<(), ()> { panic!("spend") }, || panic!("clock"))
            .await;
        if allowed {
            assert!(matches!(result, Err(LabelAttachmentError::Archived)));
        } else {
            assert!(matches!(result, Err(LabelAttachmentError::Forbidden)));
        }
    }
    assert_eq!(state.lock().unwrap().calls.len(), 0);
    let result = LabelAttachments::new(&port, access(true, false))
        .change(&[LabelId(-1)], &[], || -> Result<(), ()> { panic!("spend") }, || panic!("clock"))
        .await;
    assert!(matches!(result, Err(LabelAttachmentError::Port("unknown label"))));
    assert_eq!(state.lock().unwrap().calls, ["resolve:[-1]"]);
}

#[tokio::test]
async fn duplicate_adds_win_and_removal_records_precede_input_order_additions() {
    let state = Mutex::new(State { current: vec![snapshot(3), snapshot(4)], ..State::default() });
    let port = Port(&state);
    LabelAttachments::new(&port, access(true, false))
        .change(
            &[LabelId(2), LabelId(1), LabelId(2)],
            &[LabelId(4), LabelId(3), LabelId(2), LabelId(4)],
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
    assert_eq!(
        state.lock().unwrap().calls,
        [
            "resolve:[2, 1]",
            "view",
            "current",
            "spend",
            "clock",
            "remove:3",
            "record:Unlabeled:3",
            "publish:Unlabeled:3",
            "remove:4",
            "record:Unlabeled:4",
            "publish:Unlabeled:4",
            "add:2",
            "record:Labeled:2",
            "publish:Labeled:2",
            "add:1",
            "record:Labeled:1",
            "publish:Labeled:1"
        ]
    );
}

#[tokio::test]
async fn cap_is_checked_before_throttle_and_noop_still_spends_once_and_reads_clock() {
    let state = Mutex::new(State { current: (1..=25).map(snapshot).collect(), ..State::default() });
    let port = Port(&state);
    let app = LabelAttachments::new(&port, access(true, false));
    let result = app.change(&[LabelId(26)], &[], || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
    assert!(matches!(result,Err(LabelAttachmentError::Invalid(message)) if message=="an issue can have at most 25 labels"));
    assert_eq!(state.lock().unwrap().calls, ["resolve:[26]", "view", "current"]);
    state.lock().unwrap().calls.clear();
    app.change(
        &[],
        &[],
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
    assert_eq!(state.lock().unwrap().calls, ["resolve:[]", "view", "current", "spend", "clock"]);
}

#[tokio::test]
async fn set_retains_two_reads_and_does_not_remove_a_concurrent_label_unseen_by_first_read() {
    let first = vec![snapshot(1)];
    let second = vec![snapshot(1), snapshot(2)];
    let state = Mutex::new(State { current: second.clone(), snapshots: VecDeque::from([first, second]), ..State::default() });
    let port = Port(&state);
    LabelAttachments::new(&port, access(true, false)).set(&[LabelId(1)], || Ok::<_, ()>(()), || Timestamp(9)).await.unwrap();
    assert_eq!(state.lock().unwrap().current.len(), 2);
    assert_eq!(state.lock().unwrap().calls, ["view", "current", "resolve:[1]", "view", "current", "add:1"]);
}

#[tokio::test]
async fn timeline_failure_keeps_committed_association_and_stops_before_publication() {
    let state = Mutex::new(State { fail_record: true, ..State::default() });
    let port = Port(&state);
    let result = LabelAttachments::new(&port, access(true, false)).change(&[LabelId(1)], &[], || Ok::<_, ()>(()), || Timestamp(9)).await;
    assert!(matches!(result, Err(LabelAttachmentError::Port("timeline failed"))));
    assert_eq!(state.lock().unwrap().current.len(), 1);
    assert_eq!(state.lock().unwrap().calls, ["resolve:[1]", "view", "current", "add:1", "record:Labeled:1"]);
}
