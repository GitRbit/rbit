//! Adopted application behavior against the real registered SQLite adapters.
use super::*;
use rbit_core::Timestamp;
use rbit_storage::{Work, count_work};

async fn seed(fixture: &Fixture) -> rbit_core::IssueId {
    let repo = fixture.repo(&fixture.alice).await;
    let issue = fixture
        .services
        .issues
        .create(&fixture.alice, &repo, NewIssueInput { title: "Inbox <unsafe>".into(), ..NewIssueInput::default() })
        .await
        .unwrap();
    fixture
        .database
        .notifications()
        .notify(fixture.bob.id(), issue.id, repo.repo.repo.id, NotificationReason::Mention, None, Timestamp::now())
        .await
        .unwrap();
    issue.id
}
#[tokio::test]
async fn real_inbox_preserves_empty_and_visible_query_budgets_and_type_aliases() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let service = &fixture.services.notifications;
    let (page, work) = count_work(service.inbox_page(&fixture.bob, false, 0, 0)).await;
    let page = page.unwrap();
    assert_eq!((page.total, page.page, page.per_page), (0, 1, 1));
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 });
    seed(&fixture).await;
    let (page, work) = count_work(service.inbox_page(&fixture.bob, true, 1, 50)).await;
    let page = page.unwrap();
    assert_eq!((page.total, page.notifications.len()), (1, 1));
    assert_eq!(work, Work { acquisitions: 3, statements: 3, cache_writes: 0 });
    let row: rbit_notifications::Notification = page.notifications.into_iter().next().unwrap();
    let legacy: rbit_storage::sqlite::notifications::Notification = row;
    assert_eq!(legacy.title, "Inbox <unsafe>");
    let (result, work) = count_work(service.mark_read(&fixture.carol, legacy.id)).await;
    assert!(matches!(result, Err(rbit_services::ServiceError::NotFound)));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert!(matches!(service.find(&fixture.carol, legacy.id).await, Err(rbit_services::ServiceError::NotFound)));
}
#[tokio::test]
async fn hidden_rows_are_deleted_before_total_with_no_refill_or_extra_acl_queries() {
    let fixture = Fixture::new(Visibility::Private, |_| {}).await;
    seed(&fixture).await;
    let (page, work) = count_work(fixture.services.notifications.inbox(&fixture.bob, true, 1)).await;
    let page = page.unwrap();
    assert_eq!(page.notifications, Vec::new());
    assert_eq!(page.total, 0);
    assert_eq!(work, Work { acquisitions: 4, statements: 4, cache_writes: 0 });
    assert_eq!(fixture.database.notifications().count(fixture.bob.id(), false).await.unwrap(), 0);
}
#[tokio::test]
async fn disabled_badge_and_thread_read_are_zero_work_but_state_preserves_subscription_in_one_query() {
    let fixture = Fixture::new(Visibility::Public, |config| config.issues.notifications = false).await;
    let issue = seed(&fixture).await;
    fixture.services.notifications.set_subscribed(fixture.bob.id(), issue, false).await.unwrap();
    let (badge, work) = count_work(fixture.services.notifications.has_unread(fixture.bob.id())).await;
    assert!(!badge.unwrap());
    assert_eq!(work, Work::default());
    let (read, work) = count_work(fixture.services.notifications.mark_thread_read(fixture.bob.id(), issue)).await;
    assert!(!read);
    assert_eq!(work, Work::default());
    let (state, work) = count_work(fixture.services.notifications.thread_state(fixture.bob.id(), issue)).await;
    let (subscription, unread) = state.unwrap();
    assert!(!subscription.unwrap().subscribed);
    assert!(!unread);
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert!(fixture.database.notifications().has_unread(fixture.bob.id()).await.unwrap(), "disabled read did not consume unread state");
}

#[tokio::test]
async fn concrete_store_debug_is_opaque_and_performs_no_storage_work() {
    let fixture = Fixture::new(Visibility::Public, |_| {}).await;
    let store = rbit_storage::NotificationInboxStore::new(fixture.database.notifications());
    let ((ordinary, pretty), work) = count_work(async { (format!("{store:?}"), format!("{store:#?}")) }).await;
    assert_eq!(ordinary, "NotificationInboxStore { .. }");
    assert_eq!(pretty, "NotificationInboxStore { .. }");
    assert_eq!(work, Work::default());
}
