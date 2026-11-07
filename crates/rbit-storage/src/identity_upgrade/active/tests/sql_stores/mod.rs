use super::*;
use crate::sqlite::{external_identities::NewExternalIdentity, recovery::TokenPurpose};
use crate::{Database, NewUser, StorageError};
use rbit_core::{AccountId, Timestamp};
use rbit_identity_models::credential::{CredentialEpoch, CredentialProofWrite};
mod recovery;
mod retirement;
mod sealing;

struct SqlFixture {
    fixture: Fixture,
    accepted: ActiveIdentityPair,
    database: Database,
}
impl SqlFixture {
    async fn new() -> Self {
        let fixture = Fixture::new().await;
        let mut owner = prepared(&fixture).await;
        owner.publish_active_inner().await.unwrap();
        let mut accepted = owner.accept_active_inner().await.unwrap();
        let key = fixture.sql.with_file_name(crate::sealing::KEY_FILE_NAME);
        let database = accepted.database(&key).await.unwrap();
        Self { fixture, accepted, database }
    }
    async fn close(self) -> Fixture {
        let Self { fixture, accepted, database } = self;
        database.close().await;
        drop(database);
        let StoppedIdentityUpgrade { connection, db, .. } = accepted.owner;
        connection.close().await.unwrap();
        drop(db);
        fixture
    }
}
fn new_user() -> NewUser {
    NewUser {
        name: "charlie".parse().unwrap(),
        email: "charlie@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "fixture PHC".into(),
        is_admin: false,
    }
}
fn epoch(value: i64) -> CredentialEpoch {
    CredentialEpoch::new(value).unwrap()
}

#[tokio::test]
async fn every_pool_bearing_legacy_credential_write_refuses_before_sql() {
    let f = SqlFixture::new().await;
    let db = &f.database;
    let account = AccountId(1);
    let ((), work) = crate::count_work(async {
        assert!(db.accounts().create_user(new_user()).await.is_err());
        assert!(db.accounts().create_user_with_initial_password(new_user()).await.is_err());
        assert!(db.accounts().register_user(new_user(), true).await.is_err());
        assert!(
            db.external_identities()
                .register(
                    new_user(),
                    false,
                    NewExternalIdentity { provider_id: "fixture", subject: "subject", email: None, username: None },
                    true
                )
                .await
                .is_err()
        );
        assert!(db.profiles().set_password_hash(account, "replaced").await.is_err());
        assert!(db.profiles().set_password_hash_if_current(account, "replaced", "unchanged PHC").await.is_err());
        assert!(db.recovery().reset_password(&[1; 32], "replaced", Timestamp::now()).await.is_err());
        assert!(db.recovery().consume(&[1; 32], TokenPurpose::Reset, Timestamp::now()).await.is_err());
        assert!(db.two_factor().claim_step(account, &[1; 20], 1).await.is_err());
        assert!(db.two_factor().spend_recovery_code(account, &[1; 32]).await.is_err());
        assert!(db.two_factor().use_passkey(account, 999, 0, 1).await.is_err());
        assert!(db.credential_revocations().initialize_pair(&f.accepted.session_store().epoch_sidecar()).await.is_err());
        assert!(db.migrate().await.is_err());
    })
    .await;
    assert_eq!(work, crate::Work::default());
    assert_eq!(db.accounts().count_users().await.unwrap(), 2);
    assert_eq!(db.profiles().password_hash(account).await.unwrap().as_deref(), Some("unchanged PHC"));
}

#[tokio::test]
async fn shared_email_rename_locale_and_verification_paths_remain_available() {
    use rbit_identity_models::AccountLocale;
    let f = SqlFixture::new().await;
    let (changed, work) =
        crate::count_work(f.database.profiles().set_email_if_password(AccountId(1), &"new@example.com".parse().unwrap(), "unchanged PHC"))
            .await;
    assert!(changed.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 2));
    let (changed, work) =
        crate::count_work(f.database.accounts().rename_if_password(AccountId(1), &"renamed".parse().unwrap(), "unchanged PHC")).await;
    assert!(changed.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert!(f.database.preferences().set_locale(AccountId(1), Some(AccountLocale::Spanish)).await.unwrap());
    assert_eq!(f.database.preferences().locale(AccountId(1)).await.unwrap(), Some(AccountLocale::Spanish));
    assert!(f.database.recovery().consume(&[9; 32], TokenPurpose::Verify, Timestamp::now()).await.unwrap().is_none());
    let row = f.database.accounts().find_session_user_at_epoch(AccountId(1), false, epoch(1)).await.unwrap().unwrap();
    assert_eq!(row.user.name().as_str(), "renamed");
    assert_eq!(row.locale, Some(AccountLocale::Spanish));
}

#[tokio::test]
async fn original_claim_is_available_without_legacy_shared_helper_bypass() {
    let f = SqlFixture::new().await;
    let two_factor = f.database.two_factor();
    let secret = [7; 20];
    assert!(two_factor.begin_enrolment(AccountId(1), &secret).await.unwrap());
    assert_eq!(two_factor.confirm(AccountId(1), &secret, 1, &[[4; 32]]).await.unwrap(), Some(true));
    let (claimed, work) = crate::count_work(two_factor.spend_recovery_code_at_epoch(AccountId(1), &[4; 32], epoch(1))).await;
    assert_eq!(claimed.unwrap(), CredentialProofWrite::Claimed);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(two_factor.remaining_recovery_codes(AccountId(1)).await.unwrap(), 0);
}

#[tokio::test]
async fn caller_owned_legacy_transactions_cannot_obtain_private_active_write_authority() {
    let f = SqlFixture::new().await;
    // Public default registration deliberately denies v3, even though it knows the
    // layout. No public Database method exports its active pool or transaction.
    let mut connection = f.fixture.connection().await;
    let mut tx = connection.begin().await.unwrap();
    assert!(crate::sqlite::credential_revocations::stage(&mut tx, AccountId(1), Timestamp::now()).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(
        f.database.accounts().find_session_user_at_epoch(AccountId(1), false, epoch(1)).await.unwrap().unwrap().user.id(),
        AccountId(1)
    );
}
