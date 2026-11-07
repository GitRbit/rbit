use rbit_identity::accounts::{AccountError, AccountRegistration, SignUp, bootstrap_setup_token};
use rbit_kernel::AdmissionError;

use super::*;

#[tokio::test]
async fn setup_token_closed_registration_and_live_setting_order_remain_exact() {
    let repository = Repository::default();
    let work = work();
    let occupied = work.begin().unwrap();
    let registration = || AccountRegistration::new(&repository, &work, 8, Some("expected"));
    assert!(!format!("{:?}", registration()).contains("expected"));
    assert!(matches!(registration().sign_up(input(), "wrong", || false).await, Err(AccountError::SetupTokenRequired)));
    assert_eq!(repository.0.lock().unwrap().calls, ["count"]);
    assert!(matches!(
        AccountRegistration::new(&repository, &work, 8, None).sign_up(input(), "expected", || true).await,
        Err(AccountError::SetupTokenRequired)
    ));
    assert!(matches!(registration().sign_up(input(), " expected \t", || false).await, Err(AccountError::Admission(AdmissionError::Busy))));
    repository.0.lock().unwrap().count = 1;
    assert!(matches!(registration().sign_up(input(), "expected", || false).await, Err(AccountError::RegistrationClosed)));
    assert_eq!(repository.0.lock().unwrap().writes, 0);
    drop(occupied);
    repository.0.lock().unwrap().count = 0;
    let policy_repository = repository.clone();
    registration()
        .sign_up(input(), " expected \t", move || {
            assert_eq!(policy_repository.0.lock().unwrap().calls.last().unwrap(), "count");
            false
        })
        .await
        .unwrap();
    assert_eq!(repository.0.lock().unwrap().calls.last().unwrap(), "register:false:false");
}

#[tokio::test]
async fn bootstrap_generation_and_open_short_circuit_keep_existing_policy() {
    let repository = Repository::default();
    let work = work();
    assert_eq!(bootstrap_setup_token(&repository, Some(" configured \n")).await.unwrap().as_deref(), Some("configured"));
    let generated = bootstrap_setup_token(&repository, Some(" \t")).await.unwrap().unwrap();
    assert_eq!(generated.len(), 32);
    assert!(generated.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
    repository.0.lock().unwrap().count = 1;
    assert!(bootstrap_setup_token(&repository, Some("configured")).await.unwrap().is_none());
    repository.0.lock().unwrap().calls.clear();
    let app = AccountRegistration::new(&repository, &work, 8, None);
    assert!(app.registration_open(true).await.unwrap());
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert!(app.sign_up(input(), "anything", || true).await.is_ok(), "existing instance ignores missing setup token");
}

#[tokio::test]
async fn repository_arbitrates_registration_race_and_provisioning_modes() {
    let repository = Repository::default();
    let work = work();
    let app = || AccountRegistration::new(&repository, &work, 8, Some("setup"));
    repository.0.lock().unwrap().reject_registration = true;
    assert!(matches!(
        app().sign_up(input(), "setup", || false).await,
        Err(AccountError::Store(StoreError("registration transaction rejected")))
    ));
    assert_eq!(repository.0.lock().unwrap().writes, 0);
    repository.0.lock().unwrap().reject_registration = false;
    app().create_user(input(), None).await.unwrap();
    app().create_user(input(), Some(true)).await.unwrap();
    app().create_user_with_initial_password(input(), false).await.unwrap();
    assert!(repository.0.lock().unwrap().calls.ends_with(&[
        "register:true:false".into(),
        "explicit:false:true".into(),
        "initial:false:false".into()
    ]));
}

#[tokio::test]
async fn validation_precedes_admission_and_dropped_unpolled_work_does_nothing() {
    let repository = Repository::default();
    let work = work();
    let occupied = work.begin().unwrap();
    let app = || AccountRegistration::new(&repository, &work, 8, None);
    let future = app().create_user(input(), None);
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    drop(future);
    for (input, expected) in [
        (SignUp { username: "../".into(), ..input() }, "name"),
        (SignUp { email: "not-email".into(), ..input() }, "name"),
        (SignUp { password: "short".into(), ..input() }, "password must be at least 8 characters"),
        (SignUp { password: "雪".repeat(257), ..input() }, "password must be at most 256 characters"),
    ] {
        let error = app().create_user(input, None).await.unwrap_err();
        match error {
            AccountError::Name(_) => assert_eq!(expected, "name"),
            AccountError::Invalid(text) => assert_eq!(text, expected),
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(matches!(
        app().create_user(SignUp { password: "雪".repeat(256), ..input() }, None).await,
        Err(AccountError::Admission(AdmissionError::Busy))
    ));
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 1);
    drop(occupied);
}
