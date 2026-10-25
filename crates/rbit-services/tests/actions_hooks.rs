//! Actions integration (I): `workflow_run` and `workflow_job` actions follow a run's
//! transitions once each, and their webhooks have GitHub's shape.

use rbit_core::{
    PushSource, Timestamp,
    actions::{RunId, Status},
    webhook::WebhookEvent,
};
use rbit_services::{
    DomainEvent, ServiceError,
    actions::hooks::{JobAction, RunAction},
    webhooks::{HookTarget, SecretUpdate, WebhookInput},
};
use serde_json::Value;
use tokio::sync::broadcast::Receiver;

use crate::actions_fixture::Fixture;

const CHAIN: &str = "name: CI\non: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: echo a\n  b:\n    needs: a\n    runs-on: x\n    steps:\n      - run: echo b\n";

/// The Actions events published since the last call, as (job key or "run", action).
async fn announced(fixture: &Fixture, events: &mut Receiver<DomainEvent>, run: RunId) -> Vec<(String, &'static str)> {
    let jobs = fixture.database.actions_runs().jobs(run).await.unwrap();
    let key = |id| jobs.iter().find(|job| job.id == id).map(|job| job.key.clone()).unwrap();
    std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            DomainEvent::ActionsRun { action, .. } => Some(("run".to_owned(), action.as_str())),
            DomainEvent::ActionsJob { job, action, .. } => Some((key(job), action.as_str())),
            _ => None,
        })
        .collect()
}

fn pairs(expected: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    expected.iter().map(|(key, action)| ((*key).to_owned(), *action)).collect()
}

#[tokio::test]
async fn each_action_is_announced_once_as_the_run_moves() {
    let fixture = Fixture::new().await;
    let mut events = fixture.services.events.subscribe();
    let actions = fixture.actions();
    fixture.commit(&[(".forgejo/workflows/ci.yml", CHAIN)], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let run = actions.handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    assert_eq!(announced(&fixture, &mut events, run.id).await, pairs(&[("run", "requested"), ("a", "queued")]), "b waits on a");

    actions.report_statuses(run.id).await.unwrap();
    assert!(announced(&fixture, &mut events, run.id).await.is_empty(), "nothing changed, nothing is repeated");

    let claimed = fixture.claim(&["x"], 1).await.unwrap();
    actions.report_statuses(run.id).await.unwrap();
    assert_eq!(announced(&fixture, &mut events, run.id).await, pairs(&[("run", "in_progress"), ("a", "in_progress")]));

    let store = fixture.database.actions_runs();
    let settled = store.finish_task(claimed.task.id, Status::Success, Timestamp::now()).await.unwrap().unwrap();
    actions.settled(&settled).await.unwrap();
    assert_eq!(announced(&fixture, &mut events, run.id).await, pairs(&[("run", "requested"), ("a", "completed"), ("b", "queued")]));

    let repo_context = fixture.repo(&fixture.carol).await;
    actions.cancel(&fixture.carol, &repo_context, run.id).await.unwrap();
    assert_eq!(announced(&fixture, &mut events, run.id).await, pairs(&[("b", "completed"), ("run", "completed")]));
}

#[tokio::test]
async fn a_workflow_error_is_requested_and_completed_at_once() {
    let fixture = Fixture::new().await;
    let mut events = fixture.services.events.subscribe();
    fixture.commit(&[(".forgejo/workflows/broken.yml", "on: push\njobs: [\n")], "Broken");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let run = fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    assert!(run.is_workflow_error());
    assert_eq!(announced(&fixture, &mut events, run.id).await, pairs(&[("run", "requested"), ("run", "completed")]));
}

#[tokio::test]
async fn workflow_webhooks_have_githubs_shape() {
    let fixture = Fixture::with(|config| config.webhooks.allow_private_addresses = true).await;
    let repo = fixture.repo(&fixture.alice).await;
    let webhooks = &fixture.services.webhooks;
    let input = |events: &[&str]| WebhookInput {
        url: "http://127.0.0.1:9/hook".to_owned(),
        events: events.iter().map(|event| (*event).to_owned()).collect(),
        active: true,
        secret: SecretUpdate::Keep,
    };
    let hook = webhooks.create(&fixture.alice, HookTarget::Repository(&repo), input(&["workflow_run", "workflow_job"])).await.unwrap();
    assert_eq!(hook.events, [WebhookEvent::WorkflowRun, WebhookEvent::WorkflowJob]);

    fixture.commit(&[(".forgejo/workflows/ci.yml", CHAIN)], "Build it");
    let update = fixture.push_ref("refs/heads/main").await;
    let run = fixture.actions().handle_push(repo.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let repo_id = repo.repo.repo.id;
    let requested = DomainEvent::ActionsRun { repo: repo_id, run: run.id, action: RunAction::Requested };
    let queued = DomainEvent::ActionsJob { repo: repo_id, run: run.id, job: job.id, action: JobAction::Queued };
    assert_eq!(webhooks.handle_event(&requested).await.unwrap(), 1);
    assert_eq!(webhooks.handle_event(&queued).await.unwrap(), 1);

    let log = webhooks.deliveries(HookTarget::Repository(&repo), hook.id, 1, 10).await.unwrap();
    let delivery = |event| log.deliveries.iter().find(|delivery| delivery.event == event).unwrap();
    let workflow_run = delivery(WebhookEvent::WorkflowRun);
    assert_eq!(workflow_run.action.as_deref(), Some("requested"));
    let payload: Value = serde_json::from_str(&workflow_run.payload).unwrap();
    assert_eq!(payload["workflow_run"]["run_number"], run.number);
    assert_eq!(payload["workflow_run"]["head_sha"], run.sha.as_str());
    assert_eq!(payload["workflow_run"]["head_branch"], "main");
    assert_eq!(payload["workflow_run"]["status"], "queued");
    assert_eq!(payload["workflow_run"]["conclusion"], Value::Null);
    assert_eq!(payload["workflow_run"]["display_title"], "Build it");
    assert_eq!(payload["workflow_run"]["html_url"], format!("https://git.example.com/alice/rocket/actions/runs/{}", run.number));
    assert_eq!(payload["workflow"]["path"], ".forgejo/workflows/ci.yml");
    assert_eq!(payload["repository"]["full_name"], "alice/rocket");
    assert_eq!(payload["sender"]["login"], "alice");

    let workflow_job = delivery(WebhookEvent::WorkflowJob);
    assert_eq!(workflow_job.action.as_deref(), Some("queued"));
    let payload: Value = serde_json::from_str(&workflow_job.payload).unwrap();
    assert_eq!(payload["workflow_job"]["name"], "a");
    assert_eq!(payload["workflow_job"]["run_id"], run.id.0);
    assert_eq!(payload["workflow_job"]["labels"], serde_json::json!(["x"]));
    assert_eq!(payload["workflow_job"]["workflow_name"], "CI");
}

#[tokio::test]
async fn actions_events_need_actions() {
    let fixture = Fixture::with(|config| {
        config.actions.enabled = false;
        config.webhooks.allow_private_addresses = true;
    })
    .await;
    let repo = fixture.repo(&fixture.alice).await;
    let input = WebhookInput {
        url: "http://127.0.0.1:9/hook".to_owned(),
        events: vec!["workflow_run".to_owned()],
        active: true,
        secret: SecretUpdate::Keep,
    };
    let refused = fixture.services.webhooks.create(&fixture.alice, HookTarget::Repository(&repo), input).await;
    assert!(matches!(refused, Err(ServiceError::Invalid(message)) if message.contains("needs Actions")));
}
