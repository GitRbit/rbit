//! Receipted signup uses the committed port facts and the shared setup/hash policy.
use super::fixture::*;
use rbit_identity::accounts::{AccountCreationRepository, AccountError, AccountRegistration, CreatedAccountSnapshot, NewUser};
use rbit_identity_models::credential::CredentialEpoch;
use rbit_kernel::AdmissionError;

impl AccountCreationRepository for Repository {
    async fn create_user_original(&self, new: NewUser) -> Result<CreatedAccountSnapshot, Self::Error> {
        let user = rbit_identity::accounts::AccountRepository::create_user(self, new).await?;
        Ok(CreatedAccountSnapshot { user, epoch: CredentialEpoch::new(43).unwrap() })
    }
    async fn create_user_with_initial_password_original(&self, new: NewUser) -> Result<CreatedAccountSnapshot, Self::Error> {
        let user = rbit_identity::accounts::AccountRepository::create_user_with_initial_password(self, new).await?;
        Ok(CreatedAccountSnapshot { user, epoch: CredentialEpoch::new(44).unwrap() })
    }

    async fn register_user_original(&self, new: NewUser, open: bool) -> Result<CreatedAccountSnapshot, Self::Error> {
        let user = rbit_identity::accounts::AccountRepository::register_user(self, new, open).await?;
        // Deliberately nonzero port fact proves no hardcoded/latest-epoch reconstruction.
        Ok(CreatedAccountSnapshot { user, epoch: CredentialEpoch::new(42).unwrap() })
    }
}

#[tokio::test]
async fn original_signup_shares_setup_precedence_and_mints_only_after_port_commit_result() {
    let repository = Repository::default();
    let work = work();
    let app = || AccountRegistration::new(&repository, &work, 8, Some("setup"));
    let occupied = work.begin().unwrap();
    assert!(matches!(app().sign_up_original(input(), "wrong", || false).await, Err(AccountError::SetupTokenRequired)));
    assert!(matches!(app().sign_up_original(input(), " setup \t", || false).await, Err(AccountError::Admission(AdmissionError::Busy))));
    repository.0.lock().unwrap().count = 1;
    assert!(matches!(app().sign_up_original(input(), "setup", || false).await, Err(AccountError::RegistrationClosed)));
    assert_eq!(repository.0.lock().unwrap().writes, 0);
    drop(occupied);
    repository.0.lock().unwrap().count = 0;
    repository.0.lock().unwrap().reject_registration = true;
    assert!(matches!(
        app().sign_up_original(input(), "setup", || false).await,
        Err(AccountError::Store(StoreError("registration transaction rejected")))
    ));
    repository.0.lock().unwrap().reject_registration = false;
    repository.0.lock().unwrap().calls.clear();
    let original = app().sign_up_original(input(), " setup \t", || false).await.unwrap();
    assert_eq!(original.receipt().account(), user().id());
    assert_eq!(original.receipt().original_epoch().get(), 42);
    assert_eq!(repository.0.lock().unwrap().calls, ["count", "register:false:false"]);
    assert!(!format!("{original:?}").contains(PASSWORD));
}

#[tokio::test]
async fn original_signup_is_protected_and_inert_when_unpolled() {
    let repository = Repository::default();
    let work = work();
    let future = AccountRegistration::new(&repository, &work, 8, Some("setup")).sign_up_original(input(), "setup", || false);
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 0);
    drop(future);
    drop(AccountRegistration::new(&repository, &work, 8, None).create_user_original(input(), None));
    drop(AccountRegistration::new(&repository, &work, 8, None).create_user_with_initial_password_original(input(), true));
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 0);
    assert_eq!(repository.0.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn original_provisioning_preserves_selected_store_policy_and_worker_release() {
    let repository = Repository::default();
    let work = work();
    let app = || AccountRegistration::new(&repository, &work, 8, None);
    let first = app().create_user_original(input(), None).await.unwrap();
    assert_eq!(first.receipt().original_epoch().get(), 42);
    let explicit = app().create_user_original(input(), Some(true)).await.unwrap();
    assert_eq!(explicit.receipt().original_epoch().get(), 43);
    assert!(explicit.user().is_admin);
    let temporary = app().create_user_with_initial_password_original(input(), false).await.unwrap();
    assert_eq!(temporary.receipt().original_epoch().get(), 44);
    assert_eq!(repository.0.lock().unwrap().calls, ["register:true:false", "explicit:false:true", "initial:false:false"]);
    assert_eq!(work.admitted(), 0);
}
