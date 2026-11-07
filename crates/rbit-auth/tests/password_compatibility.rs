//! Legacy module/root paths remain the exact identity credential types and errors.

use std::any::TypeId;

#[test]
fn legacy_password_types_and_constructor_remain_identical() {
    let legacy = rbit_auth::password::Passwords;
    let root: rbit_auth::Passwords = legacy;
    let identity: rbit_identity::Passwords = root;
    let _: rbit_auth::password::Passwords = identity;
    assert_eq!(TypeId::of::<rbit_auth::Passwords>(), TypeId::of::<rbit_identity::Passwords>());
    assert_eq!(TypeId::of::<rbit_auth::password::PasswordError>(), TypeId::of::<rbit_identity::PasswordError>());
}

#[tokio::test]
async fn legacy_hashes_verify_through_identity_and_error_variants_remain_available() {
    let password = "retained password facade";
    let hash = rbit_auth::Passwords::hash(password.to_owned()).await.unwrap();
    assert!(rbit_identity::Passwords::verify(password.to_owned(), hash).await.unwrap());
    assert!(!rbit_auth::password::Passwords::verify(password.to_owned(), "malformed".into()).await.unwrap());

    let failure = tokio::task::spawn_blocking(|| panic!("simulated worker failure")).await.unwrap_err();
    let legacy = rbit_auth::password::PasswordError::Join(failure);
    let root: rbit_auth::PasswordError = legacy;
    let identity: rbit_identity::PasswordError = root;
    assert!(matches!(identity, rbit_identity::PasswordError::Join(_)));
    let _: fn(argon2::password_hash::Error) -> rbit_auth::PasswordError = rbit_auth::password::PasswordError::Hash;
}
