mod support;
use rbit_actions::claims::{Claim, RunnerClaims};
use std::cell::Cell;
use support::{Port, run};

#[test]
fn empty_labels_and_unpolled_command_never_construct_store_or_token() {
    let port = Port::default();
    let constructions = Cell::new(0);
    let open = || {
        constructions.set(constructions.get() + 1);
        port.clone()
    };
    drop(RunnerClaims::claim(true, open, None));
    assert!(matches!(run(RunnerClaims::claim(false, open, None)), Ok(Claim::Nothing)));
    assert_eq!(constructions.get(), 0);
    assert_eq!(port.calls(), Vec::<&str>::new());
}
#[test]
fn success_preserves_key_masks_steps_and_status_publication_order() {
    let port = Port::default();
    assert!(matches!(run(RunnerClaims::claim(true, || port.clone(), Some("key"))), Ok(Claim::Task(spec)) if *spec == 7));
    assert_eq!(port.calls(), ["token", "claim", "key", "claimed", "spec", "masks", "steps", "statuses"]);
}
#[test]
fn invalid_spec_spends_key_without_claiming_another_job() {
    let port = Port::default();
    port.0.lock().unwrap().specs = [Err("invalid template".into())].into();
    assert!(matches!(run(RunnerClaims::claim(true, || port.clone(), Some("key"))), Ok(Claim::Spent)));
    assert_eq!(port.calls(), ["token", "claim", "key", "claimed", "spec", "fail"]);
}
#[test]
fn unkeyed_invalid_spec_retries_with_one_store_and_new_token() {
    let port = Port::default();
    {
        let mut state = port.0.lock().unwrap();
        state.jobs = [1, 2].into();
        state.specs = [Err("bad".into()), Ok(9)].into();
    }
    let constructions = Cell::new(0);
    assert!(
        matches!(run(RunnerClaims::claim(true, || { constructions.set(constructions.get()+1); port.clone() }, None)), Ok(Claim::Task(spec)) if *spec == 9)
    );
    assert_eq!(constructions.get(), 1);
    assert_eq!(
        port.calls(),
        ["token", "claim", "claimed", "spec", "fail", "token", "claim", "claimed", "spec", "masks", "steps", "statuses"]
    );
}
#[test]
fn missing_job_still_mints_once_and_infrastructure_errors_do_not_retry() {
    let port = Port::default();
    port.0.lock().unwrap().jobs.clear();
    assert!(matches!(run(RunnerClaims::claim(true, || port.clone(), None)), Ok(Claim::Nothing)));
    assert_eq!(port.calls(), ["token", "claim"]);
    for (failure, expected) in [
        ("claim", vec!["token", "claim"]),
        ("spec", vec!["token", "claim", "key", "claimed", "spec"]),
        ("steps", vec!["token", "claim", "key", "claimed", "spec", "masks", "steps"]),
        ("statuses", vec!["token", "claim", "key", "claimed", "spec", "masks", "steps", "statuses"]),
    ] {
        let port = Port::default();
        port.0.lock().unwrap().failure = Some(failure);
        assert!(matches!(run(RunnerClaims::claim(true, || port.clone(), Some("key"))), Err(error) if error == failure));
        assert_eq!(port.calls(), expected);
    }
}
