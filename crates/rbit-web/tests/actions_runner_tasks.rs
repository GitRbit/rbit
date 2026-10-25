//! The runner protocol's task calls over HTTP (`docs/design/actions.md` §12.6): the long
//! poll — claims, empty answers, wake-ups, one poll per runner, the instance's cap,
//! idempotent retries, and that an idle poll touches no SQLite — and what a claimed job's
//! specification holds. Jobs are seeded through the store with job template payloads.
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
