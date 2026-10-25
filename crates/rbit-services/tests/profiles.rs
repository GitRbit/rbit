//! Profile and account-settings use-cases against real, temporary `SQLite` and redb stores.

use std::sync::Arc;

use rbit_auth::Passwords;
use rbit_core::{Config, User};
use rbit_services::{
    ProfileService, ServiceError, Services,
    profiles::{Profile, PublicProfile, UpdateProfile},
};
use rbit_storage::{Database, KvStore, NewUser};

const PASSWORD: &str = "correct horse battery";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let services = Services::new(Arc::new(Config::default()), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    fn profiles(&self) -> &ProfileService {
        &self.services.profiles
    }

    async fn user(&self, name: &str) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: Passwords::hash(PASSWORD.to_owned()).await.unwrap(),
                is_admin: false,
            })
            .await
            .unwrap()
    }

    /// The user as currently stored (sessions re-read it on every request).
    async fn reload(&self, user: &User) -> User {
        self.database.accounts().find_user(user.id()).await.unwrap().unwrap()
    }
}

fn update(display_name: &str, bio: &str, website: &str, location: &str) -> UpdateProfile {
    UpdateProfile { display_name: display_name.to_owned(), bio: bio.to_owned(), website: website.to_owned(), location: location.to_owned() }
}

#[tokio::test]
async fn username_changes_require_current_password_and_keep_identity() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    fixture.user("bob").await;
    let service = fixture.profiles();
    assert!(matches!(service.change_username(&alice, "wrong".into(), "newname").await, Err(ServiceError::InvalidCredentials)));
    assert!(service.change_username(&alice, PASSWORD.into(), "settings").await.is_err());
    assert!(matches!(service.change_username(&alice, PASSWORD.into(), "BOB").await, Err(ServiceError::Conflict(_))));
    service.change_username(&alice, PASSWORD.into(), " newname ").await.unwrap();
    let renamed = fixture.reload(&alice).await;
    assert_eq!(renamed.id(), alice.id());
    assert_eq!(renamed.email, alice.email);
    assert_eq!(renamed.name().as_str(), "newname");
    let audit = fixture.services.audit.recent_for(&renamed, 10).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, rbit_services::audit::AuditAction::UsernameChanged);
    assert_eq!(audit[0].actor_id, Some(alice.id()));
    assert_eq!(audit[0].target_name.as_deref(), Some("newname"));
    assert_eq!(fixture.services.accounts.moved_to("ALICE").await.unwrap().unwrap().id, alice.id());
    assert!(fixture.database.accounts().find_login("alice").await.unwrap().is_none());
    assert_eq!(fixture.database.accounts().find_login("newname").await.unwrap().unwrap().user.id(), alice.id());
    assert!(!fixture.database.accounts().rename_if_password(alice.id(), &"raced".parse().unwrap(), "stale-hash").await.unwrap());
    service.change_username(&renamed, PASSWORD.into(), "lastone").await.unwrap();
    assert!(matches!(service.change_username(&renamed, PASSWORD.into(), "limited").await, Err(ServiceError::RateLimited { .. })));
}

#[tokio::test]
async fn update_and_read_profile() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;

    let blank = fixture.profiles().profile(&alice).await.unwrap();
    assert_eq!(blank, Profile::default());
    assert_eq!(fixture.profiles().public_profile(alice.id()).await.unwrap(), PublicProfile::default());

    fixture
        .profiles()
        .update_profile(&alice, update(" Alice Liddell ", "Curious.\nVery.", "https://alice.example", " Oxford "))
        .await
        .unwrap();
    let profile = fixture.profiles().profile(&alice).await.unwrap();
    assert_eq!(profile.display_name.as_deref(), Some("Alice Liddell"));
    assert_eq!(profile.bio.as_deref(), Some("Curious.\nVery."));
    assert_eq!(profile.website.as_deref(), Some("https://alice.example"));
    assert_eq!(profile.location.as_deref(), Some("Oxford"));

    let reloaded = fixture.reload(&alice).await;
    assert_eq!(reloaded.account.label(), "Alice Liddell");
    let public = fixture.profiles().public_profile(alice.id()).await.unwrap();
    assert_eq!(public.website.as_deref(), Some("https://alice.example"));
    assert_eq!(public.location.as_deref(), Some("Oxford"));

    // Blank fields clear their value.
    fixture.profiles().update_profile(&alice, update("", "  ", "", "")).await.unwrap();
    assert_eq!(fixture.profiles().profile(&alice).await.unwrap(), Profile::default());
    assert_eq!(fixture.reload(&alice).await.account.label(), "alice");
}

