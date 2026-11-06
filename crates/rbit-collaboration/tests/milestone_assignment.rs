#![cfg(feature = "applications")]
//! Independent assignment fast paths, stale pre-read effects and partial commits.

use rbit_collaboration::{
    IssueState,
    milestones::{
        IssueMilestone, Milestone, MilestoneAssignment, MilestoneAttachmentAccess, MilestoneAttachmentError, MilestoneAttachmentKind,
        MilestoneAttachmentPort, MilestoneId,
    },
    threads::IssueId,
};
use rbit_contracts::{RepoId, Timestamp};
use std::sync::Mutex;
#[derive(Default)]
struct State {
    current: Option<IssueMilestone>,
    calls: Vec<String>,
    fail_record: bool,
}
struct Port<'a>(&'a Mutex<State>);
fn old(id: i64) -> IssueMilestone {
    IssueMilestone { issue: IssueId(1), id: MilestoneId(id), title: format!("v{id}") }
}
fn milestone(id: MilestoneId) -> Milestone {
    Milestone {
        id,
        repo_id: RepoId(3),
        title: format!("v{}", id.0),
        description: String::new(),
        due_on: None,
        state: IssueState::Open,
        closed_at: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}
impl MilestoneAttachmentPort for Port<'_> {
    type Error = &'static str;
    async fn find(&self, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        self.0.lock().unwrap().calls.push(format!("find:{}", id.0));
        Ok((id.0 >= 0).then(|| milestone(id)))
    }
    async fn current(&self) -> Result<Vec<IssueMilestone>, Self::Error> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("current".into());
        Ok(s.current.clone().into_iter().collect())
    }
    async fn write(&self, id: Option<MilestoneId>) -> Result<(), Self::Error> {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("write:{:?}", id.map(|id| id.0)));
        s.current = id.map(|id| old(id.0));
        Ok(())
    }
    async fn record(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: &str) -> Result<(), Self::Error> {
        let mut s = self.0.lock().unwrap();
        s.calls.push(format!("record:{kind:?}:{}:{title}", id.0));
        if s.fail_record { Err("history failed") } else { Ok(()) }
    }
    fn publish(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: String) {
        self.0.lock().unwrap().calls.push(format!("publish:{kind:?}:{}:{title}", id.0));
    }
}
fn access(allowed: bool, archived: bool) -> MilestoneAttachmentAccess {
    MilestoneAttachmentAccess { can_triage: allowed, archived }
}
fn port(state: &Mutex<State>) -> Port<'_> {
    state.lock().unwrap().calls.push("view".into());
    Port(state)
}

#[tokio::test]
async fn denied_archived_and_unknown_paths_keep_original_factory_and_query_order() {
    for (allowed, archived) in [(false, true), (true, true)] {
        let result = MilestoneAssignment::new(access(allowed, archived))
            .set(Some(MilestoneId(-1)), || -> Port<'static> { panic!("store") }, || -> Result<(), ()> { panic!("spend") })
            .await;
        if allowed {
            assert!(matches!(result, Err(MilestoneAttachmentError::Archived)));
        } else {
            assert!(matches!(result, Err(MilestoneAttachmentError::Forbidden)));
        }
    }
    let state = Mutex::new(State::default());
    let result = MilestoneAssignment::new(access(true, false))
        .set(Some(MilestoneId(-1)), || port(&state), || -> Result<(), ()> { panic!("spend") })
        .await;
    assert!(matches!(result,Err(MilestoneAttachmentError::Invalid(message)) if message=="unknown milestone"));
    assert_eq!(state.lock().unwrap().calls, ["view", "find:-1"]);
}

#[tokio::test]
async fn same_and_empty_assignments_never_spend_write_or_record() {
    let state = Mutex::new(State { current: Some(old(1)), ..State::default() });
    let app = MilestoneAssignment::new(access(true, false));
    app.set(Some(MilestoneId(1)), || port(&state), || -> Result<(), ()> { panic!("spend") }).await.unwrap();
    assert_eq!(state.lock().unwrap().calls, ["view", "find:1", "current"]);
    state.lock().unwrap().current = None;
    state.lock().unwrap().calls.clear();
    app.set(None, || port(&state), || -> Result<(), ()> { panic!("spend") }).await.unwrap();
    assert_eq!(state.lock().unwrap().calls, ["view", "current"]);
}

#[tokio::test]
async fn replacements_write_first_then_remove_record_publish_then_add_record_publish() {
    let state = Mutex::new(State { current: Some(old(1)), ..State::default() });
    MilestoneAssignment::new(access(true, false))
        .set(
            Some(MilestoneId(2)),
            || port(&state),
            || {
                state.lock().unwrap().calls.push("spend".into());
                Ok::<_, ()>(())
            },
        )
        .await
        .unwrap();
    assert_eq!(
        state.lock().unwrap().calls,
        [
            "view",
            "find:2",
            "current",
            "spend",
            "write:Some(2)",
            "record:Removed:1:v1",
            "publish:Removed:1:v1",
            "record:Added:2:v2",
            "publish:Added:2:v2"
        ]
    );
}

#[tokio::test]
async fn throttle_and_history_failure_preserve_commit_boundaries_and_retry_cannot_repair() {
    let state = Mutex::new(State::default());
    let app = MilestoneAssignment::new(access(true, false));
    assert!(matches!(
        app.set(Some(MilestoneId(1)), || port(&state), || Err("limited")).await,
        Err(MilestoneAttachmentError::Throttle("limited"))
    ));
    assert!(state.lock().unwrap().current.is_none());
    state.lock().unwrap().fail_record = true;
    state.lock().unwrap().calls.clear();
    assert!(matches!(
        app.set(Some(MilestoneId(1)), || port(&state), || Ok::<_, ()>(())).await,
        Err(MilestoneAttachmentError::Port("history failed"))
    ));
    assert_eq!(state.lock().unwrap().current.as_ref().unwrap().id, MilestoneId(1));
    assert_eq!(state.lock().unwrap().calls, ["view", "find:1", "current", "write:Some(1)", "record:Added:1:v1"]);
    state.lock().unwrap().calls.clear();
    app.set(Some(MilestoneId(1)), || port(&state), || -> Result<(), ()> { panic!("noop spends nothing") }).await.unwrap();
    assert_eq!(state.lock().unwrap().calls, ["view", "find:1", "current"]);
}

#[tokio::test]
async fn replacement_keeps_pre_read_event_snapshot_when_another_assignment_intervenes() {
    let state = Mutex::new(State { current: Some(old(1)), ..State::default() });
    MilestoneAssignment::new(access(true, false))
        .set(
            Some(MilestoneId(2)),
            || port(&state),
            || {
                // Existing SQL's returned previous row is ignored: do not silently change
                // event semantics to this newer row without a separate concurrency design.
                state.lock().unwrap().current = Some(old(9));
                Ok::<_, ()>(())
            },
        )
        .await
        .unwrap();
    assert!(state.lock().unwrap().calls.contains(&"record:Removed:1:v1".to_owned()));
    assert!(!state.lock().unwrap().calls.contains(&"record:Removed:9:v9".to_owned()));
}
