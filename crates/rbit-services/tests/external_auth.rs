//! External sign-in (workstream O) against real, temporary `SQLite` and redb stores: the
//! linking rules, registration through a provider, the second factor, suspension, the
//! last-method guard and the checks a callback passes before any provider is contacted.
//! The HTTP side (discovery, keys, token exchange) is covered end to end against fake
//! providers in `rbit-e2e`.

use std::sync::Arc;

use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::{
    AccountId, Config, Timestamp, User,
    config::{ProviderConfig, ProviderKind, ProviderRegistration},
};
use rbit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
    external_auth::{Callback, CallbackParams, ExternalSignIn, VerifiedIdentity},
};
use rbit_storage::{Database, KvStore};

const PASSWORD: &str = "correct horse battery";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

fn provider(id: &str, kind: ProviderKind) -> ProviderConfig {
    ProviderConfig {
        kind,
        id: id.to_owned(),
        name: if kind == ProviderKind::Github { "GitHub".to_owned() } else { "Corp SSO".to_owned() },
        issuer: (kind == ProviderKind::Oidc).then(|| "https://sso.example.com".to_owned()),
        client_id: "rbit".to_owned(),
        client_secret: "s3cret".to_owned(),
        ..ProviderConfig::default()
    }
}

impl Fixture {
    async fn new() -> Self {
        Self::with(|_| {}).await
    }

    async fn with(configure: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        config.auth.providers = vec![provider("corp", ProviderKind::Oidc), provider("github", ProviderKind::Github)];
        configure(&mut config);
        let config = Arc::new(config);
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(config, &database, &kv).await.unwrap();
        Self { _dir: dir, database, services }
    }

    async fn user(&self, name: &str) -> User {
        let input = SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        self.services.accounts.create_user(input, None).await.unwrap()
    }

    async fn verify_email(&self, user: &User) {
        self.database.recovery().mark_verified(user.id(), &user.email, Timestamp::now()).await.unwrap();
    }

    async fn complete(&self, identity: VerifiedIdentity, link: Option<AccountId>) -> Result<ExternalSignIn, ServiceError> {
        let callback = Callback { identity, next: "/next".to_owned(), link_account: link };
        self.services.complete_external_sign_in(callback, &client()).await
    }

    /// Signs in with `identity`, expecting a session.
    async fn signed_in(&self, identity: VerifiedIdentity) -> User {
        match self.complete(identity, None).await.unwrap() {
            ExternalSignIn::SignedIn(user) => user,
            other => panic!("expected a session, got {other:?}"),
        }
    }

    /// Registers `username` through `identity`.
    async fn register(&self, identity: VerifiedIdentity, username: &str) -> User {
        let provider = identity.provider_id.clone();
        let email = identity.email.clone().unwrap_or_else(|| format!("{username}@example.org"));
        let ExternalSignIn::Register(token) = self.complete(identity, None).await.unwrap() else { panic!("expected registration") };
        self.services.register_external(&provider, &token, username, &email, &client()).await.unwrap().0
    }

    async fn audit(&self, user: &User) -> Vec<(AuditAction, String)> {
        self.services.audit.recent_for(user, 50).await.unwrap().into_iter().map(|event| (event.action, event.detail)).collect()
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("203.0.113.7".parse().unwrap()), user_agent: "test".to_owned() }
}

fn identity(provider: &str, subject: &str, email: Option<&str>, verified: bool) -> VerifiedIdentity {
    VerifiedIdentity {
        provider_id: provider.to_owned(),
        subject: subject.to_owned(),
        email: email.map(str::to_owned),
        email_verified: verified,
        username: Some("Ali Ce".to_owned()),
        display_name: Some("Alice".to_owned()),
        mfa: false,
    }
}

