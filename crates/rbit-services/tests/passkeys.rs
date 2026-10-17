//! Passkeys as a second factor: registration, the two-step sign-in, confirming sensitive
//! changes, and how passkeys and the authenticator app share recovery codes. A software
//! authenticator plays the browser's part against real, temporary SQLite and redb stores.

use std::sync::Arc;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer as _};
use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::{Config, Timestamp, User};
use rbit_services::{
    ServiceError, Services, SignUp,
    accounts::ClientInfo,
    admin::ListQuery,
    audit::{AuditAction, AuditCategory},
    two_factor::{GitPasswordOutcome, PasskeyRegistered, RECOVERY_CODE_COUNT, SecondFactorProof, SignInOutcome},
};
use rbit_storage::{Database, KvStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const PASSWORD: &str = "correct horse battery";
/// `server.public_url` of the default configuration.
const ORIGIN: &str = "http://localhost:3000";

struct Fixture {
    _dir: tempfile::TempDir,
    database: Database,
    services: Services,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
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

    async fn register(&self, user: &User, authenticator: &Authenticator, name: &str) -> Result<PasskeyRegistered, ServiceError> {
        let options = self.services.two_factor.creation_options(user).await.unwrap();
        self.services.add_passkey(user, name, &authenticator.create(&options), &client()).await
    }

    async fn pending(&self) -> rbit_auth::SessionToken {
        match self.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.unwrap() {
            SignInOutcome::SecondFactorRequired(token) => token,
            SignInOutcome::SignedIn(_) => panic!("expected a second step"),
        }
    }

    /// Sets up the authenticator app; returns its secret and the step used to confirm.
    async fn enrol_app(&self, user: &User) -> (TotpSecret, u64) {
        self.services.two_factor.begin_enrolment(user, "rbit").await.unwrap();
        let record = self.database.two_factor().find(user.id()).await.unwrap().unwrap();
        let secret = TotpSecret::from_bytes(&record.secret).unwrap();
        let step = time_step(Timestamp::now().seconds());
        self.services.confirm_two_factor(user, &format!("{:06}", secret.code_at_step(step)), &client()).await.unwrap();
        (secret, step)
    }
}

fn client() -> ClientInfo {
    ClientInfo { ip: Some("203.0.113.9".parse().unwrap()), user_agent: "test".to_owned() }
}

/// A software authenticator holding one P-256 key, with a signature counter.
struct Authenticator {
    key: SigningKey,
    id: Vec<u8>,
    counter: std::cell::Cell<u32>,
}

impl Authenticator {
    fn new(seed: u8) -> Self {
        Self { key: SigningKey::from_slice(&[seed; 32]).unwrap(), id: vec![seed; 20], counter: std::cell::Cell::new(0) }
    }

    fn auth_data(&self, rp_id: &str, attested: bool) -> Vec<u8> {
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(if attested { 0x41 } else { 0x01 });
        data.extend(self.counter.get().to_be_bytes());
        if attested {
            let point = self.key.verifying_key().to_sec1_point(false);
            let point = point.as_bytes();
            data.extend([0; 16]);
            data.extend(u16::try_from(self.id.len()).unwrap().to_be_bytes());
            data.extend(&self.id);
            // COSE_Key {1: 2, 3: -7, -1: 1, -2: x, -3: y}
            data.extend([0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20]);
            data.extend(&point[1..33]);
            data.extend([0x22, 0x58, 0x20]);
            data.extend(&point[33..65]);
        }
        data
    }

    fn client_data(kind: &str, options: &Value) -> Vec<u8> {
        json!({ "type": kind, "challenge": options["challenge"], "origin": ORIGIN }).to_string().into_bytes()
    }

    /// Answers `navigator.credentials.create`.
    fn create(&self, options: &str) -> String {
        let options: Value = serde_json::from_str(options).unwrap();
        let auth_data = self.auth_data(options["rp"]["id"].as_str().unwrap(), true);
        // {"fmt": "none", "attStmt": {}, "authData": <bytes>}
        let mut attestation = vec![0xa3, 0x63, b'f', b'm', b't', 0x64, b'n', b'o', b'n', b'e'];
        attestation.extend([0x67, b'a', b't', b't', b'S', b't', b'm', b't', 0xa0]);
        attestation.extend([0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a', 0x59]);
        attestation.extend(u16::try_from(auth_data.len()).unwrap().to_be_bytes());
        attestation.extend(auth_data);
        let id = URL_SAFE_NO_PAD.encode(&self.id);
        json!({
            "id": id, "rawId": id, "type": "public-key",
            "response": {
                "clientDataJSON": URL_SAFE_NO_PAD.encode(Self::client_data("webauthn.create", &options)),
                "attestationObject": URL_SAFE_NO_PAD.encode(attestation),
                "transports": ["usb"],
            }
        })
        .to_string()
    }

    /// Answers `navigator.credentials.get`.
    fn get(&self, options: &str) -> String {
        let options: Value = serde_json::from_str(options).unwrap();
        self.counter.set(self.counter.get() + 1);
        let auth_data = self.auth_data(options["rpId"].as_str().unwrap(), false);
        let client_data = Self::client_data("webauthn.get", &options);
        let mut signed = auth_data.clone();
        signed.extend(Sha256::digest(&client_data));
        let signature: Signature = self.key.sign(&signed);
        let id = URL_SAFE_NO_PAD.encode(&self.id);
        json!({
            "id": id, "rawId": id, "type": "public-key",
            "response": {
                "clientDataJSON": URL_SAFE_NO_PAD.encode(client_data),
                "authenticatorData": URL_SAFE_NO_PAD.encode(auth_data),
                "signature": URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes()),
            }
        })
        .to_string()
    }
}

#[tokio::test]
async fn a_passkey_alone_turns_on_two_factor_and_signs_in_once_per_challenge() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(1);

    let PasskeyRegistered::FirstFactor(codes) = fixture.register(&alice, &key, " YubiKey ").await.unwrap() else {
        panic!("the first passkey should issue recovery codes");
    };
    assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
    let status = fixture.services.two_factor.status(&alice).await.unwrap();
    assert!(status.is_enabled() && !status.has_authenticator());
    assert_eq!(status.passkeys.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["YubiKey"]);
    assert!(matches!(
        fixture.services.authenticate_git_password("alice", PASSWORD.to_owned(), &client()).await.unwrap(),
        GitPasswordOutcome::TokenRequired
    ));

    let pending = fixture.pending().await;
    // Without the page's challenge, a passkey answer is refused.
    let unasked = key.get(&fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap());
    assert!(matches!(
        fixture.services.complete_sign_in(&pending, SecondFactorProof::Passkey(&unasked), &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));

    let options = fixture.services.two_factor.sign_in_options(&pending).await.unwrap().expect("alice has a passkey");
    let answer = key.get(&options);
    let user = fixture.services.complete_sign_in(&pending, SecondFactorProof::Passkey(&answer), &client()).await.unwrap();
    assert_eq!(user.id(), alice.id());

    // The same answer can't open a second sign-in: its challenge is gone.
    let again = fixture.pending().await;
    fixture.services.two_factor.sign_in_options(&again).await.unwrap();
    assert!(matches!(
        fixture.services.complete_sign_in(&again, SecondFactorProof::Passkey(&answer), &client()).await,
        Err(ServiceError::InvalidCredentials)
    ));
    // Recovery codes work for passkey-only accounts too.
    fixture.services.complete_sign_in(&again, &codes[0], &client()).await.unwrap();

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::Authentication), &ListQuery::default()).await.unwrap();
    let details: Vec<_> = log.items.iter().map(|event| (event.action, event.detail.as_str())).collect();
    assert!(details.contains(&(AuditAction::SignIn, "password and passkey")), "{details:?}");
}

