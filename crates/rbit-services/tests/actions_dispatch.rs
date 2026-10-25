//! Actions workstream T: `workflow_dispatch` (who may, which refs, inputs) and scheduled
//! runs (the table, its refresh on default-branch pushes and a ticker driven by a fake
//! clock), against real stores and repositories.

use std::collections::BTreeMap;

use rbit_core::{
    PushSource, Timestamp,
    actions::{RunEvent, Status},
};
use rbit_services::{ServiceError, actions::dispatch::DispatchRequest};
use serde_json::{Value, json};

use crate::actions_fixture::{Fixture, workflow};

const MANUAL: &str = "name: Manual
on:
  workflow_dispatch:
    inputs:
      level:
        type: choice
        options: [low, high]
        required: true
      debug:
        type: boolean
      count:
        type: number
        default: 3
jobs:
  a:
    runs-on: x
    steps: [{run: x}]
";

fn request(workflow: &str, reference: &str, inputs: &[(&str, &str)]) -> DispatchRequest {
    DispatchRequest {
        workflow: workflow.into(),
        reference: reference.into(),
        inputs: inputs.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect::<BTreeMap<_, _>>(),
    }
}

fn invalid(result: Result<rbit_core::actions::Run, ServiceError>) -> String {
    match result {
        Err(ServiceError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn writers_dispatch_with_checked_inputs() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(".forgejo/workflows/manual.yml", MANUAL), (".forgejo/workflows/push.yml", &workflow("Push", "push"))], "CI");
    fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.carol).await;
    let actions = fixture.actions();

    let run =
        actions.dispatch(&fixture.carol, &repo, &request("manual.yml", "main", &[("level", "high"), ("debug", "true")])).await.unwrap();
    assert_eq!((run.event, run.reference.as_str(), run.workflow_name.as_str()), (RunEvent::WorkflowDispatch, "refs/heads/main", "Manual"));
    assert_eq!(
        (run.actor, run.trusted, run.status, run.title.as_str()),
        (Some(fixture.carol.id()), true, Status::Waiting, "Manual ran manually")
    );
    let (event, inputs) = fixture.database.actions_runs().event(run.id).await.unwrap().unwrap();
    let event: Value = serde_json::from_str(&event).unwrap();
    assert_eq!(event["inputs"], json!({ "level": "high", "debug": "true", "count": "3" }));
    assert_eq!((event["ref"].as_str(), event["workflow"].as_str()), (Some("refs/heads/main"), Some(".forgejo/workflows/manual.yml")));
    assert_eq!(event["sender"]["login"], "carol");
    let inputs: Value = serde_json::from_str(&inputs.unwrap()).unwrap();
    assert_eq!(inputs, json!({ "level": "high", "debug": true, "count": 3.0 }));
    let jobs = fixture.database.actions_runs().jobs(run.id).await.unwrap();
    assert_eq!(actions.task_context(&run, &jobs[0]).await.unwrap()["inputs"], inputs);

    let path =
        actions.dispatch(&fixture.carol, &repo, &request(".forgejo/workflows/manual.yml", "refs/heads/main", &[("level", "low")])).await;
    assert_eq!(path.unwrap().workflow_path, ".forgejo/workflows/manual.yml");

    assert_eq!(
        invalid(actions.dispatch(&fixture.carol, &repo, &request("manual.yml", "main", &[])).await),
        "the input `level` is required"
    );
    let wrong = invalid(actions.dispatch(&fixture.carol, &repo, &request("manual.yml", "main", &[("level", "medium")])).await);
    assert!(wrong.contains("level"), "{wrong}");
    let unknown = invalid(actions.dispatch(&fixture.carol, &repo, &request("manual.yml", "main", &[("level", "low"), ("x", "1")])).await);
    assert_eq!(unknown, "the workflow has no input `x`");
    let push_only = invalid(actions.dispatch(&fixture.carol, &repo, &request("push.yml", "main", &[])).await);
    assert_eq!(push_only, ".forgejo/workflows/push.yml does not run on workflow_dispatch");
    assert_eq!(
        invalid(actions.dispatch(&fixture.carol, &repo, &request("gone.yml", "main", &[])).await),
        "there is no workflow `gone.yml` at this ref"
    );
    assert!(matches!(actions.dispatch(&fixture.carol, &repo, &request("../x.yml", "main", &[])).await, Err(ServiceError::Invalid(_))));
}

