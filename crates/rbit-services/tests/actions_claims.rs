//! Real scheduler delegation, token replacement and spent-key SQL work.
use crate::actions_fixture::{Fixture, workflow};
use rbit_core::{
    PushSource, Timestamp,
    actions::{Status, TaskId},
};
use rbit_services::actions::runners::{CachedRunner, ManagedScope, RegistrationOptions};
use rbit_storage::count_work;
use rbit_workflow::protocol::{Backend, Capabilities, DeclaredLabel, PollRequest, RegisterRequest};
use rbit_workflow::secret::Secret;
use std::sync::Arc;

async fn runner(fixture: &Fixture) -> Arc<CachedRunner> {
    let repo = fixture.repo(&fixture.alice).await;
    let actions = fixture.actions();
    let registration =
        actions.create_registration_token(&fixture.alice, ManagedScope::Repository(&repo), RegistrationOptions::default()).await.unwrap();
    let registered = actions
        .register_runner(&RegisterRequest {
            token: Secret::new(registration.secret.expose().to_owned()),
            name: "claim-runner".into(),
            labels: vec![DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container }],
            capabilities: Capabilities {
                backends: vec![Backend::Container],
                os: "linux".into(),
                arch: "x86_64".into(),
                capacity: 1,
                version: "0.2.0".into(),
                ephemeral: false,
                exclusive_labels: Vec::new(),
            },
        })
        .await
        .unwrap();
    actions.authenticate_runner(registered.credential.expose()).await.unwrap().unwrap()
}

async fn seed(fixture: &Fixture) {
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push"))], "Runner claim fixture");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await;
    fixture.actions().handle_push(repo.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap();
}

#[tokio::test]
async fn remembered_replay_rotates_only_token_and_spent_key_uses_one_read_without_new_claim() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    seed(&fixture).await;
    let runner = runner(&fixture).await;
    let actions = fixture.actions();
    let request = PollRequest { free_slots: 1 };
    let first = actions.poll(&runner, &request, Some("same-key")).await.unwrap().unwrap();
    let (replayed, replay_work) = count_work(actions.poll(&runner, &request, Some("same-key"))).await;
    let replayed = replayed.unwrap().unwrap();
    let (again, again_work) = count_work(actions.poll(&runner, &request, Some("same-key"))).await;
    let again = again.unwrap().unwrap();
    assert_eq!((first.task_id, replayed.task_id), (again.task_id, again.task_id));
    assert_eq!(replay_work, again_work, "same specification assembly and token rotation work");
    assert_ne!(first.token, replayed.token);
    assert_ne!(replayed.token, again.token);
    let store = fixture.database.actions_runs();
    let old = rbit_auth::TaskTokenSecret::parse(first.token.expose()).unwrap().digest();
    let current = rbit_auth::TaskTokenSecret::parse(again.token.expose()).unwrap().digest();
    assert!(store.running_task_by_token(&old).await.unwrap().is_none());
    assert!(store.running_task_by_token(&current).await.unwrap().is_some());
    store.finish_task(TaskId(first.task_id.0), Status::Success, Timestamp::now()).await.unwrap().unwrap();
    seed(&fixture).await;
    let (spent, work) = count_work(actions.poll(&runner, &request, Some("same-key"))).await;
    assert!(spent.unwrap().is_none());
    assert_eq!((work.acquisitions, work.statements), (1, 1), "spent key skips rotation/specification/queue");
    let next = actions.poll(&runner, &request, Some("new-key")).await.unwrap().unwrap();
    assert_ne!(next.task_id, first.task_id, "the spent key did not take queued work");
}
