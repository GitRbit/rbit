//! Dormant provider producers over actual temporary SQLite/redb, not remote provider fixtures.
use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::{
    AccountId, Config, Timestamp, User,
    config::{ProviderConfig, ProviderKind},
};
use rbit_identity::accounts::{ProviderAuthentication, ProviderProofError};
use rbit_identity_models::sessions::ClientInfo;
use rbit_services::{
    ServiceError, Services, SignUp,
    audit::AuditAction,
    external_auth::{Callback, ExternalSignIn, OriginalExternalSignIn, VerifiedIdentity},
};
use rbit_storage::{
    Database, KvStore, Work, count_work,
    sqlite::{credential_revocations as intents, writer_compatibility::register},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::sync::Arc;

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
    raw: SqlitePool,
}
impl Fixture {
    async fn new(trust_email: bool, trust_mfa: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        config.auth.providers = vec![ProviderConfig {
            id: "corp".into(),
            name: "Corp SSO".into(),
            kind: ProviderKind::Oidc,
            issuer: Some("https://sso.example.com".into()),
            client_id: "rbit".into(),
            client_secret: "fixture".into(),
            trust_email,
            trust_mfa,
            ..ProviderConfig::default()
        }];
        let raw = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(register(
                SqliteConnectOptions::new().filename(config.storage.sqlite_path()).create_if_missing(true).foreign_keys(true),
            ))
            .await
            .unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let sessions = kv.sessions().epoch_sidecar();
        sessions.initialize_identity_if_empty().await.unwrap();
        database.credential_revocations().initialize_pair(&sessions).await.unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        Self { _dir: dir, database, services, raw }
    }
    async fn user(&self, name: &str) -> User {
        self.services
            .accounts
            .create_user_original(
                SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() },
                None,
            )
            .await
            .unwrap()
    }
    async fn linked(&self, user: &User, subject: &str) {
        assert!(matches!(
            self.services.complete_external_sign_in(callback(subject, None, false, Some(user.id())), &client()).await.unwrap(),
            ExternalSignIn::Linked
        ));
    }
    async fn stage(&self, account: AccountId) -> intents::RevocationIntent {
        let original: i64 = sqlx::query_scalar("SELECT authentication_epoch FROM user_logins WHERE account_id=?")
            .bind(account.0)
            .fetch_one(&self.raw)
            .await
            .unwrap();
        self.database
            .profiles()
            .set_password_hash_with_intent(
                account,
                "fixture reset",
                rbit_identity_models::credential::CredentialEpoch::new(original).unwrap(),
                Timestamp(20),
            )
            .await
            .unwrap()
            .unwrap()
    }
    async fn complete_intent(&self, intent: &intents::RevocationIntent) {
        assert_eq!(
            self.services.accounts.recover_original_session_intent(*intent).await.unwrap(),
            rbit_identity::sessions::SessionRecoveryDisposition::Completed
        );
    }
    async fn enrol(&self, user: &User) -> Vec<String> {
        self.services.two_factor.begin_enrolment(user, "rbit").await.unwrap();
        let record = self.database.two_factor().find(user.id()).await.unwrap().unwrap();
        let secret = TotpSecret::from_bytes(&record.secret).unwrap();
        let code = format!("{:06}", secret.code_at_step(time_step(Timestamp::now().seconds())));
        self.services.confirm_two_factor(user, &code, &client()).await.unwrap().unwrap()
    }
}
fn client() -> ClientInfo {
    ClientInfo { ip: None, user_agent: "fixture".into() }
}
fn callback(subject: &str, email: Option<&str>, mfa: bool, link: Option<AccountId>) -> Callback {
    Callback {
        identity: VerifiedIdentity {
            provider_id: "corp".into(),
            subject: subject.into(),
            email: email.map(str::to_owned),
            email_verified: true,
            username: Some("fixture".into()),
            display_name: None,
            mfa,
        },
        next: "/next".into(),
        link_account: link,
    }
}
fn signed(result: OriginalExternalSignIn) -> rbit_services::accounts::ProvenAccount {
    match result {
        OriginalExternalSignIn::SignedIn(proven) => proven,
        other => panic!("expected original account: {other:?}"),
    }
}
fn pending(result: OriginalExternalSignIn) -> rbit_auth::SessionToken {
    match result {
        OriginalExternalSignIn::SecondFactorRequired(token) => token,
        other => panic!("expected MFA: {other:?}"),
    }
}

