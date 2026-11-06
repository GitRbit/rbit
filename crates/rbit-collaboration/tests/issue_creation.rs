#![cfg(feature = "applications")]
//! Independent metadata ordering and explicit existing post-row cancellation gaps.

#[path = "support/thread_fixtures.rs"]
mod fixtures;

use rbit_collaboration::{
    IssueState,
    issues::{IssueCreation, IssueCreationAccess, IssueCreationError, IssueCreationPort, MetadataCreated, NewIssueInput},
    labels::{Label, LabelId},
    milestones::{Milestone, MilestoneId},
    threads::{Issue, IssueId},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::User;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

#[derive(Default)]
struct Bridge {
    calls: Mutex<Vec<&'static str>>,
    attached: Mutex<Vec<LabelId>>,
    row_committed: AtomicBool,
    fail_record: bool,
    pause_attach: bool,
    attaching: Notify,
    release: Notify,
}
impl Bridge {
    fn log(&self, value: &'static str) {
        self.calls.lock().unwrap().push(value);
    }
}

impl IssueCreationPort for Bridge {
    type Error = &'static str;
    async fn labels(&self, ids: &[LabelId]) -> Result<Vec<Label>, Self::Error> {
        self.log("labels");
        if ids.iter().any(|id| id.0 < 0) {
            return Err("unknown label");
        }
        Ok(ids
            .iter()
            .map(|id| Label {
                id: *id,
                repo_id: RepoId(3),
                name: format!("l{}", id.0),
                color: "#aabbcc".into(),
                description: String::new(),
                created_at: Timestamp(1),
                updated_at: Timestamp(1),
            })
            .collect())
    }
    async fn assignees(&self, _: &[AccountId]) -> Result<Vec<User>, Self::Error> {
        self.log("assignees");
        Ok(Vec::new())
    }
    async fn milestone(&self, id: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        self.log("milestone");
        if id.0 < 0 {
            return Ok(None);
        }
        Ok(Some(Milestone {
            id,
            repo_id: RepoId(3),
            title: "v1".into(),
            description: String::new(),
            due_on: None,
            state: IssueState::Open,
            closed_at: None,
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        }))
    }
    async fn create_thread(&self, _: &str, _: &str) -> Result<Issue, Self::Error> {
        self.log("row");
        self.row_committed.store(true, Ordering::SeqCst);
        Ok(fixtures::issue())
    }
    async fn attach(&self, _: IssueId, labels: &[LabelId], _: &[AccountId], _: Option<MilestoneId>) -> Result<(), Self::Error> {
        self.log("attach");
        self.attached.lock().unwrap().extend_from_slice(labels);
        if self.pause_attach {
            self.attaching.notify_one();
            self.release.notified().await;
        }
        Ok(())
    }
    async fn record(&self, _: IssueId, event: MetadataCreated) -> Result<(), Self::Error> {
        self.log(match event {
            MetadataCreated::Labeled(_) => "labeled",
            MetadataCreated::Assigned(_) => "assigned",
            MetadataCreated::Milestoned { .. } => "milestoned",
        });
        if self.fail_record { Err("timeline failed") } else { Ok(()) }
    }
    fn publish_opened(&self, _: &Issue) {
        self.log("opened");
    }
}
fn access(triage: bool) -> IssueCreationAccess {
    IssueCreationAccess { can_read: true, can_triage: triage, archived: false }
}
fn input() -> NewIssueInput {
    NewIssueInput {
        title: "title".into(),
        labels: vec![LabelId(2), LabelId(1), LabelId(2)],
        milestone: Some(MilestoneId(1)),
        ..NewIssueInput::default()
    }
}

#[tokio::test]
async fn metadata_dedup_order_and_single_opened_publication_match_legacy_workflow() {
    let bridge = Bridge::default();
    IssueCreation::new(&bridge, access(true)).create(input()).await.unwrap();
    assert_eq!(*bridge.attached.lock().unwrap(), [LabelId(2), LabelId(1)]);
    assert_eq!(
        *bridge.calls.lock().unwrap(),
        ["labels", "assignees", "milestone", "row", "attach", "labeled", "labeled", "milestoned", "opened"]
    );
}

#[tokio::test]
async fn readers_ignore_invalid_metadata_and_denied_or_archived_calls_touch_no_ports() {
    let bridge = Bridge::default();
    let mut invalid = input();
    invalid.labels = vec![LabelId(-1)];
    invalid.milestone = Some(MilestoneId(-1));
    IssueCreation::new(&bridge, access(false)).create(invalid.clone()).await.unwrap();
    assert_eq!(*bridge.calls.lock().unwrap(), ["row", "opened"]);
    bridge.calls.lock().unwrap().clear();
    let denied = IssueCreationAccess { can_read: false, archived: true, ..access(true) };
    assert!(matches!(IssueCreation::new(&bridge, denied).create(invalid.clone()).await, Err(IssueCreationError::NotFound)));
    assert!(matches!(
        IssueCreation::new(&bridge, IssueCreationAccess { archived: true, ..access(true) }).create(invalid).await,
        Err(IssueCreationError::Archived)
    ));
    assert_eq!(*bridge.calls.lock().unwrap(), Vec::<&str>::new());
}

#[tokio::test]
async fn metadata_errors_and_limits_precede_row_commit_and_timeline_failure_does_not_publish_opened() {
    let bridge = Bridge::default();
    let mut many = input();
    many.labels = (0..26).map(LabelId).collect();
    assert!(
        matches!(IssueCreation::new(&bridge,access(true)).create(many).await,Err(IssueCreationError::Invalid(message)) if message=="an issue can have at most 25 labels")
    );
    assert_eq!(*bridge.calls.lock().unwrap(), ["labels"]);
    bridge.calls.lock().unwrap().clear();
    let mut missing = input();
    missing.milestone = Some(MilestoneId(-1));
    assert!(
        matches!(IssueCreation::new(&bridge,access(true)).create(missing).await,Err(IssueCreationError::Invalid(message)) if message=="unknown milestone")
    );
    assert!(!bridge.row_committed.load(Ordering::SeqCst));
    let failed = Bridge { fail_record: true, ..Bridge::default() };
    assert!(matches!(IssueCreation::new(&failed, access(true)).create(input()).await, Err(IssueCreationError::Port("timeline failed"))));
    assert!(failed.row_committed.load(Ordering::SeqCst));
    assert_eq!(*failed.calls.lock().unwrap(), ["labels", "assignees", "milestone", "row", "attach", "labeled"]);
}

#[tokio::test]
async fn cancellation_after_row_commit_retains_existing_partial_commit_without_inventing_durable_effects() {
    let bridge = Arc::new(Bridge { pause_attach: true, ..Bridge::default() });
    let owned = Arc::clone(&bridge);
    let worker = tokio::spawn(async move { IssueCreation::new(&*owned, access(true)).create(input()).await });
    bridge.attaching.notified().await;
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
    assert!(bridge.row_committed.load(Ordering::SeqCst));
    assert_eq!(*bridge.calls.lock().unwrap(), ["labels", "assignees", "milestone", "row", "attach"]);
}
