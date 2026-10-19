//! Branch protection rules against a real, migrated `SQLite` database.

use rbit_core::{
    AccountId, RepoId, User, Visibility,
    membership::{TeamId, TeamName},
    protection::{BranchPattern, MAX_RULES, ProtectionId, RuleSettings},
    status::StatusContext,
};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{
        orgs::NewOrganization,
        protection::{RuleChange, RuleInput},
    },
};

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rbit.db")).await.unwrap()
}

async fn create_user(database: &Database, name: &str) -> User {
    let new = NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    database.accounts().create_user(new).await.unwrap()
}

async fn create_org(database: &Database, name: &str, creator: &User) -> AccountId {
    let new = NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator: creator.id() };
    database.orgs().create(new).await.unwrap().account.id
}

async fn create_repo(database: &Database, owner: AccountId, name: &str) -> RepoId {
    let new = NewRepository {
        owner_id: owner,
        name: name.parse().unwrap(),
        description: None,
        visibility: Visibility::Public,
        default_branch: "main".to_owned(),
    };
    database.repositories().create(new).await.unwrap().id
}

async fn create_team(database: &Database, org: AccountId, name: &str) -> TeamId {
    let name: TeamName = name.parse().unwrap();
    database.teams().create(org, &name, None).await.unwrap().id
}

fn input(pattern: &str) -> RuleInput {
    RuleInput {
        pattern: BranchPattern::parse(pattern).unwrap(),
        settings: RuleSettings::default(),
        users: vec![],
        teams: vec![],
        contexts: vec![],
    }
}

fn saved(change: RuleChange) -> ProtectionId {
    match change {
        RuleChange::Saved(id) => id,
        other => panic!("expected the rule to be saved, got {other:?}"),
    }
}

#[tokio::test]
async fn rules_are_stored_with_their_users_and_teams() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let zed = create_user(&database, "zed").await;
    let bob = create_user(&database, "bob").await;
    let acme = create_org(&database, "acme", &alice).await;
    let repo = create_repo(&database, acme, "rocket").await;
    let core = create_team(&database, acme, "core").await;
    let docs = create_team(&database, acme, "docs").await;
    let store = database.branch_protection();

    let settings = RuleSettings {
        require_pull_request: true,
        required_approvals: 2,
        dismiss_stale_approvals: true,
        restrict_pushers: true,
        allow_admin_bypass: true,
        ..RuleSettings::default()
    };
    let main = RuleInput { settings, users: vec![zed.id(), bob.id(), bob.id()], teams: vec![docs, core], ..input("main") };
    let main_id = saved(store.create(repo, &main, alice.id()).await.unwrap());
    let release = saved(store.create(repo, &input("release/**"), alice.id()).await.unwrap());

    let rules = store.list(repo).await.unwrap();
    let patterns: Vec<_> = rules.iter().map(|rule| rule.rule.pattern.as_str()).collect();
    assert_eq!(patterns, ["main", "release/**"], "by pattern");
    let stored = &rules[0];
    assert_eq!((stored.id, stored.repo_id, stored.created_by), (main_id, repo, Some(alice.id())));
    assert_eq!(stored.rule.settings, settings);
    let users: Vec<_> = stored.users.iter().map(|user| user.name.as_str()).collect();
    assert_eq!(users, ["bob", "zed"], "by name, without duplicates");
    assert_eq!(stored.rule.allowed_users, [bob.id(), zed.id()]);
    let teams: Vec<_> = stored.teams.iter().map(|team| team.name.as_str()).collect();
    assert_eq!(teams, ["core", "docs"]);
    assert_eq!(stored.rule.allowed_teams, [core, docs]);
    assert!(stored.created_at.seconds() > 0 && stored.updated_at == stored.created_at);
    assert_eq!(rules[1].rule.settings, RuleSettings::default());
    assert!(rules[1].users.is_empty() && rules[1].teams.is_empty());

    assert_eq!(store.get(repo, release).await.unwrap().unwrap().rule.pattern.as_str(), "release/**");
    let other = create_repo(&database, acme, "other").await;
    assert!(store.get(other, release).await.unwrap().is_none(), "rules belong to one repository");
    assert!(store.list(other).await.unwrap().is_empty());

    // Patterns are unique per repository, case-sensitively.
    assert!(matches!(store.create(repo, &input("main"), alice.id()).await, Err(StorageError::Conflict("pattern"))));
    saved(store.create(repo, &input("Main"), alice.id()).await.unwrap());
    saved(store.create(other, &input("main"), alice.id()).await.unwrap());

    // Deleting a user or a team removes it from the rule.
    database.teams().delete(docs).await.unwrap();
    let stored = store.get(repo, main_id).await.unwrap().unwrap();
    assert_eq!(stored.rule.allowed_teams, [core]);

    // Deleting the repository deletes its rules.
    database.repositories().delete(other).await.unwrap();
    assert!(store.list(other).await.unwrap().is_empty());
}