#[tokio::test]
async fn linked_and_verified_email_paths_preserve_legacy_statement_budget_and_original_epoch() {
    let fixture = Fixture::new(true, false).await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    let (legacy, before) = count_work(fixture.services.complete_external_sign_in(callback("linked", None, false, None), &client())).await;
    assert!(matches!(legacy.unwrap(), ExternalSignIn::SignedIn(_)));
    let (original, after) =
        count_work(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client())).await;
    assert_eq!(after.statements + 1, before.statements, "the sole final projection moves to the fenced issuer");
    assert_eq!(after.acquisitions + 1, before.acquisitions);
    let original = signed(original.unwrap());
    assert_eq!(original.user().id(), alice.id());
    assert_eq!(original.receipt().original_epoch().get(), 1);
    let (started, issuer_work) = count_work(fixture.services.accounts.start_original_session_for(original.receipt(), &client())).await;
    assert_eq!(started.unwrap().session.user().user.id(), alice.id());
    assert_eq!(issuer_work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert_eq!(after.statements + issuer_work.statements, before.statements);
    let charlie = fixture.user("charlie").await;
    fixture.database.recovery().mark_verified(charlie.id(), &charlie.email, Timestamp::now()).await.unwrap();
    let (legacy, before) =
        count_work(fixture.services.complete_external_sign_in(callback("email-one", Some(charlie.email.as_str()), false, None), &client()))
            .await;
    assert!(matches!(legacy.unwrap(), ExternalSignIn::SignedIn(_)));
    // A different provider/account is required by the one-identity-per-provider rule.
    let bob = fixture.user("bob").await;
    fixture.database.recovery().mark_verified(bob.id(), &bob.email, Timestamp::now()).await.unwrap();
    let (result, after) = count_work(
        fixture.services.complete_external_sign_in_original(callback("email-two", Some(bob.email.as_str()), false, None), &client()),
    )
    .await;
    assert_eq!(after.statements + 1, before.statements);
    assert_eq!(after.acquisitions + 1, before.acquisitions);
    let proven = signed(result.unwrap());
    assert_eq!(proven.receipt().account(), bob.id());
    let (_, issuer) = count_work(fixture.services.accounts.start_original_session_for(proven.receipt(), &client())).await;
    assert_eq!(after.statements + issuer.statements, before.statements);
}

#[tokio::test]
async fn first_link_or_email_capture_cannot_refresh_after_reset_or_pending_completion() {
    let fixture = Fixture::new(true, false).await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    fixture.database.recovery().mark_verified(alice.id(), &alice.email, Timestamp::now()).await.unwrap();
    let store = fixture.database.external_identities();
    let (_, linked) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    let email = ProviderAuthentication::capture_verified_email(&store, alice.email.as_str()).await.unwrap().unwrap();
    let intent = fixture.stage(alice.id()).await;
    fixture.complete_intent(&intent).await;
    for origin in [linked, email] {
        assert!(matches!(ProviderAuthentication::complete(&store, origin).await, Err(ProviderProofError::Unavailable)));
    }
    let intent = fixture.stage(alice.id()).await;
    let (_, captured_pending) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    fixture.complete_intent(&intent).await;
    assert!(matches!(ProviderAuthentication::complete(&store, captured_pending).await, Err(ProviderProofError::Unavailable)));
    let proven =
        signed(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await.unwrap());
    assert_eq!(proven.receipt().original_epoch().get(), intent.target_epoch);
}

#[tokio::test]
async fn provider_mfa_keeps_receipt_audit_method_and_wrong_mode_cannot_consume_recovery() {
    let fixture = Fixture::new(false, true).await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    let codes = fixture.enrol(&alice).await;
    let token =
        pending(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await.unwrap());
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    let (result, work) = count_work(fixture.services.complete_sign_in(&token, &codes[0], &client())).await;
    assert!(matches!(result, Err(ServiceError::NotFound)));
    assert_eq!(work, Work::default());
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    let original = fixture.services.complete_original_sign_in(&token, &codes[0], &client()).await.unwrap();
    assert_eq!(original.receipt().original_epoch().get(), 1);
    let audit = fixture.services.audit.recent_for(&alice, 50).await.unwrap();
    assert!(audit.iter().any(|entry| entry.action == AuditAction::RecoveryCodeUsed && entry.detail == "Corp SSO and recovery code"));
    let trusted =
        signed(fixture.services.complete_external_sign_in_original(callback("linked", None, true, None), &client()).await.unwrap());
    assert_eq!(trusted.receipt().account(), alice.id());
    assert!(
        fixture
            .services
            .audit
            .recent_for(&alice, 50)
            .await
            .unwrap()
            .iter()
            .any(|entry| entry.action == AuditAction::SignIn && entry.detail == "Corp SSO (provider MFA)")
    );
}

#[tokio::test]
async fn stale_provider_mfa_does_not_consume_one_use_proof_and_new_origin_can_retry() {
    let fixture = Fixture::new(false, false).await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    let codes = fixture.enrol(&alice).await;
    let token =
        pending(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await.unwrap());
    let intent = fixture.stage(alice.id()).await;
    fixture.complete_intent(&intent).await;
    let before = fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap();
    assert!(fixture.services.complete_original_sign_in(&token, &codes[0], &client()).await.is_err());
    assert_eq!(fixture.database.two_factor().remaining_recovery_codes(alice.id()).await.unwrap(), before);
    let fresh =
        pending(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await.unwrap());
    let proven = fixture.services.complete_original_sign_in(&fresh, &codes[0], &client()).await.unwrap();
    assert_eq!(proven.receipt().original_epoch().get(), intent.target_epoch);
}

