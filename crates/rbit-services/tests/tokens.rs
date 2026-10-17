//! Personal access token use-cases against real, temporary SQLite and redb stores.

use std::sync::Arc;

use rbit_auth::AccessTokenSecret;
use rbit_core::{AccessTokenId, Config, RepoAccess, Timestamp, TokenScope, User};
use rbit_services::{AccessTokenService, CreateAccessToken, ServiceError, Services};
use rbit_storage::{Database, KvStore, NewAccessToken, NewUser};

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

    fn tokens(&self) -> &AccessTokenService {
        &self.services.tokens
    }

    /// Inserts a user directly; password hashing is irrelevant to token behaviour.
    async fn user(&self, name: &str) -> User {
        self.database
            .accounts()
            .create_user(NewUser {
                name: name.parse().unwrap(),
                email: format!("{name}@example.com").parse().unwrap(),
                display_name: None,
                password_hash: "not-a-real-hash".to_owned(),
                is_admin: false,
            })
            .await
            .unwrap()
    }
}

fn input(name: &str, scope: TokenScope) -> CreateAccessToken {
    CreateAccessToken { name: name.to_owned(), scope, expires_in_days: Some(30) }
}

#[tokio::test]
async fn create_authenticate_revoke() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;

    let (token, secret) = fixture.tokens().create(&alice, input("  laptop  ", TokenScope::Write)).await.unwrap();
    assert!(secret.starts_with(AccessTokenSecret::PREFIX));
    assert!(secret.ends_with(&token.hint));
    assert_eq!(token.name, "laptop", "name is trimmed");
    let expires_at = token.expires_at.unwrap().seconds();
    let expected = Timestamp::now().seconds() + 30 * 24 * 60 * 60;
    assert!((expected - 5..=expected).contains(&expires_at));

    let (user, scope) = fixture.tokens().authenticate(&secret).await.unwrap().unwrap();
    assert_eq!(user.id(), alice.id());
    assert_eq!(scope, TokenScope::Write);

    let listed = fixture.tokens().list(&alice).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].last_used_at.is_some(), "authentication records last use");

    fixture.tokens().revoke(&alice, token.id).await.unwrap();
    assert!(fixture.tokens().authenticate(&secret).await.unwrap().is_none());
    assert!(fixture.tokens().list(&alice).await.unwrap().is_empty());
    assert!(matches!(fixture.tokens().revoke(&alice, token.id).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn unknown_and_malformed_secrets_are_rejected() {
    let fixture = Fixture::new().await;
    for secret in ["", "hunter2", "rbit_", "rbit_tooshort", "rit_", "rit_tooshort", &AccessTokenSecret::generate().encode()] {
        assert!(fixture.tokens().authenticate(secret).await.unwrap().is_none(), "{secret:?}");
    }
}

/// Tokens issued before the `rbit_` prefix were `rit_` + the same 43 characters, and the
/// stored digest covers only those characters: such a token keeps authenticating.
#[tokio::test]
async fn tokens_with_the_legacy_prefix_still_authenticate() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (token, secret) = fixture.tokens().create(&alice, input("old laptop", TokenScope::Write)).await.unwrap();
    let legacy = secret.replacen(AccessTokenSecret::PREFIX, AccessTokenSecret::LEGACY_PREFIX, 1);
    assert!(legacy.starts_with("rit_") && legacy.ends_with(&token.hint));

    let (user, scope) = fixture.tokens().authenticate(&legacy).await.unwrap().unwrap();
    assert_eq!(user.id(), alice.id());
    assert_eq!(scope, TokenScope::Write);

    fixture.tokens().revoke(&alice, token.id).await.unwrap();
    assert!(fixture.tokens().authenticate(&legacy).await.unwrap().is_none(), "revoking covers both spellings");
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let secret = AccessTokenSecret::generate();
    fixture
        .database
        .access_tokens()
        .create(NewAccessToken {
            account_id: alice.id(),
            name: "old".to_owned(),
            digest: secret.digest(),
            hint: secret.hint(),
            scope: TokenScope::Write,
            expires_at: Some(Timestamp(Timestamp::now().seconds() - 1)),
        })
        .await
        .unwrap();

    assert!(fixture.tokens().authenticate(&secret.encode()).await.unwrap().is_none());
}

#[tokio::test]
async fn another_user_cannot_revoke_your_token() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let mallory = fixture.user("mallory").await;
    let (token, secret) = fixture.tokens().create(&alice, input("laptop", TokenScope::Read)).await.unwrap();

    assert!(matches!(fixture.tokens().revoke(&mallory, token.id).await, Err(ServiceError::NotFound)));
    assert!(fixture.tokens().list(&mallory).await.unwrap().is_empty());
    assert!(fixture.tokens().authenticate(&secret).await.unwrap().is_some());
    assert!(matches!(fixture.tokens().revoke(&alice, AccessTokenId(9999)).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn read_scope_limits_write_access_to_read() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, secret) = fixture.tokens().create(&alice, input("ci", TokenScope::Read)).await.unwrap();

    let (_, scope) = fixture.tokens().authenticate(&secret).await.unwrap().unwrap();
    assert_eq!(scope, TokenScope::Read);
    assert_eq!(scope.limit(RepoAccess::Write), RepoAccess::Read);
    assert_eq!(scope.limit(RepoAccess::Admin), RepoAccess::Read);
    assert!(!scope.limit(RepoAccess::Admin).can_write());
}

#[tokio::test]
async fn admin_scope_reaches_administration_but_never_beyond_the_owner() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (token, secret) = fixture.tokens().create(&alice, input("ops", TokenScope::Admin)).await.unwrap();
    assert_eq!(token.scope, TokenScope::Admin);

    let (_, scope) = fixture.tokens().authenticate(&secret).await.unwrap().unwrap();
    assert_eq!(scope, TokenScope::Admin);
    assert!(scope.limit(RepoAccess::Admin).can_admin());
    assert_eq!(scope.limit(RepoAccess::Write), RepoAccess::Write);
    assert_eq!(scope.limit(RepoAccess::None), RepoAccess::None);
}

#[tokio::test]
async fn validates_input() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let invalid = |result: Result<_, ServiceError>| matches!(result, Err(ServiceError::Invalid(_)));

    assert!(invalid(fixture.tokens().create(&alice, input("   ", TokenScope::Read)).await));
    assert!(invalid(fixture.tokens().create(&alice, input(&"x".repeat(65), TokenScope::Read)).await));
    assert!(fixture.tokens().create(&alice, input(&"é".repeat(64), TokenScope::Read)).await.is_ok());

    let odd_expiry = CreateAccessToken { name: "odd".to_owned(), scope: TokenScope::Read, expires_in_days: Some(8) };
    assert!(invalid(fixture.tokens().create(&alice, odd_expiry).await));
    for &expires_in_days in AccessTokenService::EXPIRY_CHOICES {
        let choice = CreateAccessToken { name: "choice".to_owned(), scope: TokenScope::Read, expires_in_days };
        let (token, _) = fixture.tokens().create(&alice, choice).await.unwrap();
        assert_eq!(token.expires_at.is_none(), expires_in_days.is_none());
    }
}

#[tokio::test]
async fn caps_tokens_per_user() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    for n in 0..50 {
        fixture.tokens().create(&alice, input(&format!("token {n}"), TokenScope::Read)).await.unwrap();
    }

    let over_limit = fixture.tokens().create(&alice, input("one too many", TokenScope::Read)).await;
    assert!(matches!(over_limit, Err(ServiceError::Invalid(_))));
    assert!(fixture.tokens().create(&bob, input("unaffected", TokenScope::Read)).await.is_ok());
}
