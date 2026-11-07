//! Real SQL/Git work counts and failure boundaries of the feature-owned settings application.
use super::{Fixture, general, head_of};
use rbit_core::{RepoAccess, Timestamp, Visibility};
use rbit_services::{DomainEvent, ServiceError};
use rbit_storage::count_work;
use sqlx::Connection as _;
#[test]
fn original_settings_paths_preserve_concrete_type_identity() {
    use std::any::TypeId;
    assert_eq!(
        TypeId::of::<rbit_services::repo_settings::UpdateRepository>(),
        TypeId::of::<rbit_repositories::settings::UpdateRepository>()
    );
    assert_eq!(
        TypeId::of::<rbit_storage::sqlite::repo_settings::RepoGeneralSettings>(),
        TypeId::of::<rbit_repositories::settings::RepoGeneralSettings>()
    );
}
#[tokio::test]
async fn rejected_same_name_and_unchanged_branch_paths_keep_original_work() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let mut repo = fixture.repo(&alice, "demo", Visibility::Public).await;
    let settings = &fixture.services.repo_settings;
    repo.access = RepoAccess::Read;
    let (result, work) = count_work(settings.update(&repo, general(&"x".repeat(351), Visibility::Public, Some("missing")))).await;
    assert!(matches!(result, Err(ServiceError::Forbidden)));
    assert_eq!(work.statements, 0);
    repo = fixture.open(Some(&alice), "alice", "demo").await.unwrap();
    let (result, work) = count_work(settings.rename(&repo, " demo ")).await;
    result.unwrap();
    assert_eq!(work.statements, 0);
    let (result, work) =
        count_work(settings.update(&repo, general(" tidy ", Visibility::Private, Some(&repo.repo.repo.default_branch)))).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(head_of(&repo), format!("ref: refs/heads/{}\n", repo.repo.repo.default_branch));
}
#[tokio::test]
async fn changed_head_is_restored_when_actual_sql_update_fails() {
    let fixture = Fixture::with_fault_schema("CREATE TRIGGER fixture_reject_head BEFORE UPDATE OF default_branch ON repositories WHEN NEW.default_branch <> OLD.default_branch BEGIN SELECT RAISE(ABORT,'fixture HEAD failure'); END").await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "demo", Visibility::Public).await;
    fixture.push_branches(&repo, &["main", "other"]);
    let before = head_of(&repo);
    let (result, work) = count_work(fixture.services.repo_settings.update(&repo, general("new", Visibility::Public, Some("other")))).await;
    assert!(matches!(result, Err(ServiceError::Internal(ref error)) if error.to_string().contains("fixture HEAD failure")), "{result:?}");
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(head_of(&repo), before);
    let current = fixture.open(Some(&alice), "alice", "demo").await.unwrap();
    assert_eq!(current.repo.repo.description, None);
}
#[tokio::test]
async fn archive_race_publishes_one_sql_winner_and_stale_repeat_still_runs_one_statement() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let repo = fixture.repo(&alice, "demo", Visibility::Public).await;
    let settings = &fixture.services.repo_settings;
    let mut events = fixture.services.events.subscribe();
    let (first, second) = tokio::join!(settings.set_archived(&alice, &repo, true), settings.set_archived(&alice, &repo, true));
    assert_ne!(first.unwrap(), second.unwrap());
    let mut archived = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let DomainEvent::RepositoryArchived { archived: value, .. } = event {
            archived.push(value);
        }
    }
    assert_eq!(archived, [true]);
    let (result, work) = count_work(settings.set_archived(&alice, &repo, true)).await;
    assert!(!result.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (result, work) = count_work(settings.set_archived(&alice, &repo, false)).await;
    assert!(result.unwrap());
    assert_eq!((work.acquisitions, work.statements), (2, 2));
}
#[tokio::test]
async fn failed_metadata_delete_retains_original_disk_first_gap_and_retry_semantics() {
    let fixture = Fixture::with_fault_schema(
        "CREATE TABLE fixture_delete_fault (enabled INTEGER NOT NULL CHECK(enabled IN (0,1))) STRICT; \
         INSERT INTO fixture_delete_fault VALUES(1); \
         CREATE TRIGGER fixture_reject_repo_delete BEFORE DELETE ON repositories \
         WHEN (SELECT enabled FROM fixture_delete_fault)=1 \
         BEGIN SELECT RAISE(ABORT,'fixture deletion failure'); END;",
    )
    .await;
    let alice = fixture.user("alice", false).await;
    let mut repo = fixture.repo(&alice, "demo", Visibility::Public).await;
    repo.repo.repo.archived_at = Some(Timestamp(1));
    let options = rbit_storage::sqlite::writer_compatibility::register(
        sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.dir.path().join("rbit.db")),
    );
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    let mut events = fixture.services.events.subscribe();
    let (result, work) = count_work(fixture.services.repo_settings.delete(&alice, &repo, "alice/demo")).await;
    assert!(
        matches!(result, Err(ServiceError::Internal(ref error)) if error.to_string().contains("fixture deletion failure")),
        "{result:?}"
    );
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert!(!repo.disk_path.exists());
    assert!(fixture.database.repositories().find_by_path("alice", "demo").await.unwrap().is_some());
    assert!(events.try_recv().is_err());
    sqlx::query("UPDATE fixture_delete_fault SET enabled=0").execute(&mut connection).await.unwrap();
    let (result, work) = count_work(fixture.services.repo_settings.delete(&alice, &repo, "alice/demo")).await;
    result.unwrap();
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert!(matches!(events.try_recv().unwrap(), DomainEvent::RepositoryDeleted { .. }));
    connection.close().await.unwrap();
}