#[tokio::test]
async fn external_creation_uses_same_transaction_budget_and_preserves_pending_retry_semantics() {
    let fixture = Fixture::new(true, false).await;
    fixture.user("root").await;
    let ExternalSignIn::Register(legacy) = fixture
        .services
        .complete_external_sign_in(callback("legacy-new", Some("legacy@example.com"), false, None), &client())
        .await
        .unwrap()
    else {
        panic!("registration")
    };
    let (legacy_created, old_work) =
        count_work(fixture.services.register_external("corp", &legacy, "legacy", "legacy@example.com", &client())).await;
    assert_eq!(legacy_created.unwrap().1, "/next");
    let OriginalExternalSignIn::Register(token) = fixture
        .services
        .complete_external_sign_in_original(callback("new", Some("new@example.com"), false, None), &client())
        .await
        .unwrap()
    else {
        panic!("registration")
    };
    assert!(matches!(
        fixture.services.register_external_original("corp", &token, "root", "new@example.com", &client()).await,
        Err(ServiceError::Conflict("username"))
    ));
    assert!(fixture.services.external_auth.registration_draft("corp", &token).is_some());
    let (result, new_work) =
        count_work(fixture.services.register_external_original("corp", &token, "newuser", "new@example.com", &client())).await;
    let (created, next) = result.unwrap();
    assert_eq!(new_work.acquisitions, old_work.acquisitions);
    assert_eq!(new_work.statements, old_work.statements + 1, "one global allocation, no post-commit epoch read");
    assert_eq!(created.receipt().account(), created.user().id());
    assert_eq!(created.receipt().original_epoch().get(), 2);
    assert_eq!(next, "/next");
    assert!(!created.user().is_admin);
    assert!(fixture.database.recovery().verified_among(&[created.user().id()]).await.unwrap().contains(&created.user().id()));
    assert!(fixture.services.external_auth.registration_draft("corp", &token).is_none());
    assert!(matches!(
        fixture.services.register_external_original("corp", &token, "again", "again@example.com", &client()).await,
        Err(ServiceError::NotFound)
    ));
    let intent = fixture.stage(created.user().id()).await;
    fixture.complete_intent(&intent).await;
    assert_eq!(created.receipt().original_epoch().get(), 2);
    assert!(
        fixture
            .database
            .accounts()
            .find_session_user_at_epoch(created.user().id(), false, created.receipt().original_epoch())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn suspended_pending_account_keeps_forbidden_precedence_and_reactivation_reads_authoritative_flags() {
    let fixture = Fixture::new(false, false).await;
    let root = fixture.user("root").await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    let intent = fixture.stage(alice.id()).await;
    fixture.services.admin.set_suspended(&root, alice.id(), true).await.unwrap();
    assert!(matches!(
        fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await,
        Err(ServiceError::Forbidden)
    ));
    fixture.services.admin.set_suspended(&root, alice.id(), false).await.unwrap();
    assert!(matches!(
        fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await,
        Err(ServiceError::Unavailable { .. })
    ));
    fixture.complete_intent(&intent).await;
    let store = fixture.database.external_identities();
    let (_, origin) = ProviderAuthentication::capture_linked(&store, "corp", "linked").await.unwrap().unwrap();
    sqlx::query("UPDATE accounts SET name='renamed' WHERE id=?").bind(alice.id().0).execute(&fixture.raw).await.unwrap();
    sqlx::query("UPDATE user_logins SET is_admin=1 WHERE account_id=?").bind(alice.id().0).execute(&fixture.raw).await.unwrap();
    let proven = ProviderAuthentication::complete(&store, origin).await.unwrap();
    assert_eq!(proven.user().name().as_str(), "renamed");
    assert!(proven.user().is_admin);
    assert_eq!(proven.receipt().original_epoch().get(), intent.target_epoch);
}

#[tokio::test]
async fn missing_login_is_corrupt_identity_not_registration_fallback() {
    let fixture = Fixture::new(false, false).await;
    let alice = fixture.user("alice").await;
    fixture.linked(&alice, "linked").await;
    sqlx::query("DELETE FROM user_logins WHERE account_id=?").bind(alice.id().0).execute(&fixture.raw).await.unwrap();
    let store = fixture.database.external_identities();
    assert!(store.find("corp", "linked").await.unwrap().is_some());
    assert!(ProviderAuthentication::capture_linked(&store, "corp", "linked").await.is_err());
    assert!(fixture.services.complete_external_sign_in_original(callback("linked", None, false, None), &client()).await.is_err());
}

mod issuer;
