//! Actions: `workflow_run` and `repository_dispatch`. Runs that other runs and the REST API
//! start: what matches, what the event says, the default branch they run, the chain limit,
//! and who may dispatch.

use rbit_core::{
    PushSource, Timestamp,
    actions::{RunEvent, Status},
};
use rbit_services::{ServiceError, actions::hooks::RunAction};
use serde_json::{Value, json};

use crate::actions_fixture::{Fixture, workflow};

const AFTER_CI: &str = "name: After
on:
  workflow_run:
    workflows: [CI]
    types: [completed]
jobs:
  report:
    runs-on: ubuntu-latest
    steps: [{run: echo done}]
";

/// A workflow that runs after itself (and after CI), to test the chain limit.
const LOOP: &str = "name: Loop
on:
  workflow_run:
    workflows: [CI, Loop]
    types: [completed]
jobs:
  again:
    runs-on: ubuntu-latest
    steps: [{run: echo again}]
";

async fn payload(fixture: &Fixture, run: rbit_core::actions::RunId) -> Value {
    serde_json::from_str(&fixture.database.actions_runs().event(run).await.unwrap().unwrap().0).unwrap()
}

#[tokio::test]
async fn a_finished_run_starts_the_workflows_that_wait_for_it() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(".rbit/workflows/ci.yml", &workflow("CI", "push")), (".rbit/workflows/after.yml", AFTER_CI)], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let pushed = actions.handle_push(repo, PushSource::User(fixture.alice.id()), std::slice::from_ref(&update)).await.unwrap();
    assert_eq!(pushed.len(), 1, "only CI runs on the push");
    let ci = pushed[0].clone();

    assert!(actions.handle_workflow_run(repo, ci.id, RunAction::Requested).await.unwrap().is_empty(), "types: [completed]");
    fixture.database.actions_runs().cancel_run(repo, ci.id, Timestamp::now()).await.unwrap();
    let started = actions.handle_workflow_run(repo, ci.id, RunAction::Completed).await.unwrap();
    assert_eq!(started.len(), 1);
    let after = &started[0];
    assert_eq!((after.event, after.workflow_name.as_str(), after.trusted), (RunEvent::WorkflowRun, "After", true));
    assert_eq!((after.reference.as_str(), after.sha.as_str()), ("refs/heads/main", update.after.as_deref().unwrap()));
    assert_eq!((after.actor, after.status), (Some(fixture.alice.id()), Status::Waiting));
    let event = payload(&fixture, after.id).await;
    assert_eq!(event["action"], "completed");
    assert_eq!(event["workflow_run"]["id"], ci.id.0);
    assert_eq!(event["workflow_run"]["conclusion"], "cancelled");
    assert_eq!(event["workflow_run"]["head_sha"], json!(ci.sha));
    assert_eq!(event["workflow"]["name"], "CI");
    assert_eq!(event["sender"]["login"], "alice");
}

#[tokio::test]
async fn branch_filters_read_the_triggering_run_and_the_default_branch_runs() {
    let fixture = Fixture::new().await;
    let release = "name: Release
on:
  workflow_run:
    workflows: [CI]
    branches: ['release/**']
jobs:
  ship:
    runs-on: ubuntu-latest
    steps: [{run: echo ship}]
";
    fixture.commit(&[(".rbit/workflows/ci.yml", &workflow("CI", "push")), (".rbit/workflows/release.yml", release)], "CI");
    let main = fixture.push_ref("refs/heads/main").await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let on_main = actions.handle_push(repo, PushSource::User(fixture.alice.id()), std::slice::from_ref(&main)).await.unwrap();
    assert!(actions.handle_workflow_run(repo, on_main[0].id, RunAction::Completed).await.unwrap().is_empty(), "main is no release branch");

    // A release branch whose own copy of the workflow differs: the default branch's runs.
    fixture.commit(&[(".rbit/workflows/release.yml", "")], "drop it on the branch");
    let branch = fixture.push_ref("refs/heads/release/1.0").await;
    let on_branch = actions.handle_push(repo, PushSource::User(fixture.alice.id()), &[branch]).await.unwrap();
    let started = actions.handle_workflow_run(repo, on_branch[0].id, RunAction::InProgress).await.unwrap();
    assert_eq!(started.len(), 1, "all types without `types`, and the default branch's file");
    assert_eq!((started[0].reference.as_str(), started[0].sha.as_str()), ("refs/heads/main", main.after.as_deref().unwrap()));
}

#[tokio::test]
async fn chains_stop_after_three_levels() {
    let fixture = Fixture::new().await;
    fixture.commit(&[(".rbit/workflows/ci.yml", &workflow("CI", "push")), (".rbit/workflows/loop.yml", LOOP)], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let mut current = actions.handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    for level in 1..=3 {
        let started = actions.handle_workflow_run(repo, current.id, RunAction::Completed).await.unwrap();
        assert_eq!(started.len(), 1, "level {level}");
        current = started.into_iter().next().unwrap();
    }
    assert!(actions.handle_workflow_run(repo, current.id, RunAction::Completed).await.unwrap().is_empty(), "a fourth level never runs");
}

#[tokio::test]
async fn writers_dispatch_repository_events_with_their_payload() {
    let fixture = Fixture::new().await;
    let deploy = "name: Deploy
on:
  repository_dispatch:
    types: [deploy]
jobs:
  go:
    runs-on: ubuntu-latest
    steps: [{run: echo go}]
";
    fixture.commit(&[(".rbit/workflows/deploy.yml", deploy), (".rbit/workflows/any.yml", &workflow("Any", "repository_dispatch"))], "CI");
    fixture.push_ref("refs/heads/main").await;
    let actions = fixture.actions();
    let carol = fixture.repo(&fixture.carol).await;
    let runs = actions.repository_dispatch(&fixture.carol, &carol, "deploy", Some(json!({"env": "staging"}))).await.unwrap();
    let names: Vec<_> = runs.iter().map(|run| run.workflow_name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(runs.iter().all(|run| run.event == RunEvent::RepositoryDispatch && run.reference == "refs/heads/main" && run.trusted));
    let event = payload(&fixture, runs[0].id).await;
    assert_eq!((event["action"].as_str(), event["branch"].as_str()), (Some("deploy"), Some("main")));
    assert_eq!(event["client_payload"], json!({"env": "staging"}));
    let other = actions.repository_dispatch(&fixture.carol, &carol, "rollback", None).await.unwrap();
    assert_eq!(other.iter().map(|run| run.workflow_name.as_str()).collect::<Vec<_>>(), ["Any"], "types filter");

    let rita = fixture.repo(&fixture.rita).await;
    assert!(matches!(actions.repository_dispatch(&fixture.rita, &rita, "deploy", None).await, Err(ServiceError::Forbidden)));
    let too_many: serde_json::Map<String, Value> = (0..11).map(|index| (format!("k{index}"), json!(index))).collect();
    for (event_type, payload) in
        [("", None), (&*"x".repeat(101), None), ("deploy", Some(json!([1]))), ("deploy", Some(Value::Object(too_many)))]
    {
        let refused = actions.repository_dispatch(&fixture.carol, &carol, event_type, payload).await;
        assert!(matches!(refused, Err(ServiceError::Invalid(_))), "{event_type:?}: {refused:?}");
    }
}
