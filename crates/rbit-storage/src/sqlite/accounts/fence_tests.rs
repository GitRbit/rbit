//! Exact creation category only; other credential writers remain unenrolled.
use super::*;
use crate::{
    Database,
    sqlite::external_identities::{LinkConflict, NewExternalIdentity},
};
use rbit_authority_fence::{DrainError, Snapshot};
use sqlx::{
    Connection as _,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::Instant;

struct Fixture {
    dir: tempfile::TempDir,
    db: Database,
}
impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        Self { dir, db }
    }
    async fn connection(&self) -> SqliteConnection {
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db"))).await.unwrap()
    }
}
fn user(name: &str) -> NewUser {
    NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "unused".into(),
        is_admin: false,
    }
}
fn identity(subject: &str) -> NewExternalIdentity<'_> {
    NewExternalIdentity { provider_id: "corp", subject, email: None, username: None }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
async fn create(db: &Database, kind: usize, name: &str) -> Result<User, StorageError> {
    match kind {
        0 => db.accounts().create_user(user(name)).await,
        1 => db.accounts().create_user_with_initial_password(user(name)).await,
        2 => db.accounts().register_user(user(name), true).await,
        _ => db.external_identities().register(user(name), true, identity(name), true).await.map(Result::unwrap),
    }
}
fn stopped(result: &Result<User, StorageError>) {
    assert!(matches!(result, Err(StorageError::Conflict("principal creation writer stopped"))), "{result:?}");
}

#[tokio::test]
async fn all_creation_successes_preserve_roles_password_flags_and_verified_identity() {
    let fixture = Fixture::new().await;
    let accounts = fixture.db.accounts();
    let mut administrator = user("alice");
    administrator.is_admin = true;
    assert!(accounts.create_user(administrator).await.unwrap().is_admin);
    let temporary = accounts.create_user_with_initial_password(user("bob")).await.unwrap();
    assert!(accounts.password_change_required(temporary.id()).await.unwrap());
    let mut signup = user("carol");
    signup.is_admin = true; // A registration caller cannot choose the role.
    assert!(!accounts.register_user(signup, true).await.unwrap().is_admin);
    let external = fixture.db.external_identities();
    let linked = external.register(user("dave"), true, identity("dave"), true).await.unwrap().unwrap();
    assert!(!linked.is_admin);
    assert_eq!(external.find("corp", "dave").await.unwrap().unwrap().account_id, linked.id());
    assert_eq!(external.account_with_verified_email("dave@example.com").await.unwrap(), Some(linked.id()));
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn identity_business_conflict_awaits_rollback_and_settles_cleanly_without_partial_user() {
    let fixture = Fixture::new().await;
    let external = fixture.db.external_identities();
    let original = external.register(user("alice"), true, identity("same"), true).await.unwrap().unwrap();
    assert!(matches!(external.register(user("bob"), true, identity("same"), true).await.unwrap(), Err(LinkConflict::IdentityTaken)));
    assert!(fixture.db.accounts().find_account_by_name("bob").await.unwrap().is_none());
    assert_eq!(fixture.db.accounts().count_users().await.unwrap(), 1);
    assert_eq!(external.find("corp", "same").await.unwrap().unwrap().account_id, original.id());
    drop(fixture.db.authority_domain.drain(deadline()).await.unwrap());
    fixture.db.close().await;
}

#[tokio::test]
async fn four_creation_paths_share_policy_and_moderation_queue_and_terminal_stop() {
    let fixture = Fixture::new().await;
    let clone = fixture.db.clone();
    let alice = fixture
        .db
        .accounts()
        .create_user({
            let mut u = user("alice");
            u.is_admin = true;
            u
        })
        .await
        .unwrap();
    let policy = fixture.db.plugin_policy();
    let admin = clone.admin();
    let old_accounts = clone.accounts().clone();
    let old_external = clone.external_identities().clone();
    let quiet = fixture.db.authority_domain.drain(deadline()).await.unwrap();
    let mut creations: Vec<_> =
        ["bob", "carol", "dave", "erin"].into_iter().enumerate().map(|(kind, name)| Box::pin(create(&clone, kind, name))).collect();
    for creation in &mut creations {
        pending(creation.as_mut()).await;
    }
    let mut review = Box::pin(policy.prepare(alice.id(), Timestamp(1)));
    pending(review.as_mut()).await;
    let mut moderation = Box::pin(admin.set_suspended(alice.id(), false));
    pending(moderation.as_mut()).await;
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 6, uncertain: false, writers_stopped: false });
    assert!(clone.accounts().find_active_user(alice.id()).await.unwrap().is_some());
    drop(creations.pop().unwrap());
    assert_eq!(fixture.db.authority_domain.snapshot().queued, 5);
    drop(clone.close());
    drop(fixture.db.authority_domain.stop_writers().drain(deadline()).await.unwrap());
    drop(quiet);
    for creation in creations {
        stopped(&creation.await);
    }
    assert!(matches!(review.await, Err(StorageError::Conflict("package policy writer stopped"))));
    assert!(matches!(moderation.await, Err(StorageError::Conflict("account moderation writer stopped"))));
    stopped(&old_accounts.create_user(user("old")).await);
    stopped(&old_accounts.create_user_with_initial_password(user("old")).await);
    stopped(&old_accounts.register_user(user("old"), true).await);
    stopped(&old_external.register(user("old"), true, identity("old"), true).await.map(Result::unwrap));
    for kind in 0..4 {
        stopped(&create(&fixture.db, kind, "fresh").await);
    }
    assert_eq!(fixture.db.accounts().count_users().await.unwrap(), 1);
    assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: false, writers_stopped: true });
    fixture.db.close().await;
}

#[tokio::test]
async fn each_acquired_creation_cancel_is_uncertain_while_real_sql_writer_lock_is_held() {
    for kind in 0..4 {
        let fixture = Fixture::new().await;
        let mut connection = fixture.connection().await;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut connection).await.unwrap();
        let mut operation = Box::pin(create(&fixture.db, kind, "alice"));
        pending(operation.as_mut()).await;
        assert_eq!(fixture.db.authority_domain.snapshot().active, 1);
        drop(operation);
        assert_eq!(fixture.db.authority_domain.snapshot(), Snapshot { active: 0, queued: 0, uncertain: true, writers_stopped: false });
        sqlx::query("ROLLBACK").execute(&mut connection).await.unwrap();
        assert!(matches!(fixture.db.authority_domain.stop_writers().drain(deadline()).await, Err(DrainError::Uncertain)));
        // Enrollment before the first SQL await; not proof of worker entry or a
        // known settlement result for the canceled driver command.
        fixture.db.close().await;
    }
}

#[tokio::test]
async fn closed_registration_and_duplicate_failure_do_not_block_later_passive_creation() {
    let fixture = Fixture::new().await;
    let accounts = fixture.db.accounts();
    accounts.register_user(user("alice"), false).await.unwrap();
    assert!(matches!(accounts.register_user(user("bob"), false).await, Err(StorageError::RegistrationClosed)));
    assert!(accounts.find_account_by_name("bob").await.unwrap().is_none());
    assert!(accounts.create_user(user("alice")).await.is_err());
    assert!(fixture.db.authority_domain.snapshot().uncertain);
    assert_eq!(accounts.create_user(user("carol")).await.unwrap().name().as_str(), "carol");
    assert!(matches!(fixture.db.authority_domain.drain(deadline()).await, Err(DrainError::Uncertain)));
    fixture.db.close().await;
}
