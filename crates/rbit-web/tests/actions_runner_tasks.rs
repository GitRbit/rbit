//! The runner protocol's task calls over HTTP (`docs/design/actions.md` §12.6): the long
//! poll — claims, empty answers, wake-ups, one poll per runner, the instance's cap,
//! idempotent retries, and that an idle poll touches no SQLite — what a claimed job's
//! specification holds, task state calls — steps, outputs, results, cancellation and
//! ephemeral runners — and log appends by line offset with the reads pages use. Jobs are
//! seeded through the store with job template payloads.
//!
//! The server side of the runner protocol (stream S) owns this file.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use axum::http::StatusCode;
use rbit_core::{
    Timestamp,
    actions::{Run, RunEvent, Status},
};
use rbit_storage::sqlite::actions_runs::{NewJob, NewRun, RunStart};
use rbit_workflow::spec::{
    DEFAULT_TIMEOUT_MINUTES, Env, JOB_TEMPLATE_VERSION, JobSpec, JobTemplate, RunDefaults, StepAction, StepTemplate, Strategy,
};
use serde_json::{Value, json};

use rbit_services::actions::logs::LogRange;

use crate::{actions_runner::Fixture, query_count::QueryCounter};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A job `job_id` on `labels` that needs `needs`, with two steps.
pub fn template(job_id: &str, labels: &[&str], needs: &[&str]) -> JobTemplate {
    let step = |name: Option<&str>, script: &str| StepTemplate {
        id: None,
        name: name.map(str::to_owned),
        condition: None,
        action: StepAction::Run(script.to_owned()),
        shell: None,
        working_directory: None,
        env: Env::new(),
        continue_on_error: None,
        timeout_minutes: None,
    };
    JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: job_id.to_owned(),
        name: job_id.to_owned(),
        needs: needs.iter().map(|need| (*need).to_owned()).collect(),
        condition: None,
        runs_on: labels.iter().map(|label| (*label).to_owned()).collect(),
        matrix: serde_json::Map::new(),
        strategy: Strategy::SINGLE,
        permissions: None,
        concurrency: None,
        timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        continue_on_error: false,
        workflow_env: Env::new(),
        env: Env::new(),
        defaults: RunDefaults::default(),
        outputs: BTreeMap::new(),
        container: None,
        services: Vec::new(),
        steps: vec![step(Some("Build"), "make"), step(None, "\n  make test\nmake lint")],
        environment: None,
        environment_url: None,
        calls: Vec::new(),
        need_names: std::collections::BTreeMap::new(),
        deferred: false,
        default_ceiling: false,
    }
}

/// Creates a run of `alice/rocket` whose jobs have these payloads, and wakes the polls.
pub async fn seed(fixture: &Fixture, jobs: &[(JobTemplate, Vec<u8>)]) -> Run {
    seed_run(fixture, jobs, true).await
}

/// [`seed`], trusted or not.
pub async fn seed_run(fixture: &Fixture, jobs: &[(JobTemplate, Vec<u8>)], trusted: bool) -> Run {
    let repo = fixture.repo().await.repo.repo.id;
    let rows: Vec<(Vec<String>, Vec<String>)> =
        jobs.iter().map(|(template, _)| (template.needs.clone(), template.runs_on.clone())).collect();
    let new_jobs: Vec<NewJob<'_>> = jobs
        .iter()
        .zip(&rows)
        .map(|((template, payload), (needs, runs_on))| NewJob {
            key: &template.job_id,
            name: &template.name,
            matrix: None,
            needs,
            runs_on,
            permissions: None,
            payload,
            condition: false,
            max_parallel: None,
            group: None,
            environment: template.environment.as_deref(),
        })
        .collect();
    let run = fixture
        .database
        .actions_runs()
        .create_run(&NewRun {
            repo,
            workflow_path: ".rbit/workflows/ci.yml",
            workflow_name: "CI",
            title: "Build it",
            event: RunEvent::Push,
            reference: "refs/heads/main",
            sha: SHA,
            actor: Some(fixture.alice.id()),
            trigger_actor: Some(fixture.alice.id()),
            pull_number: None,
            is_fork: false,
            trusted,
            concurrency_group: None,
            cancel_in_progress: false,
            event_payload: r#"{"ref":"refs/heads/main"}"#,
            dispatch_inputs: None,
            start: RunStart::Runnable,
            jobs: &new_jobs,
            error: None,
            now: Timestamp::now(),
        })
        .await
        .unwrap();
    fixture.actions().bump_tasks_version();
    run
}