#[tokio::test]
async fn dispatch_needs_write_access_and_an_own_branch_or_tag() {
    let fixture = Fixture::new().await;
    let head = fixture.commit(&[(".forgejo/workflows/manual.yml", MANUAL)], "CI");
    fixture.push_ref("refs/heads/main").await;
    fixture.push_ref("refs/tags/v1").await;
    fixture.push_ref("refs/pull/1/head").await;
    let actions = fixture.actions();
    let inputs = [("level", "low")];

    let reader = fixture.repo(&fixture.rita).await;
    assert!(matches!(
        actions.dispatch(&fixture.rita, &reader, &request("manual.yml", "main", &inputs)).await,
        Err(ServiceError::Forbidden)
    ));
    let stranger = fixture.repo(&fixture.bob).await;
    assert!(matches!(
        actions.dispatch(&fixture.bob, &stranger, &request("manual.yml", "main", &inputs)).await,
        Err(ServiceError::Forbidden)
    ));

    let repo = fixture.repo(&fixture.alice).await;
    let tag = actions.dispatch(&fixture.alice, &repo, &request("manual.yml", "v1", &inputs)).await.unwrap();
    assert_eq!((tag.reference.as_str(), tag.sha.as_str()), ("refs/tags/v1", head.as_str()));
    for refused in ["refs/pull/1/head", "pull/1/head", head.as_str(), &head[..7], "refs/heads/nope", "", "HEAD", "refs/remotes/x"] {
        let message = invalid(actions.dispatch(&fixture.alice, &repo, &request("manual.yml", refused, &inputs)).await);
        assert_eq!(message, "the ref must be a branch or tag of this repository", "{refused}");
    }

    fixture.database.actions_settings().set_enabled(repo.repo.repo.id, false, Timestamp::now()).await.unwrap();
    assert!(matches!(actions.dispatch(&fixture.alice, &repo, &request("manual.yml", "main", &inputs)).await, Err(ServiceError::NotFound)));
}

const T0: i64 = 1_767_225_600; // 2026-01-01T00:00:00Z

fn scheduled(cron: &str) -> String {
    format!(
        "name: Nightly\non:\n  schedule:\n    - cron: '{cron}'\n    - cron: '{cron}'\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x}}]\n"
    )
}

#[tokio::test]
async fn schedules_fire_on_time_and_wait_for_the_previous_run() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(".forgejo/workflows/nightly.yml", &scheduled("*/15 * * * *"))], "Schedule");
    fixture.push_ref("refs/heads/main").await;
    let owned = fixture.repo(&fixture.alice).await.repo;
    let actions = fixture.actions();
    let head = fixture.repo(&fixture.alice).await.reader.commit_id("main".into()).await.unwrap();
    assert_eq!(actions.refresh_schedules(&owned, &head, Timestamp(T0)).await.unwrap(), 1, "a repeated expression counts once");
    let entries = fixture.database.actions_schedules().list(owned.repo.id).await.unwrap();
    assert_eq!((entries[0].cron.as_str(), entries[0].next_at), ("*/15 * * * *", Timestamp(T0 + 900)));

    assert!(actions.tick(Timestamp(T0 + 899)).await.unwrap().is_empty(), "not yet");
    let first = actions.tick(Timestamp(T0 + 900)).await.unwrap();
    assert_eq!(first.len(), 1);
    let run = &first[0];
    assert_eq!(
        (run.event, run.title.as_str(), run.actor, run.reference.as_str()),
        (RunEvent::Schedule, "Scheduled", None, "refs/heads/main")
    );
    assert_eq!(run.sha, head);
    let (event, _) = fixture.database.actions_runs().event(run.id).await.unwrap().unwrap();
    assert_eq!(serde_json::from_str::<Value>(&event).unwrap()["schedule"], "*/15 * * * *");
    assert!(actions.tick(Timestamp(T0 + 901)).await.unwrap().is_empty(), "the next fire is a quarter later");

    assert!(actions.tick(Timestamp(T0 + 1800)).await.unwrap().is_empty(), "the previous run still waits");
    let entries = fixture.database.actions_schedules().list(owned.repo.id).await.unwrap();
    assert_eq!(entries[0].next_at, Timestamp(T0 + 2700), "rescheduled, not retried every tick");
    fixture.database.actions_runs().cancel_run(owned.repo.id, run.id, Timestamp(T0 + 2000)).await.unwrap();
    assert_eq!(actions.tick(Timestamp(T0 + 2700)).await.unwrap().len(), 1);

    fixture.database.actions_settings().set_enabled(owned.repo.id, false, Timestamp::now()).await.unwrap();
    assert!(actions.tick(Timestamp(T0 + 3600)).await.unwrap().is_empty(), "Actions is off for the repository");
}