#[tokio::test]
async fn registration_refuses_duplicates_and_stale_challenges() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let bob = fixture.user("bob").await;
    let key = Authenticator::new(2);

    fixture.register(&alice, &key, "Laptop").await.unwrap();
    assert!(matches!(fixture.register(&alice, &key, "Again").await, Err(ServiceError::Invalid(_))));
    assert!(matches!(fixture.register(&bob, &key, "Stolen").await, Err(ServiceError::Invalid(_))));

    // A recent challenge stays valid while a newer one is out (a second tab), but each
    // works once, and a sign-in confirmation challenge can't register a passkey.
    let options = fixture.services.two_factor.creation_options(&alice).await.unwrap();
    let other = Authenticator::new(3);
    let answer = other.create(&options);
    fixture.services.two_factor.creation_options(&alice).await.unwrap();
    assert_eq!(fixture.services.add_passkey(&alice, "Phone", &answer, &client()).await.unwrap(), PasskeyRegistered::Added);
    assert!(matches!(fixture.services.add_passkey(&alice, "Phone", &answer, &client()).await, Err(ServiceError::Invalid(_))));

    let confirm = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let mut wrong_ceremony: Value = serde_json::from_str(&confirm).unwrap();
    wrong_ceremony["rp"] = json!({ "id": "localhost" });
    let answer = Authenticator::new(6).create(&wrong_ceremony.to_string());
    assert!(matches!(fixture.services.add_passkey(&alice, "Tablet", &answer, &client()).await, Err(ServiceError::Invalid(_))));
    assert_eq!(fixture.services.two_factor.status(&alice).await.unwrap().passkeys.len(), 2);
}