#[tokio::test]
async fn updates_replace_everything_and_deletes_are_scoped() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let acme = create_org(&database, "acme", &alice).await;
    let repo = create_repo(&database, acme, "rocket").await;
    let other = create_repo(&database, acme, "other").await;
    let core = create_team(&database, acme, "core").await;
    let store = database.branch_protection();

    let id =
        saved(store.create(repo, &RuleInput { users: vec![alice.id()], teams: vec![core], ..input("main") }, alice.id()).await.unwrap());
    saved(store.create(repo, &input("dev"), alice.id()).await.unwrap());

    let settings = RuleSettings { deny_force_push: false, restrict_pushers: true, ..RuleSettings::default() };
    let change = RuleInput { settings, users: vec![bob.id()], teams: vec![], ..input("trunk") };
    assert_eq!(store.update(repo, id, &change).await.unwrap(), RuleChange::Saved(id));
    let stored = store.get(repo, id).await.unwrap().unwrap();
    assert_eq!((stored.rule.pattern.as_str(), stored.rule.settings), ("trunk", settings));
    assert_eq!((stored.rule.allowed_users.as_slice(), stored.rule.allowed_teams.as_slice()), ([bob.id()].as_slice(), [].as_slice()));
    assert!(stored.updated_at >= stored.created_at);

    assert!(matches!(store.update(repo, id, &input("dev")).await, Err(StorageError::Conflict("pattern"))));
    assert_eq!(store.update(other, id, &input("x")).await.unwrap(), RuleChange::NotFound, "another repository's rule");
    assert_eq!(store.update(repo, ProtectionId(9999), &input("x")).await.unwrap(), RuleChange::NotFound);

    assert!(!store.delete(other, id).await.unwrap(), "scoped to the repository");
    assert!(store.delete(repo, id).await.unwrap());
    assert!(!store.delete(repo, id).await.unwrap());
    assert_eq!(store.list(repo).await.unwrap().len(), 1);
}

#[tokio::test]
async fn teams_must_belong_to_the_organisation_and_rules_are_limited() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let acme = create_org(&database, "acme", &alice).await;
    let rival = create_org(&database, "rival", &alice).await;
    let repo = create_repo(&database, acme, "rocket").await;
    let personal = create_repo(&database, alice.id(), "mine").await;
    let ours = create_team(&database, acme, "core").await;
    let theirs = create_team(&database, rival, "core").await;
    let store = database.branch_protection();

    let foreign = RuleInput { teams: vec![ours, theirs], ..input("main") };
    assert_eq!(store.create(repo, &foreign, alice.id()).await.unwrap(), RuleChange::TeamOutsideOrganization);
    assert!(store.list(repo).await.unwrap().is_empty(), "nothing was saved");
    assert_eq!(
        store.create(personal, &RuleInput { teams: vec![ours], ..input("main") }, alice.id()).await.unwrap(),
        RuleChange::TeamOutsideOrganization
    );
    let id = saved(store.create(repo, &RuleInput { teams: vec![ours], ..input("main") }, alice.id()).await.unwrap());
    assert_eq!(store.update(repo, id, &foreign).await.unwrap(), RuleChange::TeamOutsideOrganization);
    assert_eq!(store.get(repo, id).await.unwrap().unwrap().rule.allowed_teams, [ours], "the update was rolled back");

    for n in 1..MAX_RULES {
        saved(store.create(repo, &input(&format!("branch-{n}")), alice.id()).await.unwrap());
    }
    assert_eq!(store.create(repo, &input("one-too-many"), alice.id()).await.unwrap(), RuleChange::TooManyRules);
    assert_eq!(store.list(repo).await.unwrap().len(), MAX_RULES);
}