#[tokio::test]
async fn a_first_sign_in_registers_an_account_without_a_password() {
    let fx = Fixture::new().await;
    fx.user("root").await;
    let external = &fx.services.external_auth;
    let ExternalSignIn::Register(token) = fx.complete(identity("corp", "sub-1", Some("alice@corp.example"), true), None).await.unwrap()
    else {
        panic!("expected registration");
    };
    let draft = external.registration_draft("corp", &token).unwrap();
    assert_eq!((draft.username.as_str(), draft.email.as_str(), draft.email_verified), ("AliCe", "alice@corp.example", true));
    assert_eq!((draft.provider_name.as_str(), draft.next.as_str()), ("Corp SSO", "/next"));
    assert!(external.registration_draft("github", &token).is_none(), "the step belongs to its provider");

    let conflict = fx.services.register_external("corp", &token, "root", "alice@corp.example", &client()).await;
    assert!(matches!(conflict, Err(ServiceError::Conflict("username"))));
    let (alice, next) = fx.services.register_external("corp", &token, "alice", "alice@corp.example", &client()).await.unwrap();
    assert_eq!(next, "/next");
    assert!(!alice.is_admin);
    assert!(fx.database.recovery().verified_among(&[alice.id()]).await.unwrap().contains(&alice.id()), "the provider verified it");
    assert!(external.registration_draft("corp", &token).is_none(), "the step works once");
    assert!(matches!(fx.services.sign_in("alice", String::new(), &client()).await, Err(ServiceError::InvalidCredentials)), "no password");

    let methods = external.methods(&alice).await.unwrap();
    assert!(!methods.has_password);
    assert_eq!(methods.identities.len(), 1);
    assert_eq!(methods.identities[0].username.as_deref(), Some("Ali Ce"));
    assert_eq!(methods.linkable.iter().map(|provider| provider.id.as_str()).collect::<Vec<_>>(), ["github"]);

    // Signing in again finds the account by subject (the address may have changed).
    let again = fx.signed_in(identity("corp", "sub-1", Some("renamed@corp.example"), true)).await;
    assert_eq!(again.id(), alice.id());
    let audit = fx.audit(&alice).await;
    assert!(audit.contains(&(AuditAction::ExternalIdentityLinked, "Corp SSO (Ali Ce), new account".to_owned())), "{audit:?}");
    assert!(audit.iter().filter(|(action, detail)| *action == AuditAction::SignIn && detail == "Corp SSO").count() == 2, "{audit:?}");
}

#[tokio::test]
async fn registration_keeps_an_edited_address_unverified() {
    let fx = Fixture::new().await;
    let ExternalSignIn::Register(token) = fx.complete(identity("corp", "s", Some("a@corp.example"), true), None).await.unwrap() else {
        panic!("expected registration");
    };
    let (user, _) = fx.services.register_external("corp", &token, "first", "other@example.com", &client()).await.unwrap();
    assert!(user.is_admin, "the first account of an instance administers it, however it was created");
    assert_eq!(fx.database.recovery().verified_among(&[user.id()]).await.unwrap().len(), 0);

    // An address the provider didn't verify isn't marked verified either.
    let ExternalSignIn::Register(token) = fx.complete(identity("corp", "t", Some("b@corp.example"), false), None).await.unwrap() else {
        panic!("expected registration");
    };
    assert!(!fx.services.external_auth.registration_draft("corp", &token).unwrap().email_verified);
    let (second, _) = fx.services.register_external("corp", &token, "second", "b@corp.example", &client()).await.unwrap();
    assert_eq!(fx.database.recovery().verified_among(&[second.id()]).await.unwrap().len(), 0);
    // An address another account uses is refused with a way out.
    let ExternalSignIn::Register(token) = fx.complete(identity("corp", "u", None, false), None).await.unwrap() else { panic!() };
    let error = fx.services.register_external("corp", &token, "third", "b@corp.example", &client()).await.unwrap_err();
    assert!(error.to_string().contains("already uses this email address"), "{error}");
}

#[tokio::test]
async fn trusted_providers_link_by_verified_email_only() {
    let fx = Fixture::with(|config| config.auth.providers[0].trust_email = true).await;
    let alice = fx.user("alice").await;
    let bob = fx.user("bob").await;
    fx.verify_email(&alice).await;

    // Bob's address is not verified: never linked, registration is offered instead.
    let bob_identity = identity("corp", "bob-sub", Some("bob@example.com"), true);
    assert!(matches!(fx.complete(bob_identity, None).await.unwrap(), ExternalSignIn::Register(_)));
    // The provider didn't verify Alice's address: not linked either.
    assert!(matches!(
        fx.complete(identity("corp", "a-sub", Some("alice@example.com"), false), None).await.unwrap(),
        ExternalSignIn::Register(_)
    ));
    assert_eq!(fx.database.external_identities().list_for_account(alice.id()).await.unwrap().len(), 0);

    // Both verified: linked and signed in.
    let user = fx.signed_in(identity("corp", "a-sub", Some("ALICE@example.com"), true)).await;
    assert_eq!(user.id(), alice.id());
    assert!(
        fx.audit(&alice)
            .await
            .iter()
            .any(|(action, detail)| *action == AuditAction::ExternalIdentityLinked && detail.ends_with("by verified email"))
    );
    assert_eq!(fx.signed_in(identity("corp", "a-sub", None, false)).await.id(), alice.id());

    // GitHub doesn't have trust_email: the same verified address registers.
    assert!(matches!(
        fx.complete(identity("github", "1", Some("alice@example.com"), true), None).await.unwrap(),
        ExternalSignIn::Register(_)
    ));
    let _ = bob;
}

