//! Same-insert epoch capture, unchanged legacy work, SQL rollback and bootstrap authority.
use rbit_core::Timestamp;
use rbit_storage::{
    Database, NewUser, StorageError, Work, count_work,
    sqlite::{
        external_identities::{LinkConflict, NewExternalIdentity},
        writer_compatibility::register,
    },
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

struct Fixture {
    dir: tempfile::TempDir,
    database: Database,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        Self { dir, database }
    }
    async fn raw(&self) -> sqlx::SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(register(SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db")).foreign_keys(true)))
            .await
            .unwrap()
    }
}
fn user(name: &str) -> NewUser {
    NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "fixture PHC".into(),
        is_admin: false,
    }
}
fn identity(subject: &str) -> NewExternalIdentity<'_> {
    NewExternalIdentity { provider_id: "corp", subject, email: None, username: None }
}

#[tokio::test]
async fn original_insert_allocates_in_same_transaction_with_explicit_one_statement_security_cost() {
    let legacy = Fixture::new().await;
    let original = Fixture::new().await;
    let counter = crate::query_count::QueryCounter::new();
    let (old, baseline) = count_work(counter.measure(legacy.database.accounts().register_user(user("alice"), false))).await;
    assert!(old.unwrap().is_admin);
    let old_queries = counter.queries();
    let counter = crate::query_count::QueryCounter::new();
    let (created, work) = count_work(counter.measure(original.database.accounts().register_user_original(user("alice"), false))).await;
    assert_eq!(work.acquisitions, baseline.acquisitions);
    assert_eq!(work.statements, baseline.statements + 1);
    assert_eq!(work, Work { acquisitions: 1, statements: 4, cache_writes: 0 });
    assert_eq!(counter.queries().len(), old_queries.len() + 1);
    let created = created.unwrap();
    assert!(created.user.is_admin);
    assert_eq!(created.epoch.get(), 1);
    original
        .database
        .profiles()
        .set_password_hash_with_intent(created.user.id(), "new PHC", created.epoch, Timestamp(20))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.epoch.get(), 1, "creation facts cannot be silently restamped after reset");
    assert!(original.database.accounts().find_session_user_at_epoch(created.user.id(), false, created.epoch).await.unwrap().is_none());
}

#[tokio::test]
async fn login_insert_and_commit_failures_publish_no_snapshot_or_orphan_account() {
    let fixture = Fixture::new().await;
    fixture.database.accounts().register_user_original(user("alice"), true).await.unwrap();
    let mut duplicate = user("bob");
    duplicate.email = "alice@example.com".parse().unwrap();
    assert!(matches!(fixture.database.accounts().register_user_original(duplicate, true).await, Err(StorageError::Conflict("email"))));
    assert!(fixture.database.accounts().find_account_by_name("bob").await.unwrap().is_none());
    let raw = fixture.raw().await;
    sqlx::raw_sql(
        "CREATE TABLE failing_commit(account_id INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED) STRICT; \
        CREATE TRIGGER fail_new_commit AFTER INSERT ON user_logins BEGIN INSERT INTO failing_commit VALUES(999999); END;",
    )
    .execute(&raw)
    .await
    .unwrap();
    assert!(fixture.database.accounts().register_user_original(user("carol"), true).await.is_err());
    assert!(fixture.database.accounts().find_account_by_name("carol").await.unwrap().is_none());
    assert_eq!(fixture.database.accounts().count_users().await.unwrap(), 1);
    raw.close().await;
}

#[tokio::test]
async fn external_creation_link_and_epoch_share_transaction_with_one_global_allocation() {
    let legacy = Fixture::new().await;
    let original = Fixture::new().await;
    let (_, baseline) = count_work(legacy.database.external_identities().register(user("alice"), true, identity("first"), false)).await;
    let (created, work) =
        count_work(original.database.external_identities().register_original(user("alice"), true, identity("first"), false)).await;
    assert_eq!(work.acquisitions, baseline.acquisitions);
    assert_eq!(work.statements, baseline.statements + 1);
    let created = created.unwrap().unwrap();
    assert_eq!(created.epoch.get(), 1);
    assert!(created.user.is_admin);
    assert_eq!(
        original.database.external_identities().account_with_verified_email("alice@example.com").await.unwrap(),
        Some(created.user.id())
    );
    let conflict = original.database.external_identities().register_original(user("bob"), true, identity("first"), true).await.unwrap();
    assert!(matches!(conflict, Err(LinkConflict::IdentityTaken)));
    assert!(original.database.accounts().find_account_by_name("bob").await.unwrap().is_none());
    assert_eq!(original.database.accounts().count_users().await.unwrap(), 1);
}

