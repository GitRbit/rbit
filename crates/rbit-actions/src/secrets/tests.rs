//! Value checks (moved from `rbit-services`) and the order and fail-closed rules of opening.
use std::cell::RefCell;

use super::*;

/// What `rbit_workflow::spec::MAX_SECRET_BYTES` allows, as the services adapter passes it.
const MAX_VALUE_BYTES: usize = 48 * 1024;

#[test]
fn values_are_checked() {
    assert!(check_value("secret", "x", MAX_VALUE_BYTES).is_ok());
    assert_eq!(check_value("secret", "", MAX_VALUE_BYTES), Err("a secret needs a value".to_owned()));
    assert_eq!(check_value("secret", "a\0b", MAX_VALUE_BYTES), Err("a secret must not contain NUL characters".to_owned()));
    assert!(check_value("secret", &"x".repeat(MAX_VALUE_BYTES), MAX_VALUE_BYTES).is_ok());
    assert_eq!(check_value("secret", &"x".repeat(MAX_VALUE_BYTES + 1), MAX_VALUE_BYTES), Err("a secret is at most 48 KiB".to_owned()));
    assert!(check_value("variable", &"x".repeat(MAX_VARIABLE_BYTES + 1), MAX_VARIABLE_BYTES).is_err());
}

#[test]
fn only_owner_secrets_may_be_limited_to_selected_repositories() {
    assert_eq!(check_visibility(true, false), Ok(()));
    assert_eq!(check_visibility(false, true), Ok(()));
    assert_eq!(check_visibility(false, false), Err("a repository secret is visible to its repository only".to_owned()));
}

#[test]
fn a_task_gets_the_first_of_each_name_and_fails_closed() {
    let opened = RefCell::new(Vec::new());
    let open = |value: &&str| {
        opened.borrow_mut().push((*value).to_owned());
        (*value != "stale").then(|| (*value).to_owned())
    };
    let given = [("TOKEN", &"repo"), ("TOKEN", &"stale"), ("OTHER", &"owner")];
    let secrets = open_task_secrets(given, open).unwrap();
    assert_eq!(secrets, BTreeMap::from([("OTHER".to_owned(), "owner".to_owned()), ("TOKEN".to_owned(), "repo".to_owned())]));
    assert_eq!(*opened.borrow(), ["repo", "owner"], "a shadowed secret is never opened");

    let failing = [("A", &"repo"), ("B", &"stale"), ("C", &"owner")];
    assert_eq!(open_task_secrets(failing, open), Err("B".to_owned()));
    opened.borrow_mut().clear();
    assert_eq!(open_environment_secrets(failing, open), Err("B".to_owned()));
    assert_eq!(*opened.borrow(), ["repo", "stale"], "nothing after the failing one is opened");
    let environment = open_environment_secrets([("X", &"one"), ("X", &"two")], open).unwrap();
    assert_eq!(environment, [("X".to_owned(), "one".to_owned()), ("X".to_owned(), "two".to_owned())], "no deduplication");
}