pub fn payload(template: &JobTemplate) -> (JobTemplate, Vec<u8>) {
    (template.clone(), serde_json::to_vec(template).unwrap())
}

/// `POST poll` with `free_slots: 1` and an optional idempotency key.
pub async fn poll(fixture: &Fixture, credential: &str, key: Option<&str>) -> (StatusCode, Value) {
    let mut headers = vec![("rbit-runner-protocol", "1")];
    if let Some(key) = key {
        headers.push(("idempotency-key", key));
    }
    let answer = fixture.send("poll", &headers, Some(credential), r#"{"free_slots":1}"#.into()).await;
    (answer.status, answer.json)
}

/// The job specification of a `200` poll answer.
pub fn spec(json: &Value) -> JobSpec {
    let spec: JobSpec = serde_json::from_value(json["task"].clone()).unwrap();
    spec.validate().unwrap();
    spec
}

async fn quick() -> Fixture {
    Fixture::with(|config| config.actions.long_poll_seconds = 1).await
}

#[tokio::test]
async fn a_poll_claims_a_waiting_job_and_hands_out_its_specification() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let (status, _) = poll(&fixture, &credential, Some("k0")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "nothing waits yet");

    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let started = Instant::now();
    let (status, json) = poll(&fixture, &credential, Some("k1")).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert!(started.elapsed() < Duration::from_millis(900), "a claim answers at once");
    let spec = spec(&json);
    assert_eq!((spec.template.job_id.as_str(), spec.checkout.url.as_str()), ("build", "https://git.example.com/alice/rocket"));
    assert_eq!((spec.checkout.sha.as_str(), spec.checkout.reference.as_str()), (SHA, "refs/heads/main"));
    assert_eq!((spec.cache.write.as_str(), &spec.cache.read[..]), ("heads/main", &["heads/main".to_owned()][..]));
    assert!(rbit_workflow::protocol::valid_task_token(spec.token.expose()));
    assert!(!spec.contexts.github.contains_key("token"), "the token never travels in the github context");
    assert_eq!(spec.contexts.github["repository"], "alice/rocket");
    assert!(spec.secrets.is_empty() && spec.contexts.vars.is_empty() && spec.contexts.needs.is_empty());

    let store = fixture.database.actions_runs();
    let jobs = store.jobs(run.id).await.unwrap();
    assert_eq!(jobs[0].status, Status::Running);
    let task = store.task(jobs[0].task_id.unwrap()).await.unwrap().unwrap();
    assert_eq!(task.id.0, spec.task_id.0);
    let steps = store.steps(task.id).await.unwrap();
    let names: Vec<_> = steps.iter().map(|step| (step.name.as_str(), step.status)).collect();
    assert_eq!(names, [("Build", Status::Waiting), ("Run make test", Status::Waiting)]);
    let digest = rbit_auth::TaskTokenSecret::parse(spec.token.expose()).unwrap().digest();
    assert!(store.running_task_by_token(&digest).await.unwrap().is_some(), "only the token's digest is stored");

    let (status, _) = poll(&fixture, &credential, Some("k2")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "the job is taken");
}