#[tokio::test]
async fn profile_validation() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let invalid = |result: Result<_, ServiceError>| matches!(result, Err(ServiceError::Invalid(_)));

    assert!(invalid(fixture.profiles().update_profile(&alice, update(&"x".repeat(81), "", "", "")).await));
    assert!(invalid(fixture.profiles().update_profile(&alice, update("", &"x".repeat(301), "", "")).await));
    assert!(invalid(fixture.profiles().update_profile(&alice, update("", "", "alice.example", "")).await));
    assert!(invalid(fixture.profiles().update_profile(&alice, update("", "", "javascript:alert(1)", "")).await));
    assert!(invalid(fixture.profiles().update_profile(&alice, update("", "", "", &"x".repeat(81))).await));
    assert!(invalid(fixture.profiles().update_profile(&alice, update("Two\nlines", "", "", "")).await));
    assert_eq!(fixture.profiles().profile(&alice).await.unwrap(), Profile::default(), "rejected input changes nothing");

    assert!(fixture.profiles().update_profile(&alice, update(&"é".repeat(80), &"é".repeat(300), "", &"é".repeat(80))).await.is_ok());
}

#[tokio::test]
async fn change_email() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    fixture.user("bob").await;

    let wrong_password = fixture.profiles().change_email(&alice, "nope".to_owned(), "new@example.com").await;
    assert!(matches!(wrong_password, Err(ServiceError::InvalidCredentials)));
    let malformed = fixture.profiles().change_email(&alice, PASSWORD.to_owned(), "not an email").await;
    assert!(matches!(malformed, Err(ServiceError::Invalid(_))));
    let taken = fixture.profiles().change_email(&alice, PASSWORD.to_owned(), "BOB@example.com").await;
    assert!(matches!(taken, Err(ServiceError::Conflict("email"))));
    assert_eq!(fixture.reload(&alice).await.email.as_str(), "alice@example.com");

    fixture.profiles().change_email(&alice, PASSWORD.to_owned(), " alice@wonderland.example ").await.unwrap();
    assert_eq!(fixture.reload(&alice).await.email.as_str(), "alice@wonderland.example");
}

#[tokio::test]
async fn change_password() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let new_password = "a much better passphrase";
    let session = fixture.services.accounts.start_session(&alice).await.unwrap();

    let wrong = fixture.profiles().change_password(&alice, "wrong".to_owned(), new_password.to_owned()).await;
    assert!(matches!(wrong, Err(ServiceError::InvalidCredentials)));
    let short = fixture.profiles().change_password(&alice, PASSWORD.to_owned(), "short".to_owned()).await;
    assert!(matches!(short, Err(ServiceError::Invalid(_))));
    let long = fixture.profiles().change_password(&alice, PASSWORD.to_owned(), "x".repeat(257)).await;
    assert!(matches!(long, Err(ServiceError::Invalid(_))));

    fixture.profiles().change_password(&alice, PASSWORD.to_owned(), new_password.to_owned()).await.unwrap();
    assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_none(), "revoked before the change returns");
    let hash = fixture.database.profiles().password_hash(alice.id()).await.unwrap().unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(Passwords::verify(new_password.to_owned(), hash.clone()).await.unwrap());
    assert!(!Passwords::verify(PASSWORD.to_owned(), hash).await.unwrap());

    // The old password no longer re-authenticates sensitive changes.
    let stale = fixture.profiles().change_email(&alice, PASSWORD.to_owned(), "a@example.com").await;
    assert!(matches!(stale, Err(ServiceError::InvalidCredentials)));
}
