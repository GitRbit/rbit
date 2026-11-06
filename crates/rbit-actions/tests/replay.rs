mod support;
use rbit_actions::claims::{Resumed, RunnerClaims};
use support::{Port, Task, run};
#[test]
fn missing_or_foreign_task_does_not_rotate_token() {
    for task in [None, Some(Task { owned: false, running: true })] {
        let port = Port::default();
        port.0.lock().unwrap().task = task;
        assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Unknown)));
        assert_eq!(port.calls(), ["lookup"]);
    }
}
#[test]
fn stopped_task_and_cas_loser_are_spent_without_loading_job() {
    let port = Port::default();
    port.0.lock().unwrap().task.as_mut().unwrap().running = false;
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Spent)));
    assert_eq!(port.calls(), ["lookup"]);
    let port = Port::default();
    port.0.lock().unwrap().replaced = false;
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Spent)));
    assert_eq!(port.calls(), ["lookup", "token", "replace"]);
}
#[test]
fn success_rotates_before_job_lookup_and_never_recreates_steps_or_reports_statuses() {
    let port = Port::default();
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Task(spec)) if *spec == 7));
    assert_eq!(port.calls(), ["lookup", "token", "replace", "job", "resumed", "spec", "masks"]);
}
#[test]
fn missing_job_and_invalid_spec_leave_spent_key_without_new_claim() {
    let port = Port::default();
    port.0.lock().unwrap().job_exists = false;
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Spent)));
    assert_eq!(port.calls(), ["lookup", "token", "replace", "job"]);
    let port = Port::default();
    port.0.lock().unwrap().specs = [Err("invalid".into())].into();
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Ok(Resumed::Spent)));
    assert_eq!(port.calls(), ["lookup", "token", "replace", "job", "resumed", "spec", "fail"]);
}
#[test]
fn unpolled_replay_and_storage_error_never_rotate() {
    let port = Port::default();
    drop(RunnerClaims::resume(&port, "key"));
    assert_eq!(port.calls(), Vec::<&str>::new());
    port.0.lock().unwrap().failure = Some("lookup");
    assert!(matches!(run(RunnerClaims::resume(&port, "key")), Err("lookup")));
    assert_eq!(port.calls(), ["lookup"]);
}
