#![cfg(feature = "applications")]
//! Independent edit ordering, validation, no-op and stale-context event intent.

#[path = "support/thread_fixtures.rs"]
mod fixtures;

use rbit_collaboration::threads::{
    Issue, IssueId, IssueText, IssueTitle, ThreadEditAccess, ThreadEditError, ThreadEditor, ThreadTextRepository,
};
use rbit_contracts::{AccountId, Timestamp};
use std::sync::Mutex;

struct Store {
    calls: Mutex<Vec<&'static str>>,
    result: Option<Issue>,
    fail: bool,
}
impl Store {
    fn new(result: Option<Issue>) -> Self {
        Self { calls: Mutex::new(Vec::new()), result, fail: false }
    }
    fn spend(&self) {
        self.calls.lock().unwrap().push("spend");
    }
    fn now(&self) -> Timestamp {
        self.calls.lock().unwrap().push("clock");
        Timestamp(9)
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ThreadTextRepository for Store {
    type Error = &'static str;
    async fn update_text(
        &self,
        id: IssueId,
        title: Option<&IssueTitle>,
        body: Option<&IssueText>,
        actor: AccountId,
        at: Timestamp,
    ) -> Result<Option<Issue>, Self::Error> {
        self.calls.lock().unwrap().push("store");
        assert_eq!(id, IssueId(1));
        assert_eq!(actor, AccountId(4));
        assert_eq!(at, Timestamp(9));
        if let Some(title) = title {
            assert_eq!(title.as_str(), "updated");
        }
        if let Some(body) = body {
            assert_eq!(body.as_str(), "a\nb");
        }
        if self.fail { Err("write failed") } else { Ok(self.result.clone()) }
    }
}
fn access(can_edit: bool, archived: bool) -> ThreadEditAccess {
    ThreadEditAccess { can_edit, archived }
}

#[tokio::test]
async fn permissions_archive_title_body_and_throttle_win_in_original_order() {
    let store = Store::new(Some(fixtures::issue()));
    let issue = fixtures::issue();
    for (allowed, archived) in [(false, true), (true, true)] {
        let editor = ThreadEditor::new(&store, AccountId(4), access(allowed, archived));
        let result =
            editor.edit(&issue, Some(""), Some(&"x".repeat(65537)), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
        if allowed {
            assert!(matches!(result, Err(ThreadEditError::Archived)));
        } else {
            assert!(matches!(result, Err(ThreadEditError::Forbidden)));
        }
    }
    let editor = ThreadEditor::new(&store, AccountId(4), access(true, false));
    let result =
        editor.edit(&issue, Some(""), Some(&"x".repeat(65537)), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
    assert!(matches!(result,Err(ThreadEditError::Text(error)) if error.to_string()=="title must not be empty"));
    let result = editor.edit(&issue, None, Some(&"x".repeat(65537)), || -> Result<(), ()> { panic!("spend") }, || panic!("clock")).await;
    assert!(matches!(result,Err(ThreadEditError::Text(error)) if error.to_string()=="description must be at most 65536 bytes"));
    assert!(matches!(
        editor.edit(&issue, None, None, || Err("limited"), || panic!("clock")).await,
        Err(ThreadEditError::Throttle("limited"))
    ));
    assert_eq!(*store.calls.lock().unwrap(), Vec::<&str>::new());
}

#[tokio::test]
async fn unchanged_request_still_spends_and_calls_store_but_returns_no_event_intent() {
    let issue = fixtures::issue();
    let store = Store::new(Some(issue.clone()));
    let editor = ThreadEditor::new(&store, AccountId(4), access(true, false));
    let result = editor
        .edit(
            &issue,
            None,
            None,
            || {
                store.spend();
                Ok::<_, ()>(())
            },
            || store.now(),
        )
        .await
        .unwrap();
    assert_eq!(result.issue, issue);
    assert!(result.change.is_none());
    assert_eq!(*store.calls.lock().unwrap(), ["spend", "clock", "store"]);
}

#[tokio::test]
async fn changed_and_stale_contexts_retain_original_previous_values_instead_of_new_preflight_reads() {
    let issue = fixtures::issue();
    let mut updated = issue.clone();
    updated.title = IssueTitle::parse("updated").unwrap();
    updated.body = IssueText::body("a\nb").unwrap();
    let store = Store::new(Some(updated.clone()));
    let editor = ThreadEditor::new(&store, AccountId(4), access(true, false));
    let result = editor
        .edit(
            &issue,
            Some(" updated "),
            Some("a\r\nb"),
            || {
                store.spend();
                Ok::<_, ()>(())
            },
            || store.now(),
        )
        .await
        .unwrap();
    let change = result.change.unwrap();
    assert_eq!(change.previous_title.as_deref(), Some("title"));
    assert_eq!(change.previous_body.as_deref(), Some(""));
    let stale = editor
        .edit(
            &issue,
            None,
            None,
            || {
                store.spend();
                Ok::<_, ()>(())
            },
            || store.now(),
        )
        .await
        .unwrap();
    assert_eq!(stale.change, Some(change));
    assert_eq!(stale.issue, updated);
}

#[tokio::test]
async fn missing_and_failed_store_outcomes_come_after_throttle_without_success_intent() {
    let issue = fixtures::issue();
    let mut store = Store::new(None);
    let result = ThreadEditor::new(&store, AccountId(4), access(true, false))
        .edit(
            &issue,
            None,
            None,
            || {
                store.spend();
                Ok::<_, ()>(())
            },
            || store.now(),
        )
        .await;
    assert!(matches!(result, Err(ThreadEditError::NotFound)));
    assert_eq!(*store.calls.lock().unwrap(), ["spend", "clock", "store"]);
    store.fail = true;
    let result = ThreadEditor::new(&store, AccountId(4), access(true, false))
        .edit(
            &issue,
            None,
            None,
            || {
                store.spend();
                Ok::<_, ()>(())
            },
            || store.now(),
        )
        .await;
    assert!(matches!(result, Err(ThreadEditError::Store("write failed"))));
}
