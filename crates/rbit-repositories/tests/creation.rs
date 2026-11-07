//! Owner authority and exact metadata/disk/grant/publication ordering without infrastructure.
#[path = "support/creation.rs"]
mod fixture;
use fixture::{Bridge, CreationFailures, input, run, user};
use rbit_repositories::{
    Visibility,
    creation::{RepositoryCreation, RepositoryCreationError, RepositoryMembership, RepositoryOwnerError, RepositoryOwnership},
};
#[test]
fn diagnostic_formatting_needs_no_debug_port_or_infrastructure_calls() {
    // Bridge intentionally implements no Debug. Formatting this application must
    // remain available without imposing that bound on its concrete port owner.
    let port = Bridge::default();
    let diagnostic = format!("{:?}", RepositoryCreation::new(&port));
    assert!(diagnostic.contains("RepositoryCreation"));
    assert!(!diagnostic.contains("acme"), "retained owner records are not diagnostic fields");
    assert_eq!(port.calls(), Vec::<String>::new());
}
#[test]
fn self_owner_and_unpolled_requests_keep_zero_lookup_fast_path() {
    let actor = user();
    let port = Bridge::default();
    let app = RepositoryCreation::new(&port);
    drop(app.create_in(&actor, "alice", input("demo")));
    assert_eq!(port.calls(), Vec::<String>::new());
    let owner = run(RepositoryOwnership::resolve(&port, &actor, "ALIce")).unwrap();
    assert_eq!(owner.id, actor.id());
    assert_eq!(port.calls(), Vec::<String>::new());
    let created = run(app.create_in(&actor, " alice ", input(" demo "))).unwrap();
    assert_eq!(created.repo.name.as_str(), "demo");
    assert_eq!(port.calls(), ["default_branch", "create", "disk", "created"]);
}
#[test]
fn owner_failure_precedes_name_description_and_default_branch_work() {
    let port = Bridge { owner: None, ..Bridge::default() };
    let actor = user();
    assert!(matches!(
        run(RepositoryCreation::new(&port).create_in(&actor, "ghost", input("bad name"))),
        Err(RepositoryCreationError::Owner(RepositoryOwnerError::Invalid(_)))
    ));
    assert_eq!(port.calls(), ["owner:ghost"]);
    let port = Bridge::default();
    assert!(matches!(run(RepositoryCreation::new(&port).create_in(&actor, "", input("bad name"))), Err(RepositoryCreationError::Name(_))));
    assert_eq!(port.calls(), Vec::<String>::new());
}
#[test]
fn administrator_skips_membership_but_cannot_create_for_another_person() {
    let mut actor = user();
    actor.is_admin = true;
    let port = Bridge::default();
    run(RepositoryCreation::new(&port).create_in(&actor, "acme", input("demo"))).unwrap();
    assert_eq!(port.calls(), ["owner:acme", "default_branch", "create", "disk", "created"]);
    let port = Bridge { owner: Some(user().account), ..Bridge::default() };
    assert!(matches!(run(RepositoryOwnership::resolve(&port, &actor, "other-user")), Err(RepositoryOwnerError::Invalid(_))));
    assert_eq!(port.calls(), ["owner:other-user"]);
}
#[test]
fn member_permission_is_reloaded_after_disk_and_only_current_member_gets_admin_grant() {
    let actor = user();
    let port = Bridge::default();
    run(RepositoryCreation::new(&port).create_in(&actor, "acme", input("demo"))).unwrap();
    assert_eq!(port.calls(), ["owner:acme", "membership", "default_branch", "create", "disk", "membership", "grant_admin", "created"]);
    let port = Bridge::default();
    *port.roles.lock().unwrap() = std::collections::VecDeque::from([Some(RepositoryMembership::Member), None]);
    run(RepositoryCreation::new(&port).create_in(&actor, "acme", input("demo"))).unwrap();
    assert_eq!(port.calls(), ["owner:acme", "membership", "default_branch", "create", "disk", "membership", "created"]);
}
#[test]
fn disk_failure_logs_then_rolls_back_and_rollback_error_keeps_original_precedence() {
    for fail_rollback in [false, true] {
        let actor = user();
        let port = Bridge {
            failures: CreationFailures { disk: true, rollback: fail_rollback, ..CreationFailures::default() },
            ..Bridge::default()
        };
        let result = run(RepositoryCreation::new(&port).create_in(&actor, "", input("demo")));
        assert!(if fail_rollback {
            matches!(result, Err(RepositoryCreationError::Store("rollback")))
        } else {
            matches!(result, Err(RepositoryCreationError::Disk("disk")))
        });
        assert_eq!(port.calls(), ["default_branch", "create", "disk", "disk_failed", "rollback"]);
    }
}
#[test]
fn creator_grant_error_leaves_original_row_disk_gap_without_publication() {
    let actor = user();
    let port = Bridge { failures: CreationFailures { grant: true, ..CreationFailures::default() }, ..Bridge::default() };
    assert!(matches!(
        run(RepositoryCreation::new(&port).create_in(&actor, "acme", input("demo"))),
        Err(RepositoryCreationError::Store("grant"))
    ));
    assert!(port.row.lock().unwrap().is_some());
    assert!(*port.disk.lock().unwrap());
    assert_eq!(port.calls(), ["owner:acme", "membership", "default_branch", "create", "disk", "membership", "grant_admin"]);
}
#[test]
fn creation_preserves_multiline_description_unlike_general_settings_and_skips_empty_allocation() {
    let actor = user();
    let port = Bridge::default();
    let mut raw = input("demo");
    raw.description = " one\ntwo ".to_owned();
    raw.visibility = Visibility::Private;
    let created = run(RepositoryCreation::new(&port).create_in(&actor, "", raw)).unwrap();
    assert_eq!(created.repo.description.as_deref(), Some("one\ntwo"));
    assert_eq!(created.repo.visibility, Visibility::Private);
    let port = Bridge::default();
    let created = run(RepositoryCreation::new(&port).create_in(&actor, "", input("empty"))).unwrap();
    assert_eq!(created.repo.description, None);
}
#[test]
fn cancellation_while_disk_work_is_pending_does_not_invent_compensation_or_publication() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let actor = user();
    let port = Bridge { pause_disk: true, ..Bridge::default() };
    let app = RepositoryCreation::new(&port);
    let mut operation = Box::pin(app.create_in(&actor, "", input("demo")));
    assert!(matches!(operation.as_mut().poll(&mut Context::from_waker(Waker::noop())), Poll::Pending));
    drop(operation);
    assert!(port.row.lock().unwrap().is_some());
    assert_eq!(port.calls(), ["default_branch", "create", "disk"]);
}
