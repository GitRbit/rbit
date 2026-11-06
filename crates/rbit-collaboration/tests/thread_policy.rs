#![cfg(feature = "applications")]
//! Row creation/read sequencing without concrete stores or cross-feature services.

#[path = "support/thread_fixtures.rs"]
mod fixtures;

use rbit_collaboration::threads::{
    Issue, IssueKind, IssueNumber, NewIssue, ThreadAccess, ThreadApplication, ThreadDraft, ThreadError, ThreadRepository,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::Account;
use std::sync::Mutex;

#[derive(Default)]
struct Store {
    calls: Mutex<Vec<&'static str>>,
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ThreadRepository for Store {
    type Error = &'static str;
    async fn create(&self, new: &NewIssue) -> Result<Issue, Self::Error> {
        self.calls.lock().unwrap().push("create");
        assert_eq!(new.title.as_str(), "title");
        assert_eq!(new.body.as_str(), "a\nb");
        assert_eq!(new.at, Timestamp(9));
        Err("commit failed")
    }
    async fn find_by_number_with_author(&self, _: RepoId, _: IssueNumber) -> Result<Option<(Issue, Option<Account>)>, Self::Error> {
        self.calls.lock().unwrap().push("open");
        Ok(Some((fixtures::issue(), None)))
    }
    async fn open_counts(&self, _: RepoId) -> Result<(u64, u64), Self::Error> {
        self.calls.lock().unwrap().push("counts");
        Ok((2, 3))
    }
}

fn draft() -> ThreadDraft<'static> {
    ThreadDraft { author: AccountId(4), kind: IssueKind::Issue, title: " title ", body: "a\r\nb" }
}

#[tokio::test]
async fn visibility_archive_text_throttle_and_clock_keep_precedence_and_sql_errors() {
    let store = Store::default();
    for (read, archived) in [(false, true), (true, true)] {
        let app = ThreadApplication::new(&store, ThreadAccess { repo: RepoId(3), can_read: read, archived });
        let result = app.create(draft(), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
        if read {
            assert!(matches!(result, Err(ThreadError::Archived)));
        } else {
            assert!(matches!(result, Err(ThreadError::NotFound)));
        }
    }
    let app = ThreadApplication::new(&store, ThreadAccess { repo: RepoId(3), can_read: true, archived: false });
    let invalid = ThreadDraft { title: "", ..draft() };
    assert!(matches!(app.create(invalid, || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await, Err(ThreadError::Text(_))));
    assert!(matches!(app.create(draft(), || Err("limited"), || panic!("clock")).await, Err(ThreadError::Throttle("limited"))));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
    let result = app
        .create(
            draft(),
            || {
                store.calls.lock().unwrap().push("spend");
                Ok::<_, ()>(())
            },
            || {
                store.calls.lock().unwrap().push("clock");
                Timestamp(9)
            },
        )
        .await;
    assert!(matches!(result, Err(ThreadError::Store("commit failed"))));
    assert_eq!(*store.calls.lock().unwrap(), ["spend", "clock", "create"]);
}

#[tokio::test]
async fn archived_reads_join_author_once_and_counts_keep_their_existing_caller_gate() {
    let store = Store::default();
    let denied = ThreadApplication::new(&store, ThreadAccess { repo: RepoId(3), can_read: false, archived: true });
    assert!(matches!(denied.open(IssueNumber::new(1).unwrap()).await, Err(ThreadError::NotFound)));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
    let counts = denied.counts().await.unwrap();
    assert_eq!((counts.issues, counts.pulls), (2, 3));
    let app = ThreadApplication::new(&store, ThreadAccess { repo: RepoId(3), can_read: true, archived: true });
    let (_, author) = app.open(IssueNumber::new(1).unwrap()).await.unwrap();
    assert!(author.is_none());
    assert_eq!(*store.calls.lock().unwrap(), ["counts", "open"]);
}
