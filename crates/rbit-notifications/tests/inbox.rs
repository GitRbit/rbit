mod support;
use rbit_contracts::{AccountId, Timestamp};
use rbit_notifications::{
    application::{InboxApplication, InboxError},
    signals,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use support::*;

#[tokio::test]
async fn page_batches_distinct_ids_deletes_hidden_then_counts_without_refilling() {
    let fake = Fake { rows: Mutex::new(vec![row(1, 9), row(2, 4), row(3, 9), row(4, 2)]), readable: vec![9], total: 8, ..Fake::default() };
    let page = InboxApplication::new(&fake, &fake, AccountId(7)).page(true, 0, 0).await.unwrap();
    assert_eq!(page.notifications.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 3]);
    assert_eq!((page.total, page.page, page.per_page, page.page_count()), (8, 1, 1, 8));
    assert_eq!(fake.trace(), ["list:7:true:1:0", "acl:[RepoId(2), RepoId(4), RepoId(9)]", "delete:7:[2, 4]", "count:7:true"]);
}
#[tokio::test]
async fn empty_and_extreme_pages_preserve_clamps_and_never_delete_visible_rows() {
    let fake = Fake::default();
    let page = InboxApplication::new(&fake, &fake, AccountId(7)).page(false, u32::MAX, u32::MAX).await.unwrap();
    let offset = u64::from(u32::MAX - 1) * u64::from(u32::MAX);
    assert_eq!(fake.trace(), [format!("list:7:false:{}:{offset}", u32::MAX), "acl:[]".into(), "count:7:false".into()]);
    assert_eq!(page.page_count(), 1);
}
#[tokio::test]
async fn failures_stop_before_cleanup_or_count_and_preserve_error_source() {
    for (fail, expected) in [
        ("list", InboxError::Store("fixture")),
        ("acl", InboxError::Access("fixture")),
        ("delete", InboxError::Store("fixture")),
        ("count", InboxError::Store("fixture")),
    ] {
        let fake = Fake { rows: Mutex::new(vec![row(1, 9)]), fail, ..Fake::default() };
        assert_eq!(InboxApplication::new(&fake, &fake, AccountId(7)).page(false, 1, 50).await.unwrap_err(), expected);
        let trace = fake.trace();
        assert!(trace.last().unwrap().starts_with(fail));
    }
}
#[tokio::test]
async fn find_refuses_foreign_or_hidden_rows_and_preserves_cleanup_error_precedence() {
    let missing = Fake { missing: true, ..Fake::default() };
    assert_eq!(InboxApplication::new(&missing, &missing, AccountId(7)).find(11).await, Err(InboxError::NotFound));
    assert_eq!(missing.trace(), ["find:7:11"]);
    let absent_repository = Fake { missing_repository: true, ..Fake::default() };
    assert_eq!(InboxApplication::new(&absent_repository, &absent_repository, AccountId(7)).find(11).await, Err(InboxError::NotFound));
    assert_eq!(absent_repository.trace(), ["find:7:11", "repository:4"]);
    for fail in ["", "delete"] {
        let denied = Fake { denied: true, fail, ..Fake::default() };
        let result = InboxApplication::new(&denied, &denied, AccountId(7)).find(11).await;
        assert_eq!(result, Err(if fail.is_empty() { InboxError::NotFound } else { InboxError::Store("fixture") }));
        assert_eq!(denied.trace(), ["find:7:11", "repository:4", "can-read", "delete:7:[11]"]);
    }
}
#[tokio::test]
async fn unsubscribe_keeps_visibility_before_clock_subscription_and_read_order() {
    let fake = Fake::default();
    InboxApplication::new(&fake, &fake, AccountId(7))
        .unsubscribe(11, || {
            fake.calls.lock().unwrap().push("clock".into());
            Timestamp(42)
        })
        .await
        .unwrap();
    assert_eq!(fake.trace(), ["find:7:11", "repository:4", "can-read", "clock", "subscribe:11:7:false:42", "read:7:11"]);
    let denied = Fake { denied: true, ..Fake::default() };
    assert!(InboxApplication::new(&denied, &denied, AccountId(7)).unsubscribe(11, || panic!("must not sample clock")).await.is_err());
}
#[tokio::test]
async fn disabled_badge_and_read_never_construct_store_but_thread_state_still_queries_once() {
    let builds = AtomicUsize::new(0);
    assert!(
        !signals::has_unread(false, AccountId(7), || {
            builds.fetch_add(1, Ordering::Relaxed);
            Fake::default()
        })
        .await
        .unwrap()
    );
    assert!(
        !signals::mark_thread_read(false, AccountId(7), rbit_collaboration::threads::IssueId(1), || {
            builds.fetch_add(1, Ordering::Relaxed);
            Fake::default()
        })
        .await
        .unwrap()
    );
    assert_eq!(builds.load(Ordering::Relaxed), 0);
    let fake = Fake::default();
    let (subscription, unread) = signals::thread_state(false, AccountId(7), rbit_collaboration::threads::IssueId(1), &fake).await.unwrap();
    assert!(subscription.is_some());
    assert!(!unread);
    assert_eq!(fake.trace(), ["state:1:7"]);
}