#[tokio::test]
async fn a_retried_poll_gets_the_same_task_with_a_fresh_token() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[])), payload(&template("test", &["linux"], &[]))]).await;
    let (_, first) = poll(&fixture, &credential, Some("retry-me")).await;
    let (_, again) = poll(&fixture, &credential, Some("retry-me")).await;
    let (first, again) = (spec(&first), spec(&again));
    assert_eq!(first.task_id, again.task_id);
    assert_ne!(first.token, again.token, "a new token is minted");
    let store = fixture.database.actions_runs();
    let old = rbit_auth::TaskTokenSecret::parse(first.token.expose()).unwrap().digest();
    assert!(store.running_task_by_token(&old).await.unwrap().is_none(), "the lost answer's token is dead");
    let (_, other) = poll(&fixture, &credential, Some("next")).await;
    assert_eq!(spec(&other).template.job_id, "test", "a new key claims the next job");

    // Once the task stopped, its key resends nothing and claims nothing.
    let done = json!({"steps": [], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &credential, first.task_id.0, &done).await.0, StatusCode::OK);
    seed(&fixture, &[payload(&template("later", &["linux"], &[]))]).await;
    assert_eq!(poll(&fixture, &credential, Some("retry-me")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(spec(&poll(&fixture, &credential, Some("fresh")).await.1).template.job_id, "later");

    let (status, json) = poll(&fixture, &credential, Some("not a key")).await;
    assert_eq!((status, json["code"].as_str()), (StatusCode::BAD_REQUEST, Some("bad_request")));
}

#[tokio::test]
async fn polls_wait_until_a_job_appears() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 20).await;
    let credential = fixture.runner(&["linux"]).await;
    let started = Instant::now();
    let (answer, ()) = tokio::join!(poll(&fixture, &credential, Some("wait")), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    });
    assert_eq!(answer.0, StatusCode::OK, "{}", answer.1);
    assert!(started.elapsed() < Duration::from_secs(5), "woken, not timed out: {:?}", started.elapsed());
}

#[tokio::test]
async fn an_idle_poll_touches_no_sqlite() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let (status, _) = poll(&fixture, &credential, Some("first")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let counter = QueryCounter::new();
    let (status, _) = counter.measure(poll(&fixture, &credential, Some("second"))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(counter.queries().is_empty(), "an idle poll ran {:?}", counter.queries());

    // Anything that may change the answer moves the version, and the next poll looks.
    fixture.actions().bump_tasks_version();
    let counter = QueryCounter::new();
    counter.measure(poll(&fixture, &credential, Some("third"))).await;
    assert!(!counter.queries().is_empty(), "the spy works");
}

#[tokio::test]
async fn one_poll_per_runner_and_at_most_max_polls() {
    let fixture = Fixture::with(|config| {
        config.actions.long_poll_seconds = 20;
        config.actions.max_polls = 1;
    })
    .await;
    let first = fixture.runner(&["linux"]).await;
    let second = fixture.runner(&["linux"]).await;
    let started = Instant::now();
    let (ended, refused) = tokio::join!(poll(&fixture, &first, Some("a")), async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let refused = poll(&fixture, &second, Some("b")).await;
        // Its own second poll ends the first; it is dropped (and its place freed) unanswered.
        let _ = tokio::time::timeout(Duration::from_millis(500), poll(&fixture, &first, Some("c"))).await;
        refused
    });
    assert_eq!(ended.0, StatusCode::NO_CONTENT, "the second poll of a runner ends its first");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!((refused.0, refused.1["code"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("unavailable")));
}

#[tokio::test]
async fn runners_get_only_jobs_they_can_run() {
    let fixture = quick().await;
    let windows = fixture.runner(&["windows"]).await;
    let linux = fixture.runner(&["linux"]).await;
    let mut broken = payload(&template("broken", &["linux"], &[]));
    broken.1 = b"jobs: {}\n".to_vec();
    let run = seed(&fixture, &[broken, payload(&template("build", &["linux"], &[]))]).await;
    assert_eq!(poll(&fixture, &windows, Some("w")).await.0, StatusCode::NO_CONTENT, "no matching labels");

    // The unreadable job fails with its key; the next poll takes the good one.
    assert_eq!(poll(&fixture, &linux, Some("l1")).await.0, StatusCode::NO_CONTENT);
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    assert_eq!((jobs[0].key.as_str(), jobs[0].status), ("broken", Status::Failure));
    let (status, json) = poll(&fixture, &linux, Some("l2")).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(spec(&json).template.job_id, "build");

    // Disabled runners take nothing.
    let repo = fixture.repo().await;
    let scope = rbit_services::actions::runners::ManagedScope::Repository(&repo);
    let id = fixture.actions().runners(&fixture.alice, scope).await.unwrap()[1].id;
    fixture.actions().set_runner_disabled(&fixture.alice, scope, id, true).await.unwrap();
    let (status, json) = poll(&fixture, &linux, Some("l3")).await;
    assert_eq!((status, json["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")));
}

#[tokio::test]
async fn needed_jobs_pass_their_results_and_outputs() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[])), payload(&template("test", &["linux"], &["build"]))]).await;
    let build = spec(&poll(&fixture, &credential, Some("b")).await.1);
    let store = fixture.database.actions_runs();
    let jobs = store.jobs(run.id).await.unwrap();
    store.add_outputs(jobs[0].id, &[("version", "1.2.3")], 1024).await.unwrap();
    let settled = store.finish_task(rbit_core::actions::TaskId(build.task_id.0), Status::Success, Timestamp::now()).await.unwrap().unwrap();
    fixture.actions().settled(&settled).await.unwrap();
    let test = spec(&poll(&fixture, &credential, Some("t")).await.1);
    let needed = &test.contexts.needs["build"];
    assert_eq!(serde_json::to_value(needed.result).unwrap(), json!("success"));
    assert_eq!(needed.outputs["version"], "1.2.3");
}

