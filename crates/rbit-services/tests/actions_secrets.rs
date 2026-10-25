//! Actions workstream U: secrets sealed under the key file and variables, managed by
//! repository administrators and owners, and what a task of a trusted or untrusted run
//! receives (§6.3, §11.10).

use rbit_core::{
    PushSource, Timestamp,
    actions::{Run, SealedSecret, SecretScope, SecretVisibility},
};
use rbit_services::{
    ServiceError,
    actions::secrets::{ActionsTarget, UNREADABLE},
    audit::AuditAction,
};

use crate::actions_fixture::{Fixture, workflow};

async fn pushed(fixture: &Fixture) -> Run {
    fixture.commit(&[(".rbit/workflows/ci.yml", &workflow("CI", "push")), (".forgejo/workflows/ci.yml", &workflow("CI", "push"))], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    fixture.actions().handle_push(repo, PushSource::User(fixture.alice.id()), &[update]).await.unwrap().remove(0)
}

fn key_path(fixture: &Fixture) -> std::path::PathBuf {
    fixture.dir.path().join("data").join("actions").join("secrets.key")
}

#[tokio::test]
async fn administrators_manage_repository_secrets_and_tasks_receive_them() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let (admin, writer) = (fixture.repo(&fixture.alice).await, fixture.repo(&fixture.carol).await);
    let all = SecretVisibility::All;

    assert!(matches!(
        actions.put_secret(&fixture.carol, ActionsTarget::Repository(&writer), "TOKEN", "x", all).await,
        Err(ServiceError::Forbidden)
    ));
    assert!(matches!(actions.list_secrets(ActionsTarget::Repository(&writer)).await, Err(ServiceError::Forbidden)));
    assert!(matches!(
        actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "GITHUB_TOKEN", "x", all).await,
        Err(ServiceError::Invalid(_))
    ));
    assert!(matches!(
        actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "EMPTY", "", all).await,
        Err(ServiceError::Invalid(_))
    ));
    assert!(matches!(
        actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "PICKY", "x", SecretVisibility::Selected).await,
        Err(ServiceError::Invalid(_))
    ));

    let info = actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "deploy_key", "s3cr3t-value", all).await.unwrap();
    assert_eq!(info.name.as_str(), "DEPLOY_KEY");
    assert!(key_path(&fixture).exists(), "the key file is created when needed");
    actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "DEPLOY_KEY", "replaced", all).await.unwrap();
    let listed = actions.list_secrets(ActionsTarget::Repository(&admin)).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].readable);

    // Neither the database nor the audit log holds the value.
    let audit = fixture.services.audit.recent_for(&fixture.alice, 10).await.unwrap();
    let set = audit.iter().filter(|event| event.action == AuditAction::ActionsSecretSet).collect::<Vec<_>>();
    assert_eq!(set.len(), 2);
    assert!(set.iter().all(|event| !format!("{event:?}").contains("replaced")));
    let sqlite = std::fs::read(fixture.dir.path().join("data").join("rbit.db")).unwrap();
    assert!(!sqlite.windows(8).any(|window| window == b"replaced"));

    actions.put_variable(&fixture.alice, ActionsTarget::Repository(&admin), "region", "eu-west").await.unwrap();
    let run = pushed(&fixture).await;
    assert!(run.trusted);
    let secrets = actions.task_secrets(&run).await.unwrap();
    assert_eq!(secrets.get("DEPLOY_KEY").map(String::as_str), Some("replaced"));
    assert_eq!(actions.task_variables(&run).await.unwrap().get("REGION").map(String::as_str), Some("eu-west"));

    let untrusted = Run { trusted: false, ..run };
    assert!(actions.task_secrets(&untrusted).await.unwrap().is_empty(), "untrusted runs get no secrets");
    assert!(actions.task_variables(&untrusted).await.unwrap().is_empty(), "nor variables");

    actions.delete_secret(&fixture.alice, ActionsTarget::Repository(&admin), "deploy_key").await.unwrap();
    assert!(matches!(
        actions.delete_secret(&fixture.alice, ActionsTarget::Repository(&admin), "DEPLOY_KEY").await,
        Err(ServiceError::NotFound)
    ));
    actions.delete_variable(&fixture.alice, ActionsTarget::Repository(&admin), "REGION").await.unwrap();
    assert!(actions.list_variables(ActionsTarget::Repository(&admin)).await.unwrap().is_empty(), "deleted");
}

