//! Actions stream S: managing runners and registration tokens from settings — who may, what
//! is audited, and what disabling and removing a runner does to the runner cache and to
//! the tasks it was running.

use rbit_core::{
    PushSource,
    actions::{RunnerId, Status},
};
use rbit_services::{
    ServiceError,
    actions::runners::{ManagedScope, RegistrationOptions},
    audit::AuditAction,
};
use rbit_workflow::{
    protocol::{Backend, Capabilities, DeclaredLabel, RegisterRequest},
    secret::Secret,
};

use crate::actions_fixture::{Fixture, workflow};

fn request(token: &str, labels: &[&str]) -> RegisterRequest {
    RegisterRequest {
        token: Secret::new(token),
        name: "build-1".into(),
        labels: labels.iter().map(|name| DeclaredLabel { name: (*name).into(), backend: Backend::Container }).collect(),
        capabilities: Capabilities {
            backends: vec![Backend::Container],
            os: "linux".into(),
            arch: "x86_64".into(),
            capacity: 1,
            version: "0.2.0".into(),
            ephemeral: false,
        },
    }
}

#[tokio::test]
async fn only_managers_issue_registration_tokens() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let (owner, reader, stranger) =
        (fixture.repo(&fixture.alice).await, fixture.repo(&fixture.rita).await, fixture.repo(&fixture.bob).await);
    let options = RegistrationOptions::default;
    assert!(matches!(
        actions.create_registration_token(&fixture.rita, ManagedScope::Repository(&reader), options()).await,
        Err(ServiceError::Forbidden)
    ));
    assert!(matches!(
        actions.create_registration_token(&fixture.bob, ManagedScope::Repository(&stranger), options()).await,
        Err(ServiceError::Forbidden)
    ));
    assert!(matches!(
        actions.create_registration_token(&fixture.alice, ManagedScope::Instance, options()).await,
        Err(ServiceError::NotFound)
    ));
    let alice_account = fixture.alice.account.clone();
    assert!(matches!(
        actions.create_registration_token(&fixture.bob, ManagedScope::Owner(&alice_account), options()).await,
        Err(ServiceError::NotFound)
    ));
    let too_long = RegistrationOptions { lifetime: std::time::Duration::from_hours(31 * 24), ..options() };
    assert!(matches!(
        actions.create_registration_token(&fixture.alice, ManagedScope::Repository(&owner), too_long).await,
        Err(ServiceError::Invalid(_))
    ));

    let issued = actions.create_registration_token(&fixture.alice, ManagedScope::Repository(&owner), options()).await.unwrap();
    assert!(issued.secret.expose().starts_with("rbrg_"));
    assert_eq!((issued.token.uses_remaining, issued.token.expires_at.0 - issued.token.created_at.0), (Some(1), 3600));
    let personal = actions.create_registration_token(&fixture.alice, ManagedScope::Owner(&alice_account), options()).await.unwrap();
    let listed = actions.registration_tokens(&fixture.alice, ManagedScope::Repository(&owner)).await.unwrap();
    assert_eq!(listed.iter().map(|token| token.id).collect::<Vec<_>>(), [issued.token.id]);
    actions.revoke_registration_token(&fixture.alice, ManagedScope::Owner(&alice_account), personal.token.id).await.unwrap();
    assert!(matches!(
        actions.revoke_registration_token(&fixture.alice, ManagedScope::Repository(&owner), personal.token.id).await,
        Err(ServiceError::NotFound)
    ));
    let audit = fixture.services.audit.recent_for(&fixture.alice, 10).await.unwrap();
    let actions_logged: Vec<_> = audit.iter().map(|entry| entry.action).collect();
    assert!(actions_logged.contains(&AuditAction::ActionsRegistrationTokenCreated));
    assert!(actions_logged.contains(&AuditAction::ActionsRegistrationTokenRevoked));
}

#[tokio::test]
async fn disabling_and_removing_runners_reaches_the_cache_and_their_tasks() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let owner = fixture.repo(&fixture.alice).await;
    let scope = ManagedScope::Repository(&owner);
    let issued = actions.create_registration_token(&fixture.alice, scope, RegistrationOptions::default()).await.unwrap();
    let registered = actions.register_runner(&request(issued.secret.expose(), &["ubuntu-latest"])).await.unwrap();
    let credential = registered.credential.expose().to_owned();
    let id = RunnerId(registered.runner_id.0);
    assert!(!actions.authenticate_runner(&credential).await.unwrap().unwrap().disabled);

    let reader = fixture.repo(&fixture.rita).await;
    assert!(matches!(
        actions.set_runner_disabled(&fixture.rita, ManagedScope::Repository(&reader), id, true).await,
        Err(ServiceError::Forbidden)
    ));
    let disabled = actions.set_runner_disabled(&fixture.alice, scope, id, true).await.unwrap();
    assert!(disabled.disabled);
    assert!(actions.authenticate_runner(&credential).await.unwrap().unwrap().disabled, "the cache follows at once");
    actions.set_runner_disabled(&fixture.alice, scope, id, false).await.unwrap();
    assert_eq!(actions.runners(&fixture.alice, scope).await.unwrap().len(), 1);
    assert_eq!(actions.runners_for_repository(&reader).await.unwrap().len(), 1, "readers see the runners their jobs can use");

    // A runner that goes away fails what it was running.
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push"))], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let run = actions.handle_push(owner.repo.repo.id, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0);
    let claimed = fixture.claim(&["ubuntu-latest"], 7).await.unwrap();
    assert_eq!(claimed.job.run_id, run.id);
    let other = fixture.database.actions_runners().find(claimed.task.runner_id.unwrap()).await.unwrap().unwrap();
    actions.remove_runner(&fixture.alice, scope, other.id).await.unwrap();
    let task = fixture.database.actions_runs().task(claimed.task.id).await.unwrap().unwrap();
    assert_eq!(task.status, Status::Failure);
    let job = fixture.database.actions_runs().job(claimed.job.id).await.unwrap().unwrap();
    assert_eq!(job.status, Status::Failure);
    assert!(matches!(actions.remove_runner(&fixture.alice, scope, other.id).await, Err(ServiceError::NotFound)));

    actions.remove_runner(&fixture.alice, scope, id).await.unwrap();
    assert!(actions.authenticate_runner(&credential).await.unwrap().is_none(), "the credential stops working");
    let audit = fixture.services.audit.recent_for(&fixture.alice, 20).await.unwrap();
    for action in [AuditAction::ActionsRunnerDisabled, AuditAction::ActionsRunnerEnabled, AuditAction::ActionsRunnerRemoved] {
        assert!(audit.iter().any(|entry| entry.action == action), "{action:?}");
    }
}