#[tokio::test]
async fn variables_reach_trusted_runs_only() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let repo = fixture.repo().await.repo.repo.id;
    let name: rbit_core::actions::VariableName = "deploy_host".parse().unwrap();
    let variables = fixture.database.actions_secrets();
    variables
        .put_variable(rbit_core::actions::SecretScope::Repository(repo), &name, "deploy.example.com", fixture.alice.id(), Timestamp::now())
        .await
        .unwrap();

    seed_run(&fixture, &[payload(&template("trusted", &["linux"], &[]))], true).await;
    let trusted = spec(&poll(&fixture, &credential, Some("a")).await.1);
    assert_eq!(trusted.contexts.vars["DEPLOY_HOST"], "deploy.example.com");

    let run = seed_run(&fixture, &[payload(&template("untrusted", &["linux"], &[]))], false).await;
    let untrusted = spec(&poll(&fixture, &credential, Some("b")).await.1);
    assert!(untrusted.contexts.vars.is_empty() && untrusted.secrets.is_empty(), "an untrusted run gets no variables or secrets");
    assert_eq!(untrusted.cache.write, format!("untrusted/{}", run.id.0));
}

// Actions: reusable workflows.
#[tokio::test]
async fn a_called_job_gets_the_secrets_its_call_passes_under_any_case() {
    use rbit_core::actions::SecretVisibility;
    use rbit_services::actions::secrets::ActionsTarget;
    use rbit_workflow::spec::{CallLevel, PassedSecrets};

    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let repo = fixture.repo().await;
    fixture
        .actions()
        .put_secret(&fixture.alice, ActionsTarget::Repository(&repo), "DEPLOY", "s3cr3t", SecretVisibility::All)
        .await
        .unwrap();
    let mut called = template("call/build", &["linux"], &[]);
    called.calls = vec![CallLevel {
        name: "call".into(),
        condition: None,
        matrix: serde_json::Map::new(),
        strategy: Strategy::SINGLE,
        needs: BTreeMap::new(),
        inputs: serde_json::Map::new(),
        deferred: BTreeMap::new(),
        // `on.workflow_call.secrets` names may be lowercase; secrets.key reads them as on GitHub.
        secrets: PassedSecrets::Map(BTreeMap::from([("key".into(), "${{ secrets.deploy }}".into())])),
    }];
    seed(&fixture, &[payload(&called)]).await;
    let task = spec(&poll(&fixture, &credential, Some("c")).await.1);
    assert_eq!(task.secrets.keys().collect::<Vec<_>>(), ["KEY"]);
    assert_eq!(task.secrets["KEY"].expose(), "s3cr3t");
}

/// `POST tasks/{task}/state` with `body`.
pub async fn state(fixture: &Fixture, credential: &str, task: impl std::fmt::Display, body: &Value) -> (StatusCode, Value) {
    let answer = fixture.call(&format!("tasks/{task}/state"), Some(credential), body.to_string()).await;
    (answer.status, answer.json)
}

fn step(index: u32, status: &str) -> Value {
    json!({"index": index, "status": status, "started_at": 100, "stopped_at": null, "log_index": 0, "log_length": 0})
}

