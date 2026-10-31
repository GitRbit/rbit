//! Runtime Actions activation, authority revocation and persisted disable recovery.

use std::{sync::Arc, time::Duration};

use rbit_auth::TaskTokenSecret;
use rbit_core::{
    PushSource, Timestamp, User,
    actions::{RunId, Status, TaskId},
    status::StatusState,
};
use rbit_plugin_api::{DesiredState, PluginState};
use rbit_services::{
    ServiceError, Services,
    actions::runners::{ManagedScope, RegistrationOptions},
};
use rbit_workflow::{
    protocol::{Backend, Capabilities, DeclaredLabel, PollRequest, RegisterRequest},
    secret::Secret,
};

use crate::actions_fixture::{Fixture, workflow};

const ACTIONS_ID: &str = "rbit.actions";
const TEST_DEADLINE: Duration = Duration::from_secs(5);

fn administrator(fixture: &Fixture) -> User {
    let mut actor = fixture.alice.clone();
    actor.is_admin = true;
    actor
}

/// Wait for both cached lifecycle progress and host publication to reflect quiescing.
/// This must stay observable while an admitted response body prevents final cleanup.
async fn wait_for_unpublished(services: &Services) {
    tokio::time::timeout(TEST_DEADLINE, async {
        loop {
            let status = services.actions_status().await.unwrap();
            if status.state == PluginState::Quiescing && services.actions.is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disabling must unpublish before waiting for old operations");
}

async fn wait_for_disabled(services: &Services) {
    tokio::time::timeout(TEST_DEADLINE, async {
        loop {
            if services.actions_status().await.unwrap().state == PluginState::Disabled {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the admitted lifecycle operation must finish after its waiter disappears");
}

struct RunningWork {
    run: RunId,
    task: TaskId,
    token: String,
    runner_credential: String,
}

/// Exercise the real registration, scheduler and credential minting path so later
/// revocation assertions start with a token that was demonstrably authorized.
async fn running_work(fixture: &Fixture) -> RunningWork {
    let actions = fixture.actions();
    fixture.commit(&[(".rbit/workflows/ci.yml", &workflow("CI", "push"))], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await;
    let run = actions.handle_push(repo.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    let registration =
        actions.create_registration_token(&fixture.alice, ManagedScope::Repository(&repo), RegistrationOptions::default()).await.unwrap();
    let registered = actions
        .register_runner(&RegisterRequest {
            token: Secret::new(registration.secret.expose()),
            name: "plugin-lifecycle-runner".into(),
            labels: vec![DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container }],
            capabilities: Capabilities {
                backends: vec![Backend::Container],
                os: "linux".into(),
                arch: "x86_64".into(),
                capacity: 1,
                version: "0.1.5".into(),
                ephemeral: false,
            },
        })
        .await
        .unwrap();
    let runner_credential = registered.credential.expose().to_owned();
    let runner = actions.authenticate_runner(&runner_credential).await.unwrap().unwrap();
    let spec =
        tokio::time::timeout(TEST_DEADLINE, actions.poll(&runner, &PollRequest { free_slots: 1 }, None)).await.unwrap().unwrap().unwrap();
    let token = spec.token.expose().to_owned();
    let grant = actions.authenticate_task(&token).await.unwrap().expect("the running task token is authorized before disable");
    assert_eq!(grant.run, run.id);
    RunningWork { run: run.id, task: TaskId(spec.task_id.0), token, runner_credential }
}

#[tokio::test]
async fn runtime_enable_and_disable_unpublish_before_draining_and_revoke_old_handles() {
    let fixture = Fixture::with(|config| config.actions.enabled = false).await;
    let admin = administrator(&fixture);
    assert!(fixture.services.actions.is_none());
    assert_eq!(fixture.database.plugin_settings().desired_enabled(ACTIONS_ID).await.unwrap(), None);
    fixture.services.set_actions_enabled(&admin, true).await.unwrap();
    let old = fixture.actions();
    let lease = old.admit().unwrap();
    let generation = lease.generation();
    let old_ui = fixture.services.ui_snapshot();
    assert_eq!(old_ui.entries.len(), 5);
    assert!(old_ui.entries.iter().all(|entry| entry.owner.as_str() == ACTIONS_ID && entry.generation == generation));
    assert_eq!(fixture.services.actions_status().await.unwrap().state, PluginState::Active);
    let work = running_work(&fixture).await;
    old.spawn_background();
    assert!(old.background_tasks() > 1, "background loops must be tracked alongside the response lease");
    let services = fixture.services.clone();
    let disable = tokio::spawn(async move { services.set_actions_enabled(&admin, false).await });
    wait_for_unpublished(&fixture.services).await;
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0, "UI must disappear while old operations are still draining");
    assert_eq!(old_ui.entries.len(), 5, "an in-flight render retains pure presentation data without blocking disable");

    assert!(!disable.is_finished(), "the admitted response must keep cleanup incomplete");
    assert!(fixture.services.actions_status().await.unwrap().in_flight >= 1);
    assert!(old.admit().is_err(), "retaining a prior service cannot bypass closed admission");
    assert!(old.authenticate_task(&work.token).await.unwrap().is_none());
    assert!(old.authenticate_runner(&work.runner_credential).await.unwrap().is_none());
    assert_eq!(fixture.database.plugin_settings().desired_enabled(ACTIONS_ID).await.unwrap(), Some(false));
    drop(lease);
    tokio::time::timeout(TEST_DEADLINE, disable).await.unwrap().unwrap().unwrap();

    let disabled = fixture.services.actions_status().await.unwrap();
    assert_eq!((disabled.desired, disabled.state), (DesiredState::Disabled, PluginState::Disabled));
    assert_eq!(old.background_tasks(), 0);
    assert_eq!(fixture.services.actions_background_tasks(), 0);
    assert_eq!(fixture.database.actions_runs().run(work.run).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(fixture.database.actions_runs().task(work.task).await.unwrap().unwrap().status, Status::Cancelled);
    assert!(!fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());

    fixture.services.set_actions_enabled(&administrator(&fixture), true).await.unwrap();
    let current = fixture.actions();
    let new_lease = current.admit().unwrap();
    assert_ne!(new_lease.generation(), generation, "reactivation must not resurrect stale handles");
    let new_ui = fixture.services.ui_snapshot();
    assert_eq!(new_ui.entries.len(), 5, "reactivation replaces registrations without duplicating them");
    assert!(new_ui.entries.iter().all(|entry| entry.generation == new_lease.generation()));
    assert!(new_ui.revision > old_ui.revision);
    assert!(current.authenticate_task(&work.token).await.unwrap().is_none());
    assert!(old.admit().is_err());
    drop(new_lease);
    fixture.services.shutdown().await;
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0);
}

#[tokio::test]
async fn non_administrators_cannot_change_runtime_or_persisted_intent() {
    let fixture = Fixture::with(|config| config.actions.enabled = false).await;
    let revision = fixture.services.plugin_revision();
    for enabled in [true, false] {
        assert!(matches!(fixture.services.set_actions_enabled(&fixture.alice, enabled).await, Err(ServiceError::Forbidden)));
        assert_eq!(fixture.database.plugin_settings().desired_enabled(ACTIONS_ID).await.unwrap(), None);
        assert!(fixture.services.actions.is_none());
        assert_eq!(fixture.services.plugin_revision(), revision);
    }
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn explicit_plugin_intent_survives_restart_and_overrides_startup_defaults() {
    let fixture = Fixture::with(|config| config.actions.enabled = false).await;
    let admin = administrator(&fixture);
    let ctx = fixture.services.context().clone();
    fixture.services.set_actions_enabled(&admin, true).await.unwrap();
    fixture.services.shutdown().await;

    let resumed = Services::new(Arc::clone(&ctx.config), &fixture.database, &ctx.kv).await.unwrap();
    assert!(resumed.actions.is_some(), "persisted enablement overrides the off startup default");
    resumed.set_actions_enabled(&admin, false).await.unwrap();
    resumed.shutdown().await;
    let mut enabled_default = (*ctx.config).clone();
    enabled_default.actions.enabled = true;
    let disabled = Services::new(Arc::new(enabled_default), &fixture.database, &ctx.kv).await.unwrap();
    assert!(disabled.actions.is_none(), "persisted disablement overrides the on startup default");
    assert_eq!(fixture.database.plugin_settings().desired_enabled(ACTIONS_ID).await.unwrap(), Some(false));
    disabled.shutdown().await;
}

#[tokio::test]
async fn interrupted_disable_is_reconciled_before_any_fresh_generation_can_authenticate() {
    let fixture = Fixture::new().await;
    let admin = administrator(&fixture);
    let ctx = fixture.services.context().clone();
    let work = running_work(&fixture).await;
    let digest = TaskTokenSecret::parse(&work.token).unwrap().digest();
    fixture.database.plugin_settings().set_desired_enabled(ACTIONS_ID, false, Some(admin.id()), Timestamp::now()).await.unwrap();
    assert!(fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());

    // Stop old in-memory resources without the administrative disable callback. This
    // leaves the exact durable boundary a crash after intent persistence would leave.
    fixture.services.shutdown().await;
    assert!(fixture.database.actions_runs().running_task_by_token(&digest).await.unwrap().is_some());
    let recovered = Services::new(Arc::clone(&ctx.config), &fixture.database, &ctx.kv).await.unwrap();
    assert!(recovered.actions.is_none());
    assert!(!fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());
    assert!(fixture.database.actions_runs().running_task_by_token(&digest).await.unwrap().is_none());
    assert_eq!(fixture.database.actions_runs().run(work.run).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(fixture.database.actions_runs().task(work.task).await.unwrap().unwrap().status, Status::Cancelled);

    recovered.set_actions_enabled(&admin, true).await.unwrap();
    assert!(recovered.actions.snapshot().unwrap().authenticate_task(&work.token).await.unwrap().is_none());
    recovered.shutdown().await;
}

#[tokio::test]
async fn recovery_retries_check_reports_after_the_run_was_already_cancelled() {
    let fixture = Fixture::new().await;
    let admin = administrator(&fixture);
    let ctx = fixture.services.context().clone();
    let work = running_work(&fixture).await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let store = fixture.database.actions_runs();
    let sha = store.run(work.run).await.unwrap().unwrap().sha;
    fixture.database.plugin_settings().set_desired_enabled(ACTIONS_ID, false, Some(admin.id()), Timestamp::now()).await.unwrap();
    store.queue_plugin_revocations().await.unwrap();
    store.cancel_run(repo, work.run, Timestamp::now()).await.unwrap();
    assert_eq!(store.run(work.run).await.unwrap().unwrap().status, Status::Cancelled);
    assert!(fixture.database.commit_statuses().latest(repo, &sha).await.unwrap().iter().any(|check| check.state == StatusState::Pending));
    fixture.services.shutdown().await;

    // A terminal run must remain in the recovery journal until its checks and any
    // surviving runner attempts are reconciled, even though it is no longer unfinished.
    let recovered = Services::new(Arc::clone(&ctx.config), &fixture.database, &ctx.kv).await.unwrap();
    assert_eq!(store.task(work.task).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(store.pending_plugin_revocations().await.unwrap(), Vec::new());
    let checks = fixture.database.commit_statuses().latest(repo, &sha).await.unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].state, StatusState::Error, "cancelled builds must stop reporting pending checks after recovery");
    assert!(!fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());
    recovered.shutdown().await;
}

#[tokio::test]
async fn cancelling_the_request_waiter_does_not_abandon_disable_or_release_its_lifecycle_lock() {
    let fixture = Fixture::new().await;
    let admin = administrator(&fixture);
    let old = fixture.actions();
    let lease = old.admit().unwrap();
    let services = fixture.services.clone();
    let actor = admin.clone();
    let request = tokio::spawn(async move { services.set_actions_enabled(&actor, false).await });
    wait_for_unpublished(&fixture.services).await;
    request.abort();
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0, "aborting the HTTP waiter cannot resurrect withdrawn UI");
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(matches!(fixture.services.set_actions_enabled(&admin, true).await, Err(ServiceError::Conflict("plugin lifecycle operation"))));
    assert_eq!(fixture.services.actions_status().await.unwrap().state, PluginState::Quiescing);
    assert!(fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());
    drop(lease);

    wait_for_disabled(&fixture.services).await;
    assert_eq!(old.background_tasks(), 0);
    assert_eq!(fixture.services.actions_background_tasks(), 0);
    assert_eq!(fixture.database.plugin_settings().desired_enabled(ACTIONS_ID).await.unwrap(), Some(false));
    assert!(!fixture.database.plugin_settings().disable_pending(ACTIONS_ID).await.unwrap());
    // Disabled is published before the audit write finishes. The host retains its
    // lifecycle lock through that write even after the request waiter was cancelled.
    tokio::time::timeout(TEST_DEADLINE, async {
        loop {
            match fixture.services.set_actions_enabled(&admin, true).await {
                Ok(()) => return,
                Err(ServiceError::Conflict("plugin lifecycle operation")) => tokio::task::yield_now().await,
                other => panic!("unexpected reactivation outcome after cleanup: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert!(fixture.services.actions.is_some());
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 5);
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn repeated_activation_cycles_replace_ui_generations_without_leaking_contributions() {
    let fixture = Fixture::with(|config| config.actions.enabled = false).await;
    let admin = administrator(&fixture);
    let mut previous_generation = None;
    for _ in 0..3 {
        assert_eq!(fixture.services.ui_snapshot().entries.len(), 0);
        fixture.services.set_actions_enabled(&admin, true).await.unwrap();
        let snapshot = fixture.services.ui_snapshot();
        assert_eq!(snapshot.entries.len(), 5);
        let generation = snapshot.entries[0].generation;
        assert_ne!(Some(generation), previous_generation);
        assert!(snapshot.entries.iter().all(|entry| entry.generation == generation));
        previous_generation = Some(generation);
        fixture.services.set_actions_enabled(&admin, false).await.unwrap();
        assert_eq!(fixture.services.ui_snapshot().entries.len(), 0);
    }
    fixture.services.shutdown().await;
}