#[tokio::test]
async fn factors_share_recovery_codes_until_the_last_one_goes() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(4);
    let (secret, step) = fixture.enrol_app(&alice).await;

    // Adding a passkey next to the app keeps the existing codes.
    assert_eq!(fixture.register(&alice, &key, "Key").await.unwrap(), PasskeyRegistered::Added);
    let codes_left = || async { fixture.services.two_factor.status(&alice).await.unwrap().recovery_codes_left };
    assert_eq!(codes_left().await, i64::try_from(RECOVERY_CODE_COUNT).unwrap());

    // Removing the app, confirmed with the passkey, leaves 2FA on with the same codes.
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    fixture
        .services
        .disable_two_factor(&alice, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();
    let status = fixture.services.two_factor.status(&alice).await.unwrap();
    assert!(status.is_enabled() && !status.has_authenticator());
    assert_eq!(status.recovery_codes_left, i64::try_from(RECOVERY_CODE_COUNT).unwrap());

    // An app code no longer counts; removing the last passkey needs a passkey or recovery code.
    let passkey = status.passkeys[0].id;
    let app_code = format!("{:06}", secret.code_at_step(step + 1));
    let refused = fixture.services.remove_passkey(&alice, passkey, PASSWORD.to_owned(), &app_code, &client()).await;
    assert!(matches!(refused, Err(ServiceError::InvalidCredentials)));
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let wrong_password = fixture
        .services
        .remove_passkey(&alice, passkey, "nope".to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await;
    assert!(matches!(wrong_password, Err(ServiceError::InvalidCredentials)));
    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    fixture
        .services
        .remove_passkey(&alice, passkey, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();

    assert!(!fixture.services.two_factor.is_enabled(alice.id()).await.unwrap());
    assert_eq!(codes_left().await, 0);
    assert!(matches!(fixture.services.sign_in("alice", PASSWORD.to_owned(), &client()).await.unwrap(), SignInOutcome::SignedIn(_)));

    let log = fixture.services.audit.list(&alice, Some(AuditCategory::TwoFactor), &ListQuery::default()).await.unwrap();
    let actions: Vec<_> = log.items.iter().map(|event| event.action).collect();
    assert_eq!(
        actions,
        [AuditAction::PasskeyRemoved, AuditAction::TwoFactorDisabled, AuditAction::PasskeyAdded, AuditAction::TwoFactorEnabled]
    );
}

#[tokio::test]
async fn recovery_codes_can_be_regenerated_with_a_passkey() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let key = Authenticator::new(5);
    let PasskeyRegistered::FirstFactor(old) = fixture.register(&alice, &key, "Key").await.unwrap() else { panic!() };

    let options = fixture.services.two_factor.confirmation_options(&alice).await.unwrap().unwrap();
    let fresh = fixture
        .services
        .regenerate_recovery_codes(&alice, PASSWORD.to_owned(), SecondFactorProof::Passkey(&key.get(&options)), &client())
        .await
        .unwrap();
    assert_eq!(fresh.len(), RECOVERY_CODE_COUNT);
    let pending = fixture.pending().await;
    assert!(fixture.services.complete_sign_in(&pending, &old[0], &client()).await.is_err());
    fixture.services.complete_sign_in(&pending, &fresh[0], &client()).await.unwrap();
}