#[tokio::test]
async fn state_calls_record_steps_outputs_and_the_result() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[])), payload(&template("test", &["linux"], &["build"]))]).await;
    let build = spec(&poll(&fixture, &credential, Some("b")).await.1);
    let task = build.task_id.0;
    let token = build.token.expose().to_owned();

    let body = json!({"steps": [step(0, "running")], "outputs": {"version": "1.2.3", "leak": format!("token={token}")}, "result": null});
    let (status, json) = state(&fixture, &credential, task, &body).await;
    assert_eq!((status, &json), (StatusCode::OK, &json!({"cancel": false})));
    let store = fixture.database.actions_runs();
    let task_id = rbit_core::actions::TaskId(task);
    let steps = store.steps(task_id).await.unwrap();
    assert_eq!((steps[0].status, steps[0].name.as_str(), steps[1].status), (Status::Running, "Build", Status::Waiting));
    let jobs = store.jobs(run.id).await.unwrap();
    let outputs = store.outputs(jobs[0].id).await.unwrap();
    assert_eq!(outputs, [("leak".to_owned(), "token=***".to_owned()), ("version".to_owned(), "1.2.3".to_owned())], "masked");

    let resent = json!({"steps": [], "outputs": {"version": "9.9.9"}, "result": null});
    assert_eq!(state(&fixture, &credential, task, &resent).await.0, StatusCode::OK);
    assert_eq!(store.outputs(jobs[0].id).await.unwrap()[1].1, "1.2.3", "outputs are kept when first seen");

    let mut done = json!({"steps": [step(0, "success"), step(1, "success")], "outputs": {}, "result": "success"});
    done["steps"][1]["log_index"] = json!(3);
    let (status, json) = state(&fixture, &credential, task, &done).await;
    assert_eq!((status, &json), (StatusCode::OK, &json!({"cancel": false})));
    let stored = store.task(task_id).await.unwrap().unwrap();
    assert_eq!(stored.status, Status::Success);
    let digest = rbit_auth::TaskTokenSecret::parse(&token).unwrap().digest();
    assert!(store.running_task_by_token(&digest).await.unwrap().is_none(), "the token died with the task");
    let jobs = store.jobs(run.id).await.unwrap();
    assert_eq!((jobs[0].status, jobs[1].status), (Status::Success, Status::Waiting), "the dependent unblocked");
    assert_eq!(store.steps(task_id).await.unwrap()[1].log_index, 3);

    // A late call about a stopped task changes nothing.
    let late = json!({"steps": [step(0, "failure")], "outputs": {"late": "x"}, "result": "failure"});
    assert_eq!(state(&fixture, &credential, task, &late).await, (StatusCode::OK, json!({"cancel": false})));
    assert_eq!(store.task(task_id).await.unwrap().unwrap().status, Status::Success);
    assert_eq!(store.outputs(jobs[0].id).await.unwrap().len(), 2);

    let test = spec(&poll(&fixture, &credential, Some("t")).await.1);
    assert_eq!(test.contexts.needs["build"].outputs["version"], "1.2.3");
}

#[tokio::test]
async fn state_calls_are_for_the_runners_own_tasks() {
    let fixture = quick().await;
    let owner = fixture.runner(&["linux"]).await;
    let other = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &owner, Some("o")).await.1).task_id.0;
    let heartbeat = json!({"steps": [], "outputs": {}, "result": null});
    let (status, json) = state(&fixture, &other, task, &heartbeat).await;
    assert_eq!((status, json["code"].as_str()), (StatusCode::FORBIDDEN, Some("forbidden")));
    for unknown in ["9999", "0", "abc"] {
        assert_eq!(state(&fixture, &owner, unknown, &heartbeat).await.0, StatusCode::NOT_FOUND, "{unknown}");
    }
    let running_at_the_end = json!({"steps": [step(0, "running")], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &owner, task, &running_at_the_end).await.0, StatusCode::BAD_REQUEST);
    let bad_output = json!({"steps": [], "outputs": {"1st": "x"}, "result": null});
    assert_eq!(state(&fixture, &owner, task, &bad_output).await.0, StatusCode::BAD_REQUEST);
    let unauthenticated = fixture.call(&format!("tasks/{task}/state"), None, heartbeat.to_string()).await;
    assert_eq!(unauthenticated.status, StatusCode::UNAUTHORIZED);
    let body = |bytes: usize| format!("{{\"steps\": [], \"outputs\": {{\"a\": \"{}\"}}, \"result\": null}}", "x".repeat(bytes));
    let over_outputs = fixture.call(&format!("tasks/{task}/state"), Some(&owner), body(1_100_000)).await;
    assert_eq!(over_outputs.status, StatusCode::BAD_REQUEST, "within the call's cap, over the outputs' limit");
    let huge = fixture.call(&format!("tasks/{task}/state"), Some(&owner), body(4_300_000)).await;
    assert_eq!(huge.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn cancellation_reaches_the_runner_in_the_answer() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let spec = spec(&poll(&fixture, &credential, Some("c")).await.1);
    let repo = fixture.repo().await;
    fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();
    let digest = rbit_auth::TaskTokenSecret::parse(spec.token.expose()).unwrap().digest();
    let store = fixture.database.actions_runs();
    assert!(store.running_task_by_token(&digest).await.unwrap().is_none(), "the token dies with the cancel, not with the answer");
    let heartbeat = json!({"steps": [], "outputs": {}, "result": null});
    assert_eq!(state(&fixture, &credential, spec.task_id.0, &heartbeat).await, (StatusCode::OK, json!({"cancel": true})));
    let stopped = json!({"steps": [step(0, "cancelled"), step(1, "skipped")], "outputs": {}, "result": "cancelled"});
    assert_eq!(state(&fixture, &credential, spec.task_id.0, &stopped).await, (StatusCode::OK, json!({"cancel": true})));
    let job = &store.jobs(run.id).await.unwrap()[0];
    assert_eq!(job.status, Status::Cancelled);
    assert_eq!(store.task(job.task_id.unwrap()).await.unwrap().unwrap().status, Status::Cancelled);
}