#[tokio::test]
async fn password_and_external_original_registration_race_still_has_one_bootstrap_administrator() {
    let fixture = Fixture::new().await;
    let accounts = fixture.database.accounts();
    let external = fixture.database.external_identities();
    let (password, provider) = tokio::join!(
        accounts.register_user_original(user("alice"), false),
        external.register_original(user("bob"), true, identity("first"), false)
    );
    let provider = provider.map(Result::unwrap);
    let accepted: Vec<_> = [password.as_ref(), provider.as_ref()].into_iter().filter_map(Result::ok).collect();
    assert_eq!(accepted.len(), 1);
    assert!(accepted[0].user.is_admin);
    assert_eq!(accepted[0].epoch.get(), 1);
    assert_eq!(fixture.database.accounts().count_users().await.unwrap(), 1);
    for result in [password, provider] {
        if let Err(error) = result {
            assert!(matches!(error, StorageError::RegistrationClosed));
        }
    }
}

#[tokio::test]
async fn all_provisioning_variants_allocate_once_and_recreated_account_ids_cannot_reuse_epochs() {
    let f = Fixture::new().await;
    let (alice, ordinary) = count_work(f.database.accounts().create_user_original(user("alice"))).await;
    let alice = alice.unwrap();
    assert_eq!(ordinary, Work { acquisitions: 1, statements: 3, cache_writes: 0 });
    let (bob, temporary) = count_work(f.database.accounts().create_user_with_initial_password_original(user("bob"))).await;
    let bob = bob.unwrap();
    assert_eq!(temporary, Work { acquisitions: 1, statements: 4, cache_writes: 0 });
    assert!(f.database.accounts().password_change_required(bob.user.id()).await.unwrap());
    assert!(bob.epoch > alice.epoch);
    f.database.admin().delete_user(bob.user.id()).await.unwrap();
    let carol = f.database.accounts().create_user_original(user("carol")).await.unwrap();
    assert_eq!(carol.user.id(), bob.user.id(), "exercise SQLite highest-ID reuse");
    assert!(carol.epoch > bob.epoch);
    assert!(f.database.accounts().find_session_user_at_epoch(carol.user.id(), false, bob.epoch).await.unwrap().is_none());
    let id = carol.user.id();
    let epoch = carol.epoch;
    f.database.close().await;
    let reopened = Database::open(&f.dir.path().join("rbit.db")).await.unwrap();
    assert!(reopened.accounts().find_session_user_at_epoch(id, false, epoch).await.unwrap().is_some());
    let dave = reopened.accounts().create_user_original(user("dave")).await.unwrap();
    assert!(dave.epoch > epoch);
}

#[tokio::test]
async fn failed_creation_rolls_back_provisional_global_allocation_and_exhaustion_refuses_without_orphan() {
    let f = Fixture::new().await;
    let first = f.database.accounts().create_user_original(user("alice")).await.unwrap();
    let mut duplicate = user("bob");
    duplicate.email = first.user.email.clone();
    assert!(f.database.accounts().create_user_original(duplicate).await.is_err());
    let next = f.database.accounts().create_user_original(user("carol")).await.unwrap();
    assert_eq!(next.epoch.get(), first.epoch.get() + 1, "rolled-back allocations are not committed authority");
    let raw = f.raw().await;
    sqlx::query("UPDATE credential_epoch_allocator SET high_water=9223372036854775807 WHERE singleton=1").execute(&raw).await.unwrap();
    assert!(matches!(f.database.accounts().create_user_original(user("dave")).await, Err(StorageError::Corrupt(_))));
    assert!(f.database.accounts().find_account_by_name("dave").await.unwrap().is_none());
    assert_eq!(f.database.accounts().count_users().await.unwrap(), 2);
    raw.close().await;
}
