#[path = "support/forks.rs"]
mod support;
use rbit_repositories::forks::{ForkCreation, ForkCreationError, ForkInput};
use std::{
    future::Future,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};
use support::*;
#[test]
fn read_owner_name_and_lazy_store_precedence_is_unchanged() {
    let actor = user();
    let parent = repository(2);
    let bridge = Bridge { fail_owner: true, ..Bridge::default() };
    let port = &bridge;
    let app = ForkCreation::new(&port);
    assert!(matches!(
        run(app.fork(&actor, &parent, false, ForkInput { owner: " denied ".into(), name: "bad name".into() })),
        Err(ForkCreationError::NotFound)
    ));
    assert_eq!(bridge.trace(), Vec::<String>::new());
    assert!(matches!(
        run(app.fork(&actor, &parent, true, ForkInput { owner: " denied ".into(), name: "bad name".into() })),
        Err(ForkCreationError::Owner("owner"))
    ));
    assert_eq!(bridge.trace(), ["owner:denied"]);
    let bridge = Bridge::default();
    let port = &bridge;
    assert!(matches!(
        run(ForkCreation::new(&port).fork(&actor, &parent, true, ForkInput { owner: String::new(), name: "bad name".into() })),
        Err(ForkCreationError::Name(_))
    ));
    assert_eq!(bridge.trace(), ["owner:"]);
}
#[test]
fn defaults_settings_archive_and_only_fork_event_are_preserved() {
    let actor = user();
    let parent = repository(2);
    let bridge = Bridge { fail_pushed: true, ..Bridge::default() };
    let port = &bridge;
    assert_eq!(format!("{:?}", ForkCreation::new(&port)), "ForkCreation { .. }");
    let fork = run(ForkCreation::new(&port).fork(&actor, &parent, true, ForkInput::default())).unwrap();
    assert_eq!(
        (&fork.repo.name, &fork.repo.description, fork.repo.visibility, &fork.repo.default_branch),
        (&parent.repo.name, &parent.repo.description, parent.repo.visibility, &parent.repo.default_branch)
    );
    assert_eq!(bridge.trace(), ["owner:", "store", "create", "copy", "record", "pushed", "pushed-failed:pushed", "RepositoryForked"]);
}
#[test]
fn copy_and_relationship_failures_keep_disk_sql_compensation_and_error_identity() {
    for record in [false, true] {
        for sql in [false, true] {
            let bridge =
                Bridge { fail_copy: !record, fail_record: record, fail_disk_cleanup: true, fail_sql_cleanup: sql, ..Bridge::default() };
            let port = &bridge;
            let error = run(ForkCreation::new(&port).fork(&user(), &repository(2), true, ForkInput::default())).unwrap_err();
            if sql {
                assert!(matches!(error, ForkCreationError::Store("delete-sql")));
            } else if record {
                assert!(matches!(error, ForkCreationError::Copy("record")));
            } else {
                assert!(matches!(error, ForkCreationError::Copy("copy")));
            }
            let mut expected = vec!["owner:", "store", "create", "copy"];
            if record {
                expected.push("record");
            }
            expected.extend([
                if record { "failed:record" } else { "failed:copy" },
                "delete-disk",
                "cleanup-failed:delete-disk",
                "delete-sql",
            ]);
            assert_eq!(bridge.trace(), expected);
        }
    }
}
#[test]
fn a_unique_race_does_not_copy_or_rollback_the_winning_row() {
    let actor = user();
    let parent = repository(2);
    let bridge = Bridge::default();
    bridge.pause.store(true, Ordering::SeqCst);
    let port = &bridge;
    let app = ForkCreation::new(&port);
    let mut winner = Box::pin(app.fork(&actor, &parent, true, ForkInput::default()));
    assert!(matches!(winner.as_mut().poll(&mut Context::from_waker(Waker::noop())), Poll::Pending));
    assert!(matches!(run(app.fork(&actor, &parent, true, ForkInput::default())), Err(ForkCreationError::Store("unique"))));
    bridge.pause.store(false, Ordering::SeqCst);
    assert!(run(winner).is_ok());
    assert_eq!(bridge.trace(), ["owner:", "store", "create", "copy", "owner:", "store", "create", "record", "pushed", "RepositoryForked"]);
}
#[test]
fn unpolled_work_has_no_effect_and_dropping_during_copy_retains_the_inherited_gap() {
    let actor = user();
    let parent = repository(2);
    let bridge = Bridge::default();
    let port = &bridge;
    let app = ForkCreation::new(&port);
    drop(app.fork(&actor, &parent, true, ForkInput::default()));
    assert_eq!(bridge.trace(), Vec::<String>::new());
    bridge.pause.store(true, Ordering::SeqCst);
    let mut pending = Box::pin(app.fork(&actor, &parent, true, ForkInput::default()));
    assert!(pending.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    drop(pending);
    assert!(bridge.row.lock().unwrap().is_some(), "existing cancellation gap is documented, not silently redesigned by extraction");
    assert_eq!(bridge.trace(), ["owner:", "store", "create", "copy"]);
}
