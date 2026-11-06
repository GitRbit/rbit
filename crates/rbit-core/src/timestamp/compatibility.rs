//! Legacy constructors and aliases are the exact shared types, not conversion wrappers.

use std::any::TypeId;

#[test]
fn timestamp_module_root_and_contract_constructors_are_identical() {
    let module = super::Timestamp(-1);
    let root = crate::Timestamp(module.0);
    let shared: rbit_contracts::Timestamp = root;
    assert_eq!(shared.seconds(), -1);
    assert_eq!(TypeId::of::<crate::Timestamp>(), TypeId::of::<rbit_contracts::Timestamp>());
    assert_eq!(TypeId::of::<super::Timestamp>(), TypeId::of::<rbit_contracts::Timestamp>());
    assert_eq!(serde_json::to_string(&module).unwrap(), "-1");
    assert_eq!(serde_json::from_str::<crate::Timestamp>("-1").unwrap(), shared);
    assert_eq!(module.iso8601(), "1969-12-31T23:59:59Z");
    assert_eq!(module.date(), "Dec 31, 1969");
}

#[test]
fn naming_errors_keep_module_and_root_type_identity_and_validation_behavior() {
    let error: crate::names::NameError = rbit_contracts::NameError::InvalidEmail;
    let root: crate::NameError = error;
    assert_eq!(root, rbit_contracts::NameError::InvalidEmail);
    assert_eq!(TypeId::of::<crate::NameError>(), TypeId::of::<rbit_contracts::NameError>());
    assert_eq!(TypeId::of::<crate::names::NameError>(), TypeId::of::<rbit_contracts::NameError>());
    let username: Result<crate::AccountName, rbit_contracts::NameError> = "Settings".parse();
    assert_eq!(username.unwrap_err(), rbit_contracts::NameError::Reserved { kind: "username", value: "Settings".into() });
    let repository: Result<crate::RepoName, rbit_contracts::NameError> = "TeamS".parse();
    assert_eq!(repository.unwrap_err(), rbit_contracts::NameError::Reserved { kind: "repository name", value: "TeamS".into() });
}
