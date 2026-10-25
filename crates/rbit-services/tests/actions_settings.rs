//! Actions workstream U: a repository's Actions switch, default token permission and
//! trusted authors, managed by its administrators and audited, and the repositories an
//! owner secret can be given to.

use rbit_core::{PushSource, Timestamp, Visibility, actions::DefaultTokenPermission};
use rbit_services::{
    CreateOrganization, CreateRepository, ServiceError,
    actions::secrets::ActionsTarget,
    audit::{AuditAction, AuditEvent},
};

use crate::actions_fixture::{Fixture, workflow};

async fn audited(fixture: &Fixture, action: AuditAction) -> Vec<AuditEvent> {
    let events = fixture.services.audit.recent_for(&fixture.alice, 50).await.unwrap();
    events.into_iter().filter(|event| event.action == action).collect()
}

#[tokio::test]
async fn only_administrators_see_or_change_the_settings() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    for user in [&fixture.carol, &fixture.rita, &fixture.bob] {
        let repo = fixture.repo(user).await;
        assert!(matches!(actions.repo_settings(&repo).await, Err(ServiceError::Forbidden)), "{}", user.name());
        assert!(matches!(actions.set_repo_enabled(user, &repo, false).await, Err(ServiceError::Forbidden)));
        assert!(matches!(actions.set_token_permission(user, &repo, DefaultTokenPermission::Write).await, Err(ServiceError::Forbidden)));
        assert!(matches!(actions.untrust_user(user, &repo, fixture.bob.id()).await, Err(ServiceError::Forbidden)));
    }
    let settings = actions.repo_settings(&fixture.repo(&fixture.alice).await).await.unwrap();
    assert!(settings.enabled, "nothing changed");
    assert_eq!(settings.token_permission, DefaultTokenPermission::Read);
}

#[tokio::test]
async fn the_switch_and_the_token_permission_are_audited_when_they_change() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await;

    assert!(actions.set_repo_enabled(&fixture.alice, &repo, false).await.unwrap());
    assert!(!actions.set_repo_enabled(&fixture.alice, &repo, false).await.unwrap(), "already off");
    assert!(!actions.repo_settings(&repo).await.unwrap().enabled);
    let disabled = audited(&fixture, AuditAction::ActionsDisabled).await;
    assert_eq!(disabled.len(), 1, "a request that changes nothing records nothing");
    assert_eq!(disabled[0].target_name.as_deref(), Some("alice/rocket"));

    // Off means no new runs; turning it back on starts them again.
    fixture.commit(&[(".forgejo/workflows/ci.yml", &workflow("CI", "push"))], "CI");
    let update = fixture.push_ref("refs/heads/main").await;
    let push = PushSource::User(fixture.alice.id());
    let repo_id = repo.repo.repo.id;
    assert!(actions.handle_push(repo_id, push, std::slice::from_ref(&update)).await.unwrap().is_empty(), "no runs while off");
    assert!(actions.set_repo_enabled(&fixture.alice, &repo, true).await.unwrap());
    assert_eq!(actions.handle_push(repo_id, push, &[update]).await.unwrap().len(), 1);
    assert_eq!(audited(&fixture, AuditAction::ActionsEnabled).await.len(), 1);

    assert!(actions.set_token_permission(&fixture.alice, &repo, DefaultTokenPermission::Write).await.unwrap());
    assert!(!actions.set_token_permission(&fixture.alice, &repo, DefaultTokenPermission::Write).await.unwrap());
    let settings = actions.repo_settings(&repo).await.unwrap();
    assert!(settings.enabled, "the permission leaves the switch alone");
    assert_eq!(settings.token_permission, DefaultTokenPermission::Write);
    let changed = audited(&fixture, AuditAction::ActionsTokenPermissionChanged).await;
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].detail, "read to write");
}

#[tokio::test]
async fn trusted_authors_are_listed_and_can_be_removed() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let repo = fixture.repo(&fixture.alice).await;
    let store = fixture.database.actions_settings();
    store.trust(repo.repo.repo.id, fixture.bob.id(), fixture.alice.id(), Timestamp(1_000)).await.unwrap();
    store.trust(repo.repo.repo.id, fixture.rita.id(), fixture.carol.id(), Timestamp(2_000)).await.unwrap();

    let trusted = actions.repo_settings(&repo).await.unwrap().trusted;
    let names: Vec<(&str, Option<&str>)> = trusted
        .iter()
        .map(|author| (author.account.name.as_str(), author.approved_by.as_ref().map(|account| account.name.as_str())))
        .collect();
    assert_eq!(names, [("bob", Some("alice")), ("rita", Some("carol"))], "oldest first, with who approved them");
    assert_eq!(trusted[0].since, Timestamp(1_000));

    actions.untrust_user(&fixture.alice, &repo, fixture.bob.id()).await.unwrap();
    assert!(!store.is_trusted(repo.repo.repo.id, fixture.bob.id()).await.unwrap());
    assert!(matches!(actions.untrust_user(&fixture.alice, &repo, fixture.bob.id()).await, Err(ServiceError::NotFound)));
    let removed = audited(&fixture, AuditAction::ActionsUserUntrusted).await;
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].detail, "bob");
    assert_eq!(actions.repo_settings(&repo).await.unwrap().trusted.len(), 1);
}

#[tokio::test]
async fn owner_secrets_can_be_given_to_any_repository_of_the_owner() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let private = CreateRepository { name: "attic".into(), description: String::new(), visibility: Visibility::Private };
    fixture.services.repositories.create(&fixture.alice, private).await.unwrap();

    let repositories = actions.owner_repositories(ActionsTarget::User(&fixture.alice)).await.unwrap();
    let names: Vec<&str> = repositories.iter().map(|repo| repo.name.as_str()).collect();
    assert_eq!(names, ["attic", "rocket"], "private ones too, by name");
    let repo = fixture.repo(&fixture.alice).await;
    assert!(matches!(actions.owner_repositories(ActionsTarget::Repository(&repo)).await, Err(ServiceError::Invalid(_))));

    let org = CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() };
    fixture.services.orgs.create(&fixture.bob, org).await.unwrap();
    let outside = fixture.services.orgs.open(Some(&fixture.carol), "acme").await.unwrap();
    assert!(matches!(actions.owner_repositories(ActionsTarget::Organization(&outside)).await, Err(ServiceError::Forbidden)));
    let owner = fixture.services.orgs.open(Some(&fixture.bob), "acme").await.unwrap();
    assert!(actions.owner_repositories(ActionsTarget::Organization(&owner)).await.unwrap().is_empty(), "acme has no repositories");
}