#[tokio::test]
async fn team_membership_and_transfers() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let bob = create_user(&database, "bob").await;
    let acme = create_org(&database, "acme", &alice).await;
    let rival = create_org(&database, "rival", &alice).await;
    let repo = create_repo(&database, acme, "rocket").await;
    let core = create_team(&database, acme, "core").await;
    let docs = create_team(&database, acme, "docs").await;
    let elsewhere = create_team(&database, rival, "core").await;
    let teams = database.teams();
    teams.add_member(core, alice.id()).await.unwrap();
    teams.add_member(docs, alice.id()).await.unwrap();
    teams.add_member(elsewhere, alice.id()).await.unwrap();
    let store = database.branch_protection();

    assert_eq!(store.teams_of(repo, alice.id()).await.unwrap(), [core, docs], "only teams of the repository's organisation");
    assert_eq!(store.teams_of(repo, bob.id()).await.unwrap(), Vec::<TeamId>::new());

    let id = saved(store.create(repo, &RuleInput { users: vec![bob.id()], teams: vec![core], ..input("main") }, alice.id()).await.unwrap());
    database.repositories().transfer(repo, rival).await.unwrap();
    let stored = store.get(repo, id).await.unwrap().unwrap();
    assert!(stored.rule.allowed_teams.is_empty(), "the former organisation's teams no longer count");
    assert_eq!(stored.rule.allowed_users, [bob.id()], "users stay");
    assert_eq!(store.teams_of(repo, alice.id()).await.unwrap(), [elsewhere]);
}

// Milestone 3: required status checks (S).
#[tokio::test]
async fn required_checks_are_stored_replaced_and_deleted_with_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let database = open_database(&dir).await;
    let alice = create_user(&database, "alice").await;
    let repo = create_repo(&database, alice.id(), "rocket").await;
    let store = database.branch_protection();
    let context = |name: &str| StatusContext::parse(name).unwrap();

    let settings = RuleSettings { require_status_checks: true, ..RuleSettings::default() };
    let contexts = vec![context("lint"), context("ci/build"), context("lint")];
    let id = saved(store.create(repo, &RuleInput { settings, contexts, ..input("main") }, alice.id()).await.unwrap());
    let stored = store.get(repo, id).await.unwrap().unwrap();
    assert!(stored.rule.settings.require_status_checks);
    assert_eq!(stored.rule.required_contexts, [context("ci/build"), context("lint")], "by name, without duplicates");

    let change = RuleInput { settings, contexts: vec![context("deploy")], ..input("main") };
    saved(store.update(repo, id, &change).await.unwrap());
    assert_eq!(store.get(repo, id).await.unwrap().unwrap().rule.required_contexts, [context("deploy")], "replaced");
    saved(store.update(repo, id, &input("main")).await.unwrap());
    let cleared = store.get(repo, id).await.unwrap().unwrap();
    assert!(!cleared.rule.settings.require_status_checks);
    assert_eq!(cleared.rule.required_contexts, Vec::<StatusContext>::new());

    // Deleting the rule deletes its contexts (and a new rule starts without them).
    saved(store.update(repo, id, &change).await.unwrap());
    assert!(store.delete(repo, id).await.unwrap());
    let fresh = saved(store.create(repo, &input("main"), alice.id()).await.unwrap());
    assert_eq!(store.get(repo, fresh).await.unwrap().unwrap().rule.required_contexts, Vec::<StatusContext>::new());
}