#[tokio::test]
async fn a_task_left_behind_by_a_rerun_is_cancelled_and_changes_nothing() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let old = spec(&poll(&fixture, &credential, Some("old")).await.1);
    let repo = fixture.repo().await;
    fixture.actions().cancel(&fixture.alice, &repo, run.id).await.unwrap();
    fixture.actions().rerun(&fixture.alice, &repo, run.id, rbit_storage::sqlite::actions_runs::Rerun::All).await.unwrap();
    let new = spec(&poll(&fixture, &credential, Some("new")).await.1);

    let late = json!({"steps": [step(0, "success"), step(1, "success")], "outputs": {"version": "old"}, "result": "success"});
    assert_eq!(state(&fixture, &credential, old.task_id.0, &late).await, (StatusCode::OK, json!({"cancel": true})));
    let store = fixture.database.actions_runs();
    let job = &store.jobs(run.id).await.unwrap()[0];
    assert_eq!((job.status, job.task_id.map(|task| task.0)), (Status::Running, Some(new.task_id.0)));
    assert!(store.outputs(job.id).await.unwrap().is_empty(), "the old attempt's outputs are dropped");
    let heartbeat = json!({"steps": [], "outputs": {}, "result": null});
    assert_eq!(state(&fixture, &credential, new.task_id.0, &heartbeat).await, (StatusCode::OK, json!({"cancel": false})));
}

#[tokio::test]
async fn a_failed_job_that_may_fail_counts_as_succeeded() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut lenient = template("build", &["linux"], &[]);
    lenient.continue_on_error = true;
    let run = seed(&fixture, &[payload(&lenient), payload(&template("test", &["linux"], &["build"]))]).await;
    let build = spec(&poll(&fixture, &credential, Some("b")).await.1);
    let failed = json!({"steps": [step(0, "failure"), step(1, "skipped")], "outputs": {}, "result": "failure"});
    assert_eq!(state(&fixture, &credential, build.task_id.0, &failed).await, (StatusCode::OK, json!({"cancel": false})));
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    assert_eq!((jobs[0].status, jobs[1].status), (Status::Success, Status::Waiting), "the job that needs it runs");
    let test = spec(&poll(&fixture, &credential, Some("t")).await.1);
    assert_eq!(serde_json::to_value(test.contexts.needs["build"].result).unwrap(), json!("success"));
}

