#![cfg(feature = "applications")]
//! Adversarial policy sequencing without runtime tasks or concrete persistence.

use std::sync::Mutex;

use rbit_collaboration::labels::{
    Label, LabelAccess, LabelApplication, LabelError, LabelFields, LabelId, LabelInput, LabelRepository, LabelUsage,
};
use rbit_contracts::{RepoId, Timestamp};

#[derive(Default)]
struct Catalog {
    calls: Mutex<Vec<&'static str>>,
    count: u64,
    fail_create: bool,
}

impl Catalog {
    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }

    fn label() -> Label {
        Label {
            id: LabelId(7),
            repo_id: RepoId(3),
            name: "Bug".into(),
            color: "#aabbcc".into(),
            description: String::new(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        }
    }
}

impl LabelRepository for Catalog {
    type Error = &'static str;

    async fn labels(&self, _: RepoId) -> Result<Vec<Label>, Self::Error> {
        self.record("list");
        Ok(vec![Self::label()])
    }
    async fn label_usage(&self, _: RepoId, kind: &str) -> Result<Vec<LabelUsage>, Self::Error> {
        assert_eq!(kind, "issue");
        self.record("usage");
        Ok(Vec::new())
    }
    async fn find_label_by_name(&self, _: RepoId, _: &str) -> Result<Option<Label>, Self::Error> {
        self.record("named");
        Ok(None)
    }
    async fn labels_named(&self, _: RepoId, _: &[String]) -> Result<Vec<Label>, Self::Error> {
        self.record("named_many");
        Ok(vec![Self::label()])
    }
    async fn count_labels(&self, _: RepoId) -> Result<u64, Self::Error> {
        self.record("count");
        Ok(self.count)
    }
    async fn create_label(&self, _: RepoId, fields: &LabelFields, at: Timestamp) -> Result<Label, Self::Error> {
        assert_eq!(fields.name, "bug");
        assert_eq!(fields.color, "#aabbcc");
        assert_eq!(at, Timestamp(9));
        self.record("create");
        if self.fail_create { Err("duplicate") } else { Ok(Self::label()) }
    }
    async fn update_label(&self, _: RepoId, _: LabelId, _: &LabelFields, _: Timestamp) -> Result<Option<Label>, Self::Error> {
        self.record("update");
        Ok(None)
    }
    async fn delete_label(&self, _: RepoId, _: LabelId) -> Result<Option<Label>, Self::Error> {
        self.record("delete");
        Ok(None)
    }
    async fn create_missing_labels(&self, _: RepoId, labels: &[LabelFields], _: Timestamp) -> Result<u64, Self::Error> {
        assert_eq!(labels.len(), 9);
        self.record("defaults");
        Ok(8)
    }
}

fn input() -> LabelInput {
    LabelInput { name: " bug ".into(), color: " AABBCC ".into(), description: " note ".into() }
}

fn access(can_write: bool, archived: bool) -> LabelAccess {
    LabelAccess { repo: RepoId(3), can_write, archived }
}

#[tokio::test]
async fn authorization_archive_and_validation_precede_count_throttle_and_clock() {
    let store = Catalog::default();
    for (can_write, archived) in [(false, true), (true, true)] {
        let result = LabelApplication::new(&store, access(can_write, archived))
            .create(&LabelInput::default(), || -> Result<(), ()> { panic!("throttle") }, || panic!("clock"))
            .await;
        if can_write {
            assert!(matches!(result, Err(LabelError::Archived)));
        } else {
            assert!(matches!(result, Err(LabelError::Forbidden)));
        }
    }
    let result = LabelApplication::new(&store, access(true, false))
        .create(&LabelInput::default(), || -> Result<(), ()> { panic!("throttle") }, || panic!("clock"))
        .await;
    assert!(matches!(result, Err(LabelError::Invalid(message)) if message == "label name must not be empty"));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
}

#[tokio::test]
async fn create_count_limit_throttle_and_sql_errors_keep_their_original_precedence() {
    let full = Catalog { count: 200, ..Catalog::default() };
    let result = LabelApplication::new(&full, access(true, false))
        .create(&input(), || -> Result<(), ()> { panic!("throttle") }, || panic!("clock"))
        .await;
    assert!(matches!(result, Err(LabelError::Invalid(_))));
    assert_eq!(*full.calls.lock().unwrap(), ["count"]);

    let store = Catalog { fail_create: true, ..Catalog::default() };
    let app = LabelApplication::new(&store, access(true, false));
    assert!(matches!(app.create(&input(), || Err("limited"), || panic!("clock")).await, Err(LabelError::Throttle("limited"))));
    assert_eq!(*store.calls.lock().unwrap(), ["count"]);
    store.calls.lock().unwrap().clear();
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
    assert!(matches!(result, Err(LabelError::Store("duplicate"))));
    assert_eq!(*store.calls.lock().unwrap(), ["count", "spend", "clock", "create"]);
}

#[tokio::test]
async fn defaults_are_conditional_and_keep_storage_insert_count_without_a_second_count() {
    let store = Catalog { count: 1, ..Catalog::default() };
    let result = LabelApplication::new(&store, access(true, false))
        .seed_defaults(|| -> Result<(), ()> { panic!("throttle") }, || panic!("clock"))
        .await;
    assert!(matches!(result, Ok(0)));
    assert_eq!(*store.calls.lock().unwrap(), ["count"]);
    let empty = Catalog::default();
    let result = LabelApplication::new(&empty, access(true, false)).seed_defaults(|| Ok::<_, ()>(()), || Timestamp(9)).await;
    assert!(matches!(result, Ok(8)), "concurrent defaults may already exist at insertion");
    assert_eq!(*empty.calls.lock().unwrap(), ["count", "defaults"]);
}

#[tokio::test]
async fn update_missing_is_after_throttle_delete_is_one_operation_and_reads_keep_ascii_matching() {
    let store = Catalog::default();
    let app = LabelApplication::new(&store, access(true, false));
    assert!(matches!(
        app.update(
            LabelId(8),
            &input(),
            || {
                store.record("spend");
                Ok::<_, ()>(())
            },
            || Timestamp(9)
        )
        .await,
        Err(LabelError::NotFound)
    ));
    assert!(matches!(app.delete(LabelId(8)).await, Err(LabelError::NotFound)));
    assert_eq!(*store.calls.lock().unwrap(), ["spend", "update", "delete"]);
    assert_eq!(app.named_many(&["BUG".into(), "Bug".into()]).await.unwrap().len(), 1);
    assert!(
        matches!(app.named_many(&["BUG".into(), "missing".into()]).await, Err(LabelError::Invalid(message)) if message == "there is no label called “missing”")
    );
    assert!(matches!(app.named("unknown").await, Err(LabelError::NotFound)));
    app.list().await.unwrap();
    app.usage().await.unwrap();
}