#[tokio::test]
async fn repository_secrets_win_over_owner_secrets_of_the_same_name() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let admin = fixture.repo(&fixture.alice).await;
    let owner = ActionsTarget::User(&fixture.alice);
    actions.put_secret(&fixture.alice, owner, "SHARED", "from-owner", SecretVisibility::All).await.unwrap();
    actions.put_secret(&fixture.alice, owner, "ONLY_OWNER", "owner-only", SecretVisibility::All).await.unwrap();
    actions.put_secret(&fixture.alice, owner, "ELSEWHERE", "hidden", SecretVisibility::Selected).await.unwrap();
    actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "SHARED", "from-repo", SecretVisibility::All).await.unwrap();

    let run = pushed(&fixture).await;
    let secrets = actions.task_secrets(&run).await.unwrap();
    assert_eq!(secrets.get("SHARED").map(String::as_str), Some("from-repo"));
    assert_eq!(secrets.get("ONLY_OWNER").map(String::as_str), Some("owner-only"));
    assert!(!secrets.contains_key("ELSEWHERE"), "selected visibility without this repository");

    actions.set_secret_repositories(&fixture.alice, owner, "ELSEWHERE", &[admin.repo.repo.id]).await.unwrap();
    assert_eq!(actions.task_secrets(&run).await.unwrap().get("ELSEWHERE").map(String::as_str), Some("hidden"));
    let listed = actions.list_secrets(owner).await.unwrap();
    assert_eq!(listed.iter().find(|secret| secret.info.name.as_str() == "ELSEWHERE").unwrap().repositories, vec![admin.repo.repo.id]);
    assert!(matches!(
        actions.set_secret_repositories(&fixture.alice, ActionsTarget::Repository(&admin), "SHARED", &[]).await,
        Err(ServiceError::Invalid(_))
    ));
}

/// Stores secret `name` of `alice/rocket` as another key sealed it, straight through the
/// store: what a database restored next to a different key file holds.
async fn sealed_elsewhere(fixture: &Fixture, name: &str) {
    let repo = fixture.repo(&fixture.alice).await.repo.repo.id;
    let sealed = |_| SealedSecret { key_id: [9; 8], nonce: [0; 12], ciphertext: vec![0; 21] };
    let store = fixture.database.actions_secrets();
    let name = name.parse().unwrap();
    store
        .put_secret(SecretScope::Repository(repo), &name, SecretVisibility::All, fixture.alice.id(), Timestamp::now(), sealed)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_secret_sealed_with_another_key_fails_closed_until_set_again() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let admin = fixture.repo(&fixture.alice).await;
    actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "FINE", "value", SecretVisibility::All).await.unwrap();
    sealed_elsewhere(&fixture, "DEPLOY").await;
    let run = pushed(&fixture).await;

    let error = actions.task_secrets(&run).await.unwrap_err().to_string();
    assert!(error.contains("DEPLOY") && error.contains(UNREADABLE), "{error}");
    let listed = actions.list_secrets(ActionsTarget::Repository(&admin)).await.unwrap();
    let readable = listed.iter().map(|secret| (secret.info.name.as_str(), secret.readable)).collect::<Vec<_>>();
    assert_eq!(readable, [("DEPLOY", false), ("FINE", true)]);

    actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "DEPLOY", "again", SecretVisibility::All).await.unwrap();
    assert_eq!(actions.task_secrets(&run).await.unwrap().get("DEPLOY").map(String::as_str), Some("again"));
}

#[tokio::test]
async fn a_lost_key_is_not_recreated_while_secrets_are_stored() {
    let fixture = Fixture::new().await;
    let admin = fixture.repo(&fixture.alice).await;
    sealed_elsewhere(&fixture, "DEPLOY").await;
    let actions = fixture.actions();

    let problem = actions.secrets_problem().await.expect("secrets are unavailable");
    assert!(problem.0.contains("missing while secrets are stored"), "{problem}");
    assert!(!key_path(&fixture).exists());
    assert!(matches!(
        actions.put_secret(&fixture.alice, ActionsTarget::Repository(&admin), "OTHER", "v", SecretVisibility::All).await,
        Err(ServiceError::Invalid(_))
    ));
    let run = pushed(&fixture).await;
    assert!(matches!(actions.task_secrets(&run).await, Err(ServiceError::Invalid(_))), "a run that would get secrets fails closed");
    actions.delete_secret(&fixture.alice, ActionsTarget::Repository(&admin), "DEPLOY").await.unwrap();
    assert!(actions.task_secrets(&run).await.unwrap().is_empty());
}