#[tokio::test]
async fn ephemeral_runners_go_away_after_their_task() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut declaration = crate::actions_runner::declare_body(&["linux"]);
    declaration["capabilities"]["ephemeral"] = json!(true);
    assert_eq!(fixture.call("declare", Some(&credential), declaration.to_string()).await.status, StatusCode::OK);
    seed(&fixture, &[payload(&template("build", &["linux"], &[])), payload(&template("more", &["linux"], &[]))]).await;
    let spec = spec(&poll(&fixture, &credential, Some("e1")).await.1);
    assert_eq!(poll(&fixture, &credential, Some("e2")).await.0, StatusCode::NO_CONTENT, "one task at a time");
    let done = json!({"steps": [step(0, "success"), step(1, "success")], "outputs": {}, "result": "success"});
    assert_eq!(state(&fixture, &credential, spec.task_id.0, &done).await.0, StatusCode::OK);
    let gone = fixture.call("declare", Some(&credential), declaration.to_string()).await;
    assert_eq!(gone.status, StatusCode::UNAUTHORIZED, "the runner was deleted with its task");
}

/// `POST tasks/{task}/logs?from={from}` with `body`.
pub async fn logs(fixture: &Fixture, credential: &str, task: i64, from: u64, body: &str) -> (StatusCode, Value) {
    let answer = fixture.call(&format!("tasks/{task}/logs?from={from}"), Some(credential), body.to_owned()).await;
    (answer.status, answer.json)
}

#[tokio::test]
async fn logs_are_appended_by_line_offset_masked_finalised_and_read() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let spec = spec(&poll(&fixture, &credential, Some("l")).await.1);
    let (task, token) = (spec.task_id.0, spec.token.expose().to_owned());

    assert_eq!(logs(&fixture, &credential, task, 0, "one\ntwo\n").await, (StatusCode::OK, json!({"next_line": 2})));
    // A resend overlaps harmlessly; only the new line is kept.
    assert_eq!(logs(&fixture, &credential, task, 1, &format!("two\nthree {token}\r\n")).await, (StatusCode::OK, json!({"next_line": 3})));
    assert_eq!(logs(&fixture, &credential, task, 0, "one\n").await, (StatusCode::OK, json!({"next_line": 3})));
    let (status, json) = logs(&fixture, &credential, task, 5, "five\n").await;
    assert_eq!((status, json["code"].as_str(), &json["next_line"]), (StatusCode::CONFLICT, Some("conflict"), &json!(3)));
    assert_eq!(logs(&fixture, &credential, task, 3, "").await, (StatusCode::OK, json!({"next_line": 3})));
    let other = fixture.runner(&["windows"]).await;
    assert_eq!(logs(&fixture, &other, task, 3, "x\n").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        fixture.call(&format!("tasks/{task}/logs"), Some(&credential), "x").await.status,
        StatusCode::BAD_REQUEST,
        "from is required"
    );

    let mut gzipped = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut gzipped, b"four\n").unwrap();
    let answer = fixture
        .send(
            &format!("tasks/{task}/logs?from=3"),
            &[("rbit-runner-protocol", "1"), ("content-encoding", "gzip")],
            Some(&credential),
            gzipped.finish().unwrap().into(),
        )
        .await;
    assert_eq!(answer.json, json!({"next_line": 4}));

    // Reads while it runs: the whole log, and a step's lines as the runner described them.
    let repo = fixture.repo().await;
    let actions = fixture.actions();
    let task_id = rbit_core::actions::TaskId(task);
    let live = actions.task_log(&repo, task_id, LogRange::From(0), 100, 1 << 20).await.unwrap();
    assert_eq!(live.lines, ["one", "two", "three ***", "four"], "the token is masked, the CR dropped");
    assert!(live.live && !live.truncated);
    let mut first = step(0, "success");
    first["log_length"] = json!(2);
    let mut second = step(1, "running");
    second["log_index"] = json!(2);
    state(&fixture, &credential, task, &json!({"steps": [first, second], "outputs": {}, "result": null})).await;
    let running = actions.step_log(&repo, task_id, 1, LogRange::From(0), 100, 1 << 20).await.unwrap();
    assert_eq!((running.lines.clone(), running.first_line, running.next_line), (vec!["three ***".to_owned(), "four".to_owned()], 2, 4));
    let tail = actions.step_log(&repo, task_id, 0, LogRange::Tail, 1, 1 << 20).await.unwrap();
    assert_eq!((tail.lines.clone(), tail.more_before), (vec!["two".to_owned()], true));

    let mut second = step(1, "success");
    second["log_index"] = json!(2);
    second["log_length"] = json!(2);
    let mut first = step(0, "success");
    first["log_length"] = json!(2);
    state(&fixture, &credential, task, &json!({"steps": [first, second], "outputs": {}, "result": "success"})).await;
    let (status, json) = logs(&fixture, &credential, task, 4, "late\n").await;
    assert_eq!((status, json["code"].as_str()), (StatusCode::CONFLICT, Some("conflict")), "the log is closed");
    let stored = fixture.database.actions_runs().task(task_id).await.unwrap().unwrap();
    assert_eq!((stored.log_state, stored.log_lines), (rbit_core::actions::LogState::Final, 4));
    let finished = actions.step_log(&repo, task_id, 1, LogRange::From(0), 100, 1 << 20).await.unwrap();
    assert_eq!((finished.lines, finished.live), (vec!["three ***".to_owned(), "four".to_owned()], false));
    let raw = actions.raw_log(&repo, task_id).await.unwrap().unwrap();
    assert!(raw.gzip);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(raw.file.into_std().await), &mut text).unwrap();
    assert_eq!(text, "one\ntwo\nthree ***\nfour\n");
}

