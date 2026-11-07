//! Shared issue fixtures with schema faults installed before final pool construction.
use super::{
    Arc, Config, CreateRepository, Database, IssueContext, IssueNumber, KvStore, LabelInput, NewIssueInput, RepoContext, Services,
    TimelineEntry, TimelineEvent, User, Visibility, user,
};

pub(super) struct Fixture {
    pub(super) directory: tempfile::TempDir,
    pub(super) database: Database,
    pub(super) services: Services,
    pub(super) alice: User,
    pub(super) bob: User,
    pub(super) carol: User,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        Self::with(|_| {}).await
    }

    pub(super) async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        Self::configured(configure, None).await
    }

    pub(super) async fn with_fault_schema(schema: &'static str) -> Self {
        Self::configured(|_| {}, Some(schema)).await
    }

    async fn configured(configure: impl FnOnce(&mut Config), schema: Option<&'static str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::fault_schema::open(&dir.path().join("rbit.db"), schema).await;
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        // These tests make many changes in a row; throttling has its own test below.
        config.security.rate_limit = false;
        configure(&mut config);
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let alice = user(&database, "alice").await;
        let bob = user(&database, "bob").await;
        let carol = user(&database, "carol").await;
        let input = CreateRepository { name: "demo".to_owned(), description: String::new(), visibility: Visibility::Public };
        services.repositories.create(&alice, input).await.unwrap();
        Self { directory: dir, database, services, alice, bob, carol }
    }

    pub(super) async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "alice", "demo").await.unwrap()
    }

    pub(super) async fn create(&self, author: &User, title: &str, body: &str) -> IssueNumber {
        let input = NewIssueInput { title: title.to_owned(), body: body.to_owned(), ..NewIssueInput::default() };
        self.services.issues.create(author, &self.repo(author).await, input).await.unwrap().number
    }

    pub(super) async fn open(&self, viewer: &User, number: IssueNumber) -> IssueContext {
        self.services.timeline.open(Some(viewer), self.repo(viewer).await, number).await.unwrap()
    }

    pub(super) async fn label(&self, name: &str) -> rbit_core::LabelId {
        let input = LabelInput { name: name.to_owned(), color: "#aabbcc".to_owned(), description: String::new() };
        self.services.issues.create_label(&self.alice, &self.repo(&self.alice).await, &input).await.unwrap().id
    }

    pub(super) async fn events_of(&self, viewer: &User, number: IssueNumber) -> Vec<TimelineEvent> {
        let ctx = self.open(viewer, number).await;
        let timeline = self.services.timeline.timeline(Some(viewer), &ctx, None, 200).await.unwrap();
        timeline
            .entries
            .into_iter()
            .filter_map(|entry| if let TimelineEntry::Event { event, .. } = entry { Some(event) } else { None })
            .collect()
    }
}
