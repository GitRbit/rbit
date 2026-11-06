#![cfg(feature = "applications")]
//! Policy ordering and preservation of list-with-progress lookup contracts.

use std::sync::Mutex;

use rbit_collaboration::{
    IssueState,
    milestones::{
        Milestone, MilestoneAccess, MilestoneApplication, MilestoneError, MilestoneFields, MilestoneId, MilestoneInput, MilestoneProgress,
        MilestoneRepository,
    },
};
use rbit_contracts::{RepoId, Timestamp};

#[derive(Default)]
struct Catalog {
    calls: Mutex<Vec<&'static str>>,
    fail_create: bool,
}

impl Catalog {
    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
    fn milestone() -> Milestone {
        Milestone {
            id: MilestoneId(7),
            repo_id: RepoId(3),
            title: "v1".into(),
            description: String::new(),
            due_on: None,
            state: IssueState::Open,
            closed_at: None,
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        }
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl MilestoneRepository for Catalog {
    type Error = &'static str;
    async fn milestones(&self, _: RepoId, state: Option<IssueState>) -> Result<Vec<MilestoneProgress>, Self::Error> {
        self.record("list");
        assert!(state.is_none() || state == Some(IssueState::Closed));
        Ok(vec![MilestoneProgress { milestone: Self::milestone(), open_issues: 4, closed_issues: 2 }])
    }
    async fn milestone_counts(&self, _: RepoId) -> Result<(u64, u64), Self::Error> {
        self.record("counts");
        Ok((2, 3))
    }
    async fn find_milestone_by_title(&self, _: RepoId, title: &str) -> Result<Option<Milestone>, Self::Error> {
        self.record("titled");
        assert_eq!(title, " V1 ", "no new trimming before SQL lookup");
        Ok(None)
    }
    async fn create_milestone(&self, _: RepoId, fields: &MilestoneFields, at: Timestamp) -> Result<Milestone, Self::Error> {
        self.record("create");
        assert_eq!(fields.title, "v1");
        assert_eq!(at, Timestamp(9));
        if self.fail_create { Err("duplicate") } else { Ok(Self::milestone()) }
    }
    async fn update_milestone(
        &self,
        _: RepoId,
        _: MilestoneId,
        _: &MilestoneFields,
        _: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        self.record("update");
        Ok(None)
    }
    async fn set_milestone_state(
        &self,
        _: RepoId,
        _: MilestoneId,
        state: IssueState,
        _: Timestamp,
    ) -> Result<Option<Milestone>, Self::Error> {
        self.record("state");
        assert_eq!(state, IssueState::Closed);
        Ok(None)
    }
    async fn delete_milestone(&self, _: RepoId, _: MilestoneId) -> Result<Option<Milestone>, Self::Error> {
        self.record("delete");
        Ok(None)
    }
}

fn access(can_write: bool, archived: bool) -> MilestoneAccess {
    MilestoneAccess { repo: RepoId(3), can_write, archived }
}
fn input() -> MilestoneInput {
    MilestoneInput { title: " v1 ".into(), ..MilestoneInput::default() }
}

#[tokio::test]
async fn denied_and_archived_writes_do_not_validate_throttle_read_or_consult_clock() {
    let store = Catalog::default();
    for (writer, archived) in [(false, true), (true, true)] {
        let app = MilestoneApplication::new(&store, access(writer, archived));
        let result = app.create(&MilestoneInput::default(), || -> Result<(), ()> { panic!("throttle") }, || panic!("clock")).await;
        if writer {
            assert!(matches!(result, Err(MilestoneError::Archived)));
        } else {
            assert!(matches!(result, Err(MilestoneError::Forbidden)));
        }
        assert!(app.delete(MilestoneId(7)).await.is_err());
        assert!(app.set_state(MilestoneId(7), IssueState::Closed, || panic!("clock")).await.is_err());
    }
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
}

#[tokio::test]
async fn validation_throttle_clock_and_sql_error_order_is_preserved_without_preflight_reads() {
    let store = Catalog { fail_create: true, ..Catalog::default() };
    let app = MilestoneApplication::new(&store, access(true, false));
    assert!(matches!(
        app.create(&MilestoneInput::default(), || -> Result<(), ()> { panic!("throttle") }, || panic!("clock")).await,
        Err(MilestoneError::Invalid(_))
    ));
    assert!(matches!(app.create(&input(), || Err("limited"), || panic!("clock")).await, Err(MilestoneError::Throttle("limited"))));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
    let result = app
        .create(
            &input(),
            || {
                store.record("spend");
                Ok::<_, ()>(())
            },
            || {
                store.record("clock");
                Timestamp(9)
            },
        )
        .await;
    assert!(matches!(result, Err(MilestoneError::Store("duplicate"))));
    assert_eq!(*store.calls.lock().unwrap(), ["spend", "clock", "create"]);
}

#[tokio::test]
async fn id_lookup_keeps_list_progress_and_missing_mutations_keep_single_store_calls() {
    let store = Catalog::default();
    let app = MilestoneApplication::new(&store, access(true, false));
    let progress = app.find(MilestoneId(7)).await.unwrap();
    assert_eq!((progress.open_issues, progress.closed_issues), (4, 2));
    assert!(matches!(app.find(MilestoneId(8)).await, Err(MilestoneError::NotFound)));
    assert!(matches!(app.titled(" V1 ").await, Err(MilestoneError::NotFound)));
    assert_eq!(app.counts().await.unwrap(), (2, 3));
    app.list(Some(IssueState::Closed)).await.unwrap();
    assert!(matches!(
        app.update(
            MilestoneId(8),
            &input(),
            || {
                store.record("spend");
                Ok::<_, ()>(())
            },
            || Timestamp(9)
        )
        .await,
        Err(MilestoneError::NotFound)
    ));
    assert!(matches!(app.set_state(MilestoneId(8), IssueState::Closed, || Timestamp(9)).await, Err(MilestoneError::NotFound)));
    assert!(matches!(app.delete(MilestoneId(8)).await, Err(MilestoneError::NotFound)));
    assert_eq!(*store.calls.lock().unwrap(), ["list", "list", "titled", "counts", "list", "spend", "update", "state", "delete"]);
}