#[tokio::test]
async fn logs_are_read_only_by_readers_of_their_repository() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = rbit_core::actions::TaskId(spec(&poll(&fixture, &credential, Some("r")).await.1).task_id.0);
    let bob = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(true),
        )
        .await
        .unwrap();
    let input =
        rbit_services::CreateRepository { name: "other".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
    fixture.services.repositories.create(&bob, input).await.unwrap();
    let other = fixture.services.repositories.open(Some(&bob), "bob", "other").await.unwrap();
    let error = fixture.actions().task_log(&other, task, LogRange::From(0), 10, 1024).await.unwrap_err();
    assert!(matches!(error, rbit_services::ServiceError::NotFound), "another repository's task: {error}");
    assert!(fixture.actions().raw_log(&fixture.repo().await, task).await.unwrap().is_none(), "nothing written yet");
}

#[tokio::test]
async fn a_job_that_cannot_run_says_why_in_its_log() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    let mut broken = payload(&template("broken", &["linux"], &[]));
    broken.1 = br#"{"version": 99}"#.to_vec();
    let run = seed(&fixture, &[broken]).await;
    assert_eq!(poll(&fixture, &credential, Some("x")).await.0, StatusCode::NO_CONTENT);
    let job = &fixture.database.actions_runs().jobs(run.id).await.unwrap()[0];
    let window = fixture.actions().task_log(&fixture.repo().await, job.task_id.unwrap(), LogRange::From(0), 10, 4096).await.unwrap();
    assert_eq!(window.lines.len(), 1);
    assert!(window.lines[0].starts_with("rbit: this job's stored template cannot be read"), "{:?}", window.lines);
}

// Actions: third-party actions.
#[tokio::test]
async fn post_steps_the_runner_adds_are_created_once_and_never_rename_the_template() {
    let fixture = quick().await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = rbit_core::actions::TaskId(spec(&poll(&fixture, &credential, Some("p")).await.1).task_id.0);
    let store = fixture.database.actions_runs();
    let template_steps = store.steps_of_tasks(&[task]).await.unwrap();
    let added = u32::try_from(template_steps.len()).unwrap();
    let body = json!({
        "steps": [step(0, "success"), step(added, "running")],
        "outputs": {},
        "result": null,
        "post_steps": {"0": "Post Run evil/rename@v1", added.to_string(): "Post Run actions/setup-node@v4"},
    });
    assert_eq!(state(&fixture, &credential, task.0, &body).await.0, StatusCode::OK);
    assert_eq!(state(&fixture, &credential, task.0, &body).await.0, StatusCode::OK, "resent: nothing changes");
    let steps = store.steps_of_tasks(&[task]).await.unwrap();
    assert_eq!(steps.len(), template_steps.len() + 1);
    assert_eq!(steps[0].name, template_steps[0].name, "a template step keeps its name");
    let post = steps.iter().find(|step| step.number == i64::from(added)).unwrap();
    assert_eq!((post.name.as_str(), post.status), ("Post Run actions/setup-node@v4", rbit_core::actions::Status::Running));

    let bad = json!({"steps": [], "outputs": {}, "result": null, "post_steps": {"1": "a\nb"}});
    assert_eq!(state(&fixture, &credential, task.0, &bad).await.0, StatusCode::BAD_REQUEST);
}
