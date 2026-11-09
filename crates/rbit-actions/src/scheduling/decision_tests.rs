//! Job decisions after a settle, request rules and the lease answers of state reports and
//! log appends.
use std::collections::BTreeMap;

use rbit_workflow::{
    Deadline, Limits, RunContext,
    spec::{JobResult, JobTemplate, Needed},
};
use serde_json::{Map, Value, json};

use super::jobs::{
    FailFast, JobState, Verdict, calls_verdict, condition_contexts, condition_verdict, deferred_verdict, fail_fast_applies,
    fail_fast_candidates,
};
use super::leases::{Refusal, ended_task_cancels, log_open, owned};
use super::requests::{ReviewSelection, check_approval, check_rerun, check_review_comment, review_selection};

/// The planned jobs of a workflow with a three-way matrix and, when `fail_fast` is off,
/// `fail-fast: false`.
fn templates(fail_fast: bool) -> Vec<JobTemplate> {
    let source = format!(
        "on: push\njobs:\n  build:\n    runs-on: x\n    strategy:\n      fail-fast: {fail_fast}\n      matrix:\n        os: [a, b, c]\n    steps: [{{run: make}}]\n  lint:\n    runs-on: x\n    steps: [{{run: lint}}]\n"
    );
    let workflow = rbit_workflow::parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).unwrap();
    rbit_workflow::plan(&workflow, &RunContext::default(), &Limits::default(), &Deadline::NONE).unwrap()
}

fn needs(results: &[(&str, JobResult)]) -> BTreeMap<String, Needed> {
    results
        .iter()
        .map(|(key, result)| ((*key).to_owned(), Needed { result: *result, outputs: BTreeMap::from([("v".to_owned(), "1".to_owned())]) }))
        .collect()
}

#[test]
fn conditions_run_skip_or_fail_with_the_server_contexts() {
    let template = &templates(true)[0];
    let github: Map<String, Value> = json!({"ref": "refs/heads/main", "token": "secret", "Token": "x"}).as_object().cloned().unwrap();
    let vars: Map<String, Value> = json!({"STAGE": "prod"}).as_object().cloned().unwrap();
    let passed = condition_contexts(github.clone(), Map::new(), vars.clone(), template, needs(&[("test", JobResult::Success)]));
    assert_eq!(condition_verdict("github.ref == 'refs/heads/main' && vars.STAGE == 'prod'", &passed), (Verdict::Run, None));
    assert_eq!(condition_verdict("needs.test.result == 'success' && needs.test.outputs.v == '1'", &passed), (Verdict::Run, None));
    assert_eq!(condition_verdict("github.token == 'secret'", &passed), (Verdict::Skip, None), "the token is never readable");
    let failed = condition_contexts(github, Map::new(), vars, template, needs(&[("test", JobResult::Failure)]));
    assert_eq!(
        condition_verdict("github.ref == 'refs/heads/main'", &failed),
        (Verdict::Skip, None),
        "an implied success() after a failure"
    );
    assert_eq!(condition_verdict("failure() && matrix.os == 'a'", &failed), (Verdict::Run, None));
    let contexts = failed;
    let (verdict, error) = condition_verdict("fromJSON(", &contexts);
    assert_eq!(verdict, Verdict::Fail);
    assert!(error.is_some());
}

#[test]
fn calls_and_deferred_plans_decide_only_when_they_must() {
    assert_eq!(calls_verdict::<()>(&Ok(true)), None);
    assert_eq!(calls_verdict::<()>(&Ok(false)), Some(Verdict::Skip));
    assert_eq!(calls_verdict(&Err("bad")), Some(Verdict::Fail));
    assert_eq!(deferred_verdict::<u8, ()>(&Ok(vec![1])), None);
    assert_eq!(deferred_verdict::<u8, ()>(&Ok(vec![])), Some(Verdict::Skip), "an empty matrix");
    assert_eq!(deferred_verdict::<u8, &str>(&Err("cannot plan")), Some(Verdict::Fail));
}

#[test]
fn a_failed_matrix_job_cancels_its_unfinished_siblings() {
    let state = |id, key, failed, done| JobState { id, key, failed, done };
    let jobs = [
        state(1, "build", true, true),
        state(2, "build", false, false),
        state(3, "build", false, true),
        state(4, "lint", false, false),
        state(5, "deploy", true, true),
        state(6, "build", false, false),
    ];
    assert_eq!(fail_fast_candidates(&jobs), [FailFast { key: "build", first: 1, unfinished: vec![2, 6] }], "deploy has nothing left");
    let on = templates(true);
    let off = templates(false);
    assert!(fail_fast_applies(&on[0]), "a matrix with fail-fast");
    assert!(!fail_fast_applies(&off[0]), "fail-fast: false");
    assert!(!fail_fast_applies(on.iter().find(|job| job.job_id == "lint").unwrap()), "no matrix");
}

#[test]
fn requests_follow_the_run_and_review_rules() {
    assert_eq!(check_rerun(true, true), Err("a workflow error has no jobs to run again"));
    assert_eq!(check_rerun(false, false), Err("only a finished run can be run again"));
    assert_eq!(check_rerun(false, true), Ok(()));
    assert_eq!(check_approval(false), Err("the run does not wait for approval"));
    assert_eq!(check_approval(true), Ok(()));
    assert_eq!(check_review_comment(&"é".repeat(65_536)), Ok(()));
    assert_eq!(check_review_comment(&"é".repeat(65_537)), Err("the comment is too long"));

    let pending = [("Production", vec![1, 2]), ("staging", vec![3])];
    let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
    let all = |_: &str| Ok::<(), &str>(());
    assert_eq!(review_selection(&names(&["production", "STAGING"]), &pending, all), Ok(vec![1, 2, 3]));
    assert_eq!(
        review_selection(&names(&["qa"]), &pending, all),
        Err(ReviewSelection::Invalid("no job of this run waits for qa".to_owned()))
    );
    assert_eq!(review_selection(&names(&[]), &pending, all), Err(ReviewSelection::Invalid("choose an environment to review".to_owned())));
    let not_staging = |name: &str| if name == "staging" { Err("forbidden") } else { Ok(()) };
    assert_eq!(review_selection(&names(&["production", "staging"]), &pending, not_staging), Err(ReviewSelection::Refused("forbidden")));
}

#[test]
fn state_reports_and_log_appends_answer_a_lost_lease() {
    assert_eq!(owned(Some(7), 7), Ok(()));
    assert_eq!(owned(Some(8), 7), Err(Refusal::OtherRunner));
    assert_eq!(owned(None, 7), Err(Refusal::OtherRunner));
    assert!(ended_task_cancels(false, false, false), "the server stopped the task under its runner");
    assert!(!ended_task_cancels(true, false, false), "a resent final report");
    assert!(ended_task_cancels(true, true, false) && ended_task_cancels(true, false, true));
    assert_eq!(log_open(false, true), Ok(()));
    assert_eq!(log_open(true, true), Err(Refusal::LogClosed));
    assert_eq!(log_open(false, false), Err(Refusal::LogClosed));
    assert_eq!(Refusal::LogClosed.message(), "the task has ended; its log is closed");
}