#[tokio::test]
async fn schedules_follow_the_default_branch_and_their_caps() {
    let fixture = Fixture::with(|config| config.actions.max_schedules_per_repository = 2).await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let schedules = fixture.database.actions_schedules();
    let source = PushSource::User(fixture.alice.id());
    let three = "on:\n  schedule:\n    - cron: '0 1 * * *'\n    - cron: '0 2 * * *'\n    - cron: '0 3 * * *'\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    fixture.commit(&[(".forgejo/workflows/nightly.yml", three)], "Schedule [skip ci]");
    let update = fixture.push_ref("refs/heads/feature").await;
    fixture.actions().handle_push(repo, source, &[update]).await.unwrap();
    assert!(schedules.list(repo).await.unwrap().is_empty(), "only the default branch counts");
    let update = fixture.push_ref("refs/heads/main").await;
    fixture.actions().handle_push(repo, PushSource::Mirror, std::slice::from_ref(&update)).await.unwrap();
    assert!(schedules.list(repo).await.unwrap().is_empty(), "a mirror's history schedules nothing");
    fixture.actions().handle_push(repo, source, &[update]).await.unwrap();
    let crons: Vec<_> = schedules.list(repo).await.unwrap().into_iter().map(|entry| entry.cron).collect();
    assert_eq!(crons, ["0 1 * * *", "0 2 * * *"], "at most two entries, even with a skip marker");

    fixture.commit(&[(".forgejo/workflows/nightly.yml", "")], "No schedule");
    let update = fixture.push_ref("refs/heads/main").await;
    fixture.actions().handle_push(repo, source, &[update]).await.unwrap();
    assert!(schedules.list(repo).await.unwrap().is_empty(), "the schedule left with the workflow");
}

#[tokio::test]
async fn schedules_that_fail_to_plan_pause_and_removed_ones_are_dropped() {
    let fixture = Fixture::new().await;
    // `github.run_number` does not exist before the run does, so the group is empty: a
    // planning error.
    let broken = "on:\n  schedule:\n    - cron: '0 * * * *'\nconcurrency: ${{ github.run_number }}\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    fixture.commit(&[(".forgejo/workflows/hourly.yml", broken)], "Broken");
    fixture.push_ref("refs/heads/main").await;
    let owned = fixture.repo(&fixture.alice).await.repo;
    let head = fixture.repo(&fixture.alice).await.reader.commit_id("main".into()).await.unwrap();
    let actions = fixture.actions();
    actions.refresh_schedules(&owned, &head, Timestamp(T0)).await.unwrap();
    for hour in 1..=3 {
        let runs = actions.tick(Timestamp(T0 + hour * 3600)).await.unwrap();
        assert!(runs[0].is_workflow_error(), "{runs:?}");
    }
    let entries = fixture.database.actions_schedules().list(owned.repo.id).await.unwrap();
    assert!(entries[0].paused, "three failures in a row pause it");
    assert!(actions.tick(Timestamp(T0 + 4 * 3600)).await.unwrap().is_empty(), "paused");

    // A workflow that lost the entry since the table was built refreshes it.
    actions.refresh_schedules(&owned, &head, Timestamp(T0)).await.unwrap();
    fixture.commit(&[(".forgejo/workflows/hourly.yml", &workflow("Push", "push"))], "No schedule");
    fixture.push_ref("refs/heads/main").await;
    assert!(actions.tick(Timestamp(T0 + 3600)).await.unwrap().is_empty(), "the entry is gone");
    assert!(fixture.database.actions_schedules().list(owned.repo.id).await.unwrap().is_empty(), "and so is the table");
}
