//! Actual-store creation fast paths, SQL uniqueness and existing filesystem/grant failure gaps.
use super::Fixture;
use rbit_core::{Config, OrgRole, Visibility, membership::BasePermission};
use rbit_services::{CreateRepository, DomainEvent, ServiceError, orgs::CreateOrganization};
use rbit_storage::count_work;
use sqlx::Connection as _;
fn input(name: &str) -> CreateRepository {
    CreateRepository { name: name.to_owned(), ..CreateRepository::default() }
}
#[test]
fn original_creation_paths_preserve_exact_input_and_storage_type_identity() {
    use std::any::TypeId;
    assert_eq!(TypeId::of::<CreateRepository>(), TypeId::of::<rbit_repositories::creation::CreateRepository>());
    assert_eq!(TypeId::of::<rbit_storage::NewRepository>(), TypeId::of::<rbit_repositories::creation::NewRepository>());
}
#[tokio::test]
async fn self_owner_fast_path_is_one_statement_and_invalid_other_owner_precedes_bad_name() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let (result, work) = count_work(fixture.services.repositories.create_in(&alice, " ALIce ", input(" demo "))).await;
    assert_eq!(result.unwrap().repo.name.as_str(), "demo");
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(fixture.services.repositories.create_in(&alice, "ghost", input("bad name"))).await;
    assert!(matches!(result,Err(ServiceError::Invalid(message)) if message=="you cannot create repositories for ghost"));
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(fixture.services.repositories.create(&alice, input("bad name"))).await;
    assert!(matches!(result, Err(ServiceError::Invalid(_))));
    assert_eq!(work.statements, 0);
}
#[tokio::test]
async fn racing_same_name_creates_only_one_unique_sql_row_bare_repository_and_event() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let mut events = fixture.services.events.subscribe();
    let (first, second) = tokio::join!(
        fixture.services.repositories.create(&alice, input("demo")),
        fixture.services.repositories.create(&alice, input("DEMO"))
    );
    assert!(matches!(
        (&first, &second),
        (Ok(_), Err(ServiceError::Conflict("repository name"))) | (Err(ServiceError::Conflict("repository name")), Ok(_))
    ));
    let winner = first.ok().or_else(|| second.ok()).unwrap();
    let mut created = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let DomainEvent::RepositoryCreated { repo, .. } = event {
            created.push(repo);
        }
    }
    assert_eq!(created, [winner.repo.id]);
    let opened = fixture.open(Some(&alice), "alice", winner.repo.name.as_str()).await.unwrap();
    assert!(opened.disk_path.join("HEAD").exists());
}
#[tokio::test]
async fn actual_disk_failure_rolls_back_metadata_and_rollback_failure_preserves_original_gap() {
    for reject_rollback in [false, true] {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice", false).await;
        let mut config = Config::default();
        config.storage.data_dir = fixture.dir.path().to_path_buf();
        let root = config.storage.repositories_dir();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(alice.id().0.to_string()), b"fixture blocks owner directory").unwrap();
        let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.dir.path().join("rbit.db"));
        let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
        if reject_rollback {
            sqlx::query("CREATE TRIGGER fixture_reject_rollback BEFORE DELETE ON repositories BEGIN SELECT RAISE(ABORT,'fixture rollback failure'); END").execute(&mut connection).await.unwrap();
        }
        let mut events = fixture.services.events.subscribe();
        let (result, work) = count_work(fixture.services.repositories.create(&alice, input("demo"))).await;
        assert!(matches!(result, Err(ServiceError::Internal(_))));
        assert_eq!((work.acquisitions, work.statements), (2, 2));
        assert_eq!(fixture.database.repositories().find_by_path("alice", "demo").await.unwrap().is_some(), reject_rollback);
        assert!(events.try_recv().is_err());
        connection.close().await.unwrap();
    }
}
#[tokio::test]
async fn failed_creator_grant_keeps_original_row_and_disk_without_repository_created_event() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    let org = fixture
        .services
        .orgs
        .create(&alice, CreateOrganization { name: "acme".to_owned(), ..CreateOrganization::default() })
        .await
        .unwrap();
    fixture.services.orgs.add_member(&alice, &org, "bob", OrgRole::Member).await.unwrap();
    fixture.database.orgs().set_base_permission(org.account.id, BasePermission::None).await.unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.dir.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("CREATE TRIGGER fixture_reject_creator_grant BEFORE INSERT ON collaborators BEGIN SELECT RAISE(ABORT,'fixture creator grant failure'); END").execute(&mut connection).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.repositories.create_in(
        &bob,
        "acme",
        CreateRepository { visibility: Visibility::Private, ..input("demo") },
    ))
    .await;
    assert!(matches!(result, Err(ServiceError::Internal(_))));
    assert_eq!((work.acquisitions, work.statements), (5, 5));
    let kept = fixture.database.repositories().find_by_path("acme", "demo").await.unwrap().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = fixture.dir.path().to_path_buf();
    assert!(
        config.storage.repositories_dir().join(org.account.id.0.to_string()).join(format!("{}.git", kept.repo.id.0)).join("HEAD").exists()
    );
    assert!(events.try_recv().is_err());
    connection.close().await.unwrap();
}