#[tokio::test]
async fn signed_in_users_link_from_settings() {
    let fx = Fixture::new().await;
    let alice = fx.user("alice").await;
    let bob = fx.user("bob").await;
    let external = &fx.services.external_auth;

    assert!(matches!(fx.complete(identity("github", "42", None, false), Some(alice.id())).await.unwrap(), ExternalSignIn::Linked));
    assert!(
        matches!(fx.complete(identity("github", "42", None, false), Some(alice.id())).await.unwrap(), ExternalSignIn::Linked),
        "again is fine"
    );
    let error = fx.complete(identity("github", "42", None, false), Some(bob.id())).await.unwrap_err();
    assert!(error.to_string().contains("already linked to another account"), "{error}");
    let error = fx.complete(identity("github", "43", None, false), Some(alice.id())).await.unwrap_err();
    assert!(error.to_string().contains("unlink it first"), "{error}");
    assert_eq!(fx.signed_in(identity("github", "42", None, false)).await.id(), alice.id());

    // Alice has a password, so the identity can go.
    external.unlink(&alice, "github", &client()).await.unwrap();
    assert!(matches!(external.unlink(&alice, "github", &client()).await, Err(ServiceError::NotFound)));
    let audit = fx.audit(&alice).await;
    assert!(audit.contains(&(AuditAction::ExternalIdentityUnlinked, "GitHub".to_owned())), "{audit:?}");
    assert!(audit.iter().any(|(action, _)| *action == AuditAction::ExternalIdentityLinked));
}

#[tokio::test]
async fn the_last_way_to_sign_in_stays() {
    let fx = Fixture::new().await;
    let carol = fx.register(identity("corp", "c", None, false), "carol").await;
    let external = &fx.services.external_auth;
    let error = external.unlink(&carol, "corp", &client()).await.unwrap_err();
    assert!(error.to_string().contains("only way to sign in"), "{error}");
    assert!(external.methods(&carol).await.unwrap().is_last_method("corp"));

    fx.complete(identity("github", "7", None, false), Some(carol.id())).await.unwrap();
    assert!(!external.methods(&carol).await.unwrap().is_last_method("corp"));
    external.unlink(&carol, "corp", &client()).await.unwrap();
    assert!(external.unlink(&carol, "github", &client()).await.is_err(), "now GitHub is the last one");
}

#[tokio::test]
async fn registration_follows_the_provider_and_the_site() {
    // Site registration closed (the instance already has users).
    let fx = Fixture::with(|config| {
        config.server.open_registration = false;
        config.auth.providers[1].allow_registration = ProviderRegistration::Always;
    })
    .await;
    fx.user("root").await;
    let error = fx.complete(identity("corp", "x", None, false), None).await.unwrap_err();
    assert!(error.to_string().contains("can't be created with it"), "{error}");
    assert!(error.to_string().contains("Settings → Security"), "{error}");
    assert!(matches!(fx.complete(identity("github", "x", None, false), None).await.unwrap(), ExternalSignIn::Register(_)), "\"always\"");

    let fx = Fixture::with(|config| config.auth.providers[0].allow_registration = ProviderRegistration::Never).await;
    fx.user("root").await;
    assert!(fx.complete(identity("corp", "x", None, false), None).await.is_err(), "false: never, even with sign-up open");
    assert!(matches!(fx.complete(identity("github", "x", None, false), None).await.unwrap(), ExternalSignIn::Register(_)));
}

#[tokio::test]
async fn two_factor_accounts_continue_at_the_second_step() {
    let fx = Fixture::with(|config| config.auth.providers[0].trust_mfa = true).await;
    let alice = fx.user("alice").await;
    fx.complete(identity("corp", "a", None, false), Some(alice.id())).await.unwrap();
    fx.complete(identity("github", "1", None, false), Some(alice.id())).await.unwrap();

    fx.services.two_factor.begin_enrolment(&alice, "rbit").await.unwrap();
    let record = fx.database.two_factor().find(alice.id()).await.unwrap().unwrap();
    let secret = TotpSecret::from_bytes(&record.secret).unwrap();
    let step = time_step(Timestamp::now().seconds());
    fx.services.confirm_two_factor(&alice, &format!("{:06}", secret.code_at_step(step)), &client()).await.unwrap();

    let ExternalSignIn::SecondFactorRequired(pending) = fx.complete(identity("github", "1", None, false), None).await.unwrap() else {
        panic!("expected the second step");
    };
    let code = format!("{:06}", secret.code_at_step(step + 1));
    let user = fx.services.complete_sign_in(&pending, &code, &client()).await.unwrap();
    assert_eq!(user.id(), alice.id());
    assert!(fx.audit(&alice).await.contains(&(AuditAction::SignIn, "GitHub and authenticator code".to_owned())));

    // The provider's MFA is trusted only when configured and reported.
    assert!(matches!(fx.complete(identity("corp", "a", None, false), None).await.unwrap(), ExternalSignIn::SecondFactorRequired(_)));
    let with_mfa = VerifiedIdentity { mfa: true, ..identity("corp", "a", None, false) };
    assert_eq!(fx.signed_in(with_mfa).await.id(), alice.id());
    assert!(fx.audit(&alice).await.contains(&(AuditAction::SignIn, "Corp SSO (provider MFA)".to_owned())));
    let github_mfa = VerifiedIdentity { mfa: true, ..identity("github", "1", None, false) };
    assert!(matches!(fx.complete(github_mfa, None).await.unwrap(), ExternalSignIn::SecondFactorRequired(_)), "not trusted for GitHub");
}

