//! Linked external identities (workstream O) against a real, migrated SQLite database:
//! uniqueness, registration with an identity, the last-method guard on unlinking and the
//! verified-email lookup.

use rbit_core::{Timestamp, User};
use rbit_storage::{
    Database, NewUser, StorageError,
    sqlite::external_identities::{LinkConflict, NewExternalIdentity, Unlinked},
};

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
}

fn new_user(name: &str, password_hash: &str) -> NewUser {
    NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: password_hash.to_owned(),
        is_admin: false,
    }
}

fn identity<'a>(provider: &'a str, subject: &'a str) -> NewExternalIdentity<'a> {
    NewExternalIdentity { provider_id: provider, subject, email: Some("x@example.com"), username: Some("x") }
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        Self { _dir: dir, database }
    }

    async fn user(&self, name: &str, password_hash: &str) -> User {
        self.database.accounts().create_user(new_user(name, password_hash)).await.unwrap()
    }
}

#[tokio::test]
async fn identities_link_once_per_provider_and_account() {
    let fx = Fixture::new().await;
    let store = fx.database.external_identities();
    let alice = fx.user("alice", "hash").await;
    let bob = fx.user("bob", "hash").await;

    let linked = store.link(alice.id(), identity("corp", "s-1"), Timestamp(10)).await.unwrap().unwrap();
    assert_eq!((linked.account_id, linked.provider_id.as_str(), linked.subject.as_str()), (alice.id(), "corp", "s-1"));
    assert_eq!(linked.last_used_at, Some(Timestamp(10)));
    assert_eq!(store.find("corp", "s-1").await.unwrap(), Some(linked.clone()));
    assert_eq!(store.find("corp", "s-2").await.unwrap(), None);
    assert_eq!(store.find("github", "s-1").await.unwrap(), None, "subjects are per provider");

    assert_eq!(store.link(bob.id(), identity("corp", "s-1"), Timestamp(11)).await.unwrap(), Err(LinkConflict::IdentityTaken));
    assert_eq!(store.link(alice.id(), identity("corp", "s-9"), Timestamp(11)).await.unwrap(), Err(LinkConflict::ProviderTaken));
    store.link(alice.id(), identity("github", "42"), Timestamp(12)).await.unwrap().unwrap();
    let listed = store.list_for_account(alice.id()).await.unwrap();
    assert_eq!(listed.iter().map(|identity| identity.provider_id.as_str()).collect::<Vec<_>>(), ["corp", "github"]);

    store.touch(linked.id, Some("new@example.com"), None, Timestamp(20)).await.unwrap();
    let touched = store.find("corp", "s-1").await.unwrap().unwrap();
    assert_eq!((touched.email.as_deref(), touched.username, touched.last_used_at), (Some("new@example.com"), None, Some(Timestamp(20))));
}

#[tokio::test]
async fn unlinking_keeps_a_way_to_sign_in() {
    let fx = Fixture::new().await;
    let store = fx.database.external_identities();
    let with_password = fx.user("alice", "hash").await;
    let without = fx.user("bob", "").await;
    assert!(store.has_password(with_password.id()).await.unwrap());
    assert!(!store.has_password(without.id()).await.unwrap());
    let usable = ["corp", "github"];

    store.link(with_password.id(), identity("corp", "a"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(with_password.id(), "corp", &usable).await.unwrap(), Unlinked::Removed, "the password remains");
    assert_eq!(store.unlink(with_password.id(), "corp", &usable).await.unwrap(), Unlinked::NotFound);

    store.link(without.id(), identity("corp", "b"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(without.id(), "corp", &usable).await.unwrap(), Unlinked::LastMethod);
    store.link(without.id(), identity("retired", "b"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(
        store.unlink(without.id(), "corp", &usable).await.unwrap(),
        Unlinked::LastMethod,
        "a provider that is no longer configured doesn't count"
    );
    store.link(without.id(), identity("github", "b"), Timestamp(1)).await.unwrap().unwrap();
    assert_eq!(store.unlink(without.id(), "corp", &usable).await.unwrap(), Unlinked::Removed);
    assert_eq!(store.unlink(without.id(), "github", &usable).await.unwrap(), Unlinked::LastMethod);
    assert_eq!(store.unlink(without.id(), "retired", &usable).await.unwrap(), Unlinked::Removed, "GitHub still works");
}

#[tokio::test]
async fn registering_creates_the_account_and_its_identity_together() {
    let fx = Fixture::new().await;
    let store = fx.database.external_identities();
    let user = store.register(new_user("carol", ""), true, identity("corp", "c")).await.unwrap().unwrap();
    assert_eq!(store.find("corp", "c").await.unwrap().unwrap().account_id, user.id());
    assert!(!store.has_password(user.id()).await.unwrap());
    assert_eq!(
        store.account_with_verified_email("CAROL@example.com").await.unwrap(),
        Some(user.id()),
        "addresses compare case-insensitively"
    );

    let unverified = store.register(new_user("dave", ""), false, identity("corp", "d")).await.unwrap().unwrap();
    assert_eq!(store.account_with_verified_email("dave@example.com").await.unwrap(), None, "never by an unverified address");
    assert_eq!(fx.database.accounts().find_user(unverified.id()).await.unwrap().unwrap().name().as_str(), "dave");

    // The identity is taken: nothing is created.
    assert!(matches!(store.register(new_user("erin", ""), true, identity("corp", "c")).await.unwrap(), Err(LinkConflict::IdentityTaken)));
    assert!(fx.database.accounts().find_login("erin").await.unwrap().is_none());
    // The username is taken: a conflict, as with password sign-up, and no identity.
    assert!(matches!(store.register(new_user("carol", ""), true, identity("corp", "z")).await, Err(StorageError::Conflict("username"))));
    assert!(store.find("corp", "z").await.unwrap().is_none());
}
