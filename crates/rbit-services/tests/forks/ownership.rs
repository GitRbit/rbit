//! Real service SQL budgets and event/privacy/error behavior through the feature port.
use super::*;
use rbit_core::RepoAccess;
use rbit_storage::{Work, count_work};
use tokio::sync::broadcast::error::TryRecvError;

#[tokio::test]
async fn read_owner_name_validation_preserves_early_work_and_error_precedence() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let mut parent = fixture.parent(&alice, Visibility::Public).await;
    parent.access = RepoAccess::None;
    let (result, work) =
        count_work(fixture.services.forks.fork(&bob, &parent, ForkInput { owner: "missing".into(), name: "bad name".into() })).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work, Work::default());
    parent.access = RepoAccess::Read;
    let (result, work) =
        count_work(fixture.services.forks.fork(&bob, &parent, ForkInput { owner: "missing".into(), name: "bad name".into() })).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))));
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let (result, work) =
        count_work(fixture.services.forks.fork(&bob, &parent, ForkInput { owner: String::new(), name: "bad name".into() })).await;
    assert!(result.is_err());
    assert_eq!(work, Work::default());
}
#[tokio::test]
async fn real_fork_and_listing_keep_sql_counts_and_only_repository_forked_event() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Public).await;
    fixture.database.repositories().mark_pushed(parent.repo.repo.id).await.unwrap();
    let source = fixture.services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.forks.fork(&bob, &source, ForkInput::default())).await;
    let fork = result.unwrap();
    assert_eq!(work, Work { acquisitions: 3, statements: 3, cache_writes: 0 });
    assert_eq!(
        events.recv().await.unwrap(),
        DomainEvent::RepositoryForked { repo: fork.repo.id, parent: parent.repo.repo.id, actor: bob.id() }
    );
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
    let (result, work) = count_work(fixture.services.forks.page(None, &parent, 1, 0)).await;
    let (rows, total) = result.unwrap();
    assert_eq!((rows.len(), total), (1, 1));
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 });
    let opened = fixture.services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    let (result, work) = count_work(fixture.services.forks.parent(None, &opened)).await;
    assert!(result.unwrap().is_some());
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let (result, work) = count_work(fixture.services.forks.forks_among(&[])).await;
    assert_eq!(result.unwrap().len(), 0);
    assert_eq!(work, Work::default());
}
#[tokio::test]
async fn disk_failure_deletes_metadata_and_never_publishes_an_event() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let mut parent = fixture.parent(&alice, Visibility::Public).await;
    parent.disk_path = parent.disk_path.join("missing-source");
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.forks.fork(&bob, &parent, ForkInput::default())).await;
    assert!(result.is_err());
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 });
    assert!(fixture.database.repositories().find_by_path("bob", "demo").await.unwrap().is_none());
    assert!(matches!(events.try_recv(), Err(TryRecvError::Empty)));
}
#[tokio::test]
async fn concurrent_unique_race_has_one_copy_winner_and_private_lists_stay_filtered() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let parent = fixture.parent(&alice, Visibility::Private).await;
    fixture.database.repositories().grant_collaborator(parent.repo.repo.id, bob.id(), rbit_core::Permission::Read).await.unwrap();
    let as_bob = fixture.services.repositories.open(Some(&bob), "alice", "demo").await.unwrap();
    let (results, work) = count_work(async {
        tokio::join!(
            fixture.services.forks.fork(&bob, &as_bob, ForkInput::default()),
            fixture.services.forks.fork(&bob, &as_bob, ForkInput::default())
        )
    })
    .await;
    assert!(matches!(results, (Ok(_), Err(ServiceError::Conflict(_))) | (Err(ServiceError::Conflict(_)), Ok(_))));
    assert_eq!(work, Work { acquisitions: 3, statements: 3, cache_writes: 0 });
    let (result, work) = count_work(fixture.services.forks.page(None, &parent, 100, 0)).await;
    let (rows, total) = result.unwrap();
    assert_eq!((rows.len(), total), (0, 0));
    assert_eq!(work, Work { acquisitions: 2, statements: 2, cache_writes: 0 });
    let opened = fixture.services.repositories.open(Some(&bob), "bob", "demo").await.unwrap();
    assert!(fixture.services.forks.parent(None, &opened).await.unwrap().is_none());
}
