//! Independent synchronous scripted ports pin authority, ordering and original partial effects.
#[path = "support/settings.rs"]
mod fixture;
use fixture::{Bridge, repo, run, user};
use rbit_contracts::Timestamp;
use rbit_repositories::{
    Visibility,
    settings::{RepositorySettings, RepositorySettingsError, UpdateRepository},
};
fn general(visibility: Visibility, branch: Option<&str>) -> UpdateRepository {
    UpdateRepository { description: " tidy ".to_owned(), visibility, default_branch: branch.map(str::to_owned) }
}
#[test]
fn rejected_noop_and_unpolled_operations_never_construct_store_or_read_git() {
    let repo = repo();
    let port = Bridge::default();
    let binding = &port;
    let denied = RepositorySettings::new(&repo, false);
    assert!(matches!(run(denied.update(&binding, general(Visibility::Public, Some("other")))), Err(RepositorySettingsError::Forbidden)));
    let app = RepositorySettings::new(&repo, true);
    let invalid = UpdateRepository { description: "x".repeat(351), ..general(Visibility::Public, None) };
    assert!(matches!(run(app.update(&binding, invalid)), Err(RepositorySettingsError::Invalid(_))));
    assert!(matches!(
        run(app.update(&binding, UpdateRepository { description: "a\nb".to_owned(), ..general(Visibility::Public, None) })),
        Err(RepositorySettingsError::Invalid(_))
    ));
    assert_eq!(run(app.rename(&binding, " demo ")).unwrap().as_str(), "demo");
    drop(app.update(&binding, general(Visibility::Public, Some("other"))));
    assert_eq!(port.calls(), Vec::<String>::new());
}
#[test]
fn unchanged_head_and_narrowed_visibility_skip_ancestor_and_git_but_still_write_settings() {
    let repo = repo();
    let port = Bridge::default();
    let binding = &port;
    let app = RepositorySettings::new(&repo, true);
    run(app.update(&binding, general(Visibility::Private, Some(" main ")))).unwrap();
    assert_eq!(port.calls(), ["store", "update:main", "updated"]);
    assert_eq!(port.settings.lock().unwrap().as_ref().unwrap().description.as_deref(), Some("tidy"));
}
#[test]
fn private_ancestor_rejection_precedes_branch_lookup_and_sql() {
    let mut repo = repo();
    repo.repo.visibility = Visibility::Private;
    let port = Bridge { private_ancestor: true, ..Bridge::default() };
    let binding = &port;
    assert!(matches!(
        run(RepositorySettings::new(&repo, true).update(&binding, general(Visibility::Public, Some("other")))),
        Err(RepositorySettingsError::Invalid(_))
    ));
    assert_eq!(port.calls(), ["ancestor"]);
}
#[test]
fn sql_failure_restores_head_and_restore_failure_does_not_replace_original_error() {
    for restore_fails in [false, true] {
        let repo = repo();
        let port = Bridge { fail_update: true, restore_fails, ..Bridge::default() };
        let binding = &port;
        assert!(matches!(
            run(RepositorySettings::new(&repo, true).update(&binding, general(Visibility::Public, Some("other")))),
            Err(RepositorySettingsError::Store("update"))
        ));
        let mut calls = vec!["branch:other", "head:other", "store", "update:other", "head:main"];
        if restore_fails {
            calls.push("restore_failed");
        }
        assert_eq!(port.calls(), calls);
    }
}
#[test]
fn archive_constructor_precedes_clock_only_sql_winners_publish_and_unarchive_does_not_sample_clock() {
    let repo = repo();
    let actor = user();
    let port = Bridge::default();
    let binding = &port;
    let app = RepositorySettings::new(&repo, true);
    assert!(
        run(app.set_archived(&actor, &binding, true, || {
            port.log("clock");
            Timestamp(8)
        }))
        .unwrap()
    );
    assert!(
        !run(app.set_archived(&actor, &binding, true, || {
            port.log("clock");
            Timestamp(9)
        }))
        .unwrap()
    );
    assert!(run(app.set_archived(&actor, &binding, false, || panic!("unarchive has no clock"))).unwrap());
    assert_eq!(
        port.calls(),
        ["store", "clock", "archive", "archive_effect:true", "store", "clock", "archive", "store", "unarchive", "archive_effect:false"]
    );
}
#[test]
fn delete_confirmation_precedes_reader_close_and_disk_failure_prevents_store_acquisition() {
    let repo = repo();
    let actor = user();
    let port = Bridge { fail_disk: true, ..Bridge::default() };
    let binding = &port;
    let app = RepositorySettings::new(&repo, true);
    assert!(matches!(run(app.delete(&actor, &binding, "demo")), Err(RepositorySettingsError::Invalid(_))));
    assert_eq!(port.calls(), Vec::<String>::new());
    assert!(matches!(run(app.delete(&actor, &binding, " alice/demo ")), Err(RepositorySettingsError::Effect("disk"))));
    assert_eq!(port.calls(), ["close", "disk"]);
    let port = Bridge::default();
    let binding = &port;
    run(app.delete(&actor, &binding, "alice/demo")).unwrap();
    assert_eq!(port.calls(), ["close", "disk", "store", "delete", "deleted:alice/demo"]);
}
#[test]
fn cancelled_audit_future_retains_committed_archive_without_claiming_event_durability() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let repo = repo();
    let actor = user();
    let port = Bridge { pause_archive_effect: true, ..Bridge::default() };
    let binding = &port;
    let app = RepositorySettings::new(&repo, true);
    let mut operation = Box::pin(app.set_archived(&actor, &binding, true, || Timestamp(1)));
    assert!(matches!(operation.as_mut().poll(&mut Context::from_waker(Waker::noop())), Poll::Pending));
    drop(operation);
    assert!(*port.archived.lock().unwrap());
    assert_eq!(port.calls(), ["store", "archive", "archive_effect:true"]);
    assert_eq!(port.publications.lock().unwrap().len(), 0);
}