#[tokio::test]
async fn suspended_accounts_are_refused() {
    let fx = Fixture::new().await;
    let root = fx.user("root").await;
    let alice = fx.user("alice").await;
    fx.complete(identity("corp", "a", None, false), Some(alice.id())).await.unwrap();
    fx.services.admin.set_suspended(&root, alice.id(), true).await.unwrap();
    assert!(matches!(fx.complete(identity("corp", "a", None, false), None).await, Err(ServiceError::Forbidden)));
    let failures = fx.services.audit.list(&root, Some(AuditCategory::Authentication), &ListQuery::default()).await.unwrap();
    assert!(failures.items.iter().any(|event| event.action == AuditAction::SignInFailed && event.detail == "Corp SSO: account suspended"));
    assert!(
        matches!(fx.complete(identity("corp", "b", None, false), Some(alice.id())).await, Err(ServiceError::Forbidden)),
        "nor may they link"
    );
}

#[tokio::test]
async fn callbacks_need_the_flow_cookie_and_its_state() {
    let fx = Fixture::new().await;
    let external = &fx.services.external_auth;
    let providers = external.providers();
    assert_eq!(providers.iter().map(|provider| provider.name.as_str()).collect::<Vec<_>>(), ["Corp SSO", "GitHub"]);
    assert!(external.redirect_uri("github").ends_with("/login/github/callback"));

    // GitHub's authorization URL needs no discovery, so no network.
    let (url, cookie) = external.begin("github", "/somewhere", None).await.unwrap();
    assert!(url.starts_with("https://github.com/login/oauth/authorize?response_type=code&client_id=rbit&redirect_uri="), "{url}");
    assert!(url.contains("code_challenge_method=S256") && url.contains("scope=read%3Auser+user%3Aemail"), "{url}");
    let state = url.split("state=").nth(1).unwrap().split('&').next().unwrap().to_owned();
    assert!(matches!(external.begin("nope", "/", None).await, Err(ServiceError::NotFound)));

    let callback = |cookie: Option<String>, state: Option<String>, error: Option<&'static str>| {
        let external = external.clone();
        async move {
            let params = CallbackParams { code: Some("code"), state: state.as_deref(), error, error_description: None };
            external.callback("github", cookie.as_deref(), params, &client()).await
        }
    };
    let expired = callback(None, Some(state.clone()), None).await.unwrap_err().to_string();
    assert!(expired.contains("took too long"), "{expired}");
    assert!(callback(Some(cookie.clone()), Some("forged".to_owned()), None).await.is_err(), "state mismatch");
    assert!(callback(Some(format!("{cookie}x")), Some(state.clone()), None).await.is_err(), "tampered cookie");
    let denied = callback(Some(cookie.clone()), Some(state.clone()), Some("access_denied")).await.unwrap_err().to_string();
    assert!(denied.contains("GitHub didn't sign you in"), "{denied}");
    let other =
        external.callback("corp", Some(&cookie), CallbackParams { state: Some(&state), ..CallbackParams::default() }, &client()).await;
    assert!(other.is_err(), "a cookie for another provider");

    // Failures are audited and throttled per address.
    let mut limited = false;
    for _ in 0..40 {
        if let Err(ServiceError::RateLimited { .. }) = callback(None, None, None).await {
            limited = true;
            break;
        }
    }
    assert!(limited, "repeated failures run out of budget");

    // Link tokens belong to one session.
    let session = rbit_auth::SessionToken::generate();
    let token = external.link_token(&session).await.unwrap();
    assert!(external.check_link_token(&session, &token).await.unwrap());
    assert!(!external.check_link_token(&rbit_auth::SessionToken::generate(), &token).await.unwrap());
}

#[tokio::test]
async fn invalid_provider_configuration_stops_start_up() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().to_path_buf();
    config.auth.providers = vec![ProviderConfig { issuer: None, ..provider("corp", ProviderKind::Oidc) }];
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let error = Services::new(Arc::new(config), &database, &kv).await.unwrap_err();
    assert!(error.to_string().contains("`issuer` is required"), "{error}");
}
