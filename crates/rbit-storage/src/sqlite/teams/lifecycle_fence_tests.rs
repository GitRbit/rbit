//! Remaining `TeamStore` lifecycle entries; the accepted Team4 tests stay separate.
use super::*;
use crate::{
    AdmissionReason, Database, NewRepository, NewUser,
    sqlite::{orgs::NewOrganization, protection::RuleInput},
};
use rbit_core::{
    OrgRole, Visibility,
    protection::{BranchPattern, ProtectionId, RuleSettings},
    status::StatusContext,
};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::{Instant, timeout};

const WAIT: Duration = Duration::from_secs(5);
struct Fixture {
    _directory: tempfile::TempDir,
    db: Database,
    org: AccountId,
    alice: AccountId,
    bob: AccountId,
    carol: AccountId,
    team: TeamId,
    repo: RepoId,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let mut users = Vec::new();
        for name in ["alice", "bob", "carol"] {
            users.push(
                db.accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "fixture".into(),
                        is_admin: true,
                    })
                    .await
                    .unwrap()
                    .id(),
            );
        }
        let (alice, bob, carol) = (users[0], users[1], users[2]);
        let org = new_org(&db, "acme", alice).await;
        for user in [bob, carol] {
            db.orgs().add_member(org, user, OrgRole::Member).await.unwrap();
        }
        let team = db.teams().create(org, &"core".parse().unwrap(), Some("About")).await.unwrap().id;
        db.teams().add_member(team, bob).await.unwrap();
        let repo = db
            .repositories()
            .create(NewRepository {
                owner_id: org,
                name: "private".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".into(),
            })
            .await
            .unwrap()
            .id;
        db.teams().set_repository(team, repo, Permission::Write).await.unwrap();
        Self { _directory: directory, db, org, alice, bob, carol, team, repo }
    }
    async fn team(&self) -> Team {
        self.db.teams().find_by_name(self.org, "core").await.unwrap().unwrap()
    }
}
async fn new_org(db: &Database, name: &str, creator: AccountId) -> AccountId {
    db.orgs()
        .create(NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator })
        .await
        .unwrap()
        .account
        .id
}
fn rule_input(f: &Fixture, pattern: &str, teams: Vec<TeamId>) -> RuleInput {
    RuleInput {
        pattern: BranchPattern::parse(pattern).unwrap(),
        settings: RuleSettings { restrict_pushers: true, require_status_checks: true, ..RuleSettings::default() },
        users: vec![f.carol],
        teams,
        contexts: vec![StatusContext::parse("ci/build").unwrap()],
    }
}
async fn shared_graph(f: &Fixture) -> (TeamId, [ProtectionId; 2]) {
    let teams = f.db.teams();
    let other = teams.create(f.org, &"docs".parse().unwrap(), None).await.unwrap().id;
    teams.add_member(other, f.bob).await.unwrap();
    teams.set_repository(other, f.repo, Permission::Read).await.unwrap();
    f.db.collaborators().add(f.repo, f.carol, Permission::Admin, f.alice).await.unwrap();
    let mut rules = Vec::new();
    for pattern in ["main", "release/*"] {
        let result = f.db.branch_protection().create(f.repo, &rule_input(f, pattern, vec![f.team, other]), f.alice).await.unwrap();
        let crate::sqlite::protection::RuleChange::Saved(id) = result else { panic!("rule was not saved: {result:?}") };
        rules.push(id);
    }
    (other, [rules[0], rules[1]])
}
async fn clean(db: &Database) {
    drop(timeout(WAIT, db.authority_domain.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert!(!db.authority_domain.snapshot().uncertain);
}
#[derive(Debug, Clone, Copy)]
enum Operation {
    Create,
    Update,
    Delete,
}
const OPERATIONS: [Operation; 3] = [Operation::Create, Operation::Update, Operation::Delete];
async fn apply(f: &Fixture, teams: &TeamStore, operation: Operation) -> Result<(), StorageError> {
    match operation {
        Operation::Create => teams.create(f.org, &"fresh".parse().unwrap(), None).await.map(|_| ()),
        Operation::Update => teams.update(f.team, &"writers".parse().unwrap(), Some("Changed")).await,
        Operation::Delete => teams.delete(f.team).await,
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[test]
fn invalid_returned_team_row_is_a_conversion_error_only() {
    // This exercises conversion itself, not a fabricated committed SQL failure.
    let row = TeamRow { id: 1, org_id: 2, name: "new".into(), description: None, created_at: 3 };
    assert!(matches!(Team::try_from(row), Err(StorageError::Corrupt(message)) if message.contains("team name")));
}

#[tokio::test]
async fn lifecycle_results_preserve_identity_case_description_and_missing_unit_noops() {
    let f = Fixture::new().await;
    let teams = f.db.teams();
    let (other, rules) = shared_graph(&f).await;
    let original = f.team().await;
    let created = teams.create(f.org, &"spare".parse().unwrap(), Some("New")).await.unwrap();
    assert_eq!(created.org_id, f.org);
    assert_eq!(created.name.as_str(), "spare");
    assert_eq!(created.description.as_deref(), Some("New"));
    let stored = teams.find_by_name(f.org, "SPARE").await.unwrap().unwrap();
    assert_eq!((stored.id, stored.created_at), (created.id, created.created_at));
    let foreign = new_org(&f.db, "other", f.alice).await;
    teams.create(foreign, &"spare".parse().unwrap(), None).await.unwrap();
    teams.update(f.team, &"Core".parse().unwrap(), Some("Case")).await.unwrap();
    let same_case = teams.find_by_name(f.org, "core").await.unwrap().unwrap();
    assert_eq!((same_case.id, same_case.created_at), (original.id, original.created_at));
    assert_eq!(same_case.name.as_str(), "Core");
    teams.update(f.team, &"writers".parse().unwrap(), None).await.unwrap();
    teams.update(f.team, &"writers".parse().unwrap(), None).await.unwrap();
    let renamed = teams.find_by_name(f.org, "WRITERS").await.unwrap().unwrap();
    assert_eq!((renamed.id, renamed.created_at), (original.id, original.created_at));
    assert_eq!(renamed.description, None);
    assert!(teams.find_by_name(f.org, "core").await.unwrap().is_none(), "team rename does not add account-style redirects");
    assert!(teams.is_member(f.team, f.bob).await.unwrap());
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.bob).await.unwrap().team, Some(Permission::Write));
    for id in rules {
        let rule = f.db.branch_protection().get(f.repo, id).await.unwrap().unwrap();
        assert!(rule.rule.allowed_teams.contains(&f.team) && rule.rule.allowed_teams.contains(&other));
        assert_eq!(rule.teams.iter().find(|team| team.id == f.team).unwrap().name.as_str(), "writers");
    }
    teams.update(TeamId(i64::MAX), &"missing".parse().unwrap(), Some("Ignored")).await.unwrap();
    teams.delete(TeamId(i64::MAX)).await.unwrap();
    teams.delete(created.id).await.unwrap();
    teams.delete(created.id).await.unwrap();
    assert!(teams.find_by_name(f.org, "spare").await.unwrap().is_none());
    clean(&f.db).await;
}

#[tokio::test]
async fn selected_team_delete_settles_own_three_cascades_and_keeps_other_authority() {
    let f = Fixture::new().await;
    let (other, rules) = shared_graph(&f).await;
    let teams = f.db.teams();
    teams.delete(f.team).await.unwrap();
    teams.delete(f.team).await.unwrap();
    for query in [
        "SELECT COUNT(*) FROM team_members WHERE team_id = ?",
        "SELECT COUNT(*) FROM team_repositories WHERE team_id = ?",
        "SELECT COUNT(*) FROM branch_protection_teams WHERE team_id = ?",
    ] {
        let count: i64 = sqlx::query_scalar(query).bind(f.team.0).fetch_one(&f.db.pool).await.unwrap();
        assert_eq!(count, 0);
    }
    assert!(teams.find_by_name(f.org, "core").await.unwrap().is_none());
    assert!(teams.is_member(other, f.bob).await.unwrap());
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.bob).await.unwrap().team, Some(Permission::Read));
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.carol).await.unwrap().collaborator, Some(Permission::Admin));
    assert_eq!(f.db.branch_protection().teams_of(f.repo, f.bob).await.unwrap(), [other]);
    for id in rules {
        let rule = f.db.branch_protection().get(f.repo, id).await.unwrap().unwrap();
        assert!(rule.rule.settings.restrict_pushers);
        assert_eq!(rule.rule.allowed_teams, [other]);
        assert_eq!(rule.rule.allowed_users, [f.carol]);
        assert_eq!(rule.rule.required_contexts, [StatusContext::parse("ci/build").unwrap()]);
    }
    assert!(f.db.repositories().find_by_path("acme", "private").await.unwrap().is_some());
    clean(&f.db).await;
}

#[tokio::test]
async fn duplicate_and_foreign_key_errors_are_not_clean_but_later_writes_proceed() {
    for kind in 0..3 {
        let f = Fixture::new().await;
        let teams = f.db.teams();
        let original = f.team().await;
        let (result, target) = match kind {
            0 => (teams.create(f.org, &"CORE".parse().unwrap(), Some("Duplicate")).await.map(|_| ()), None),
            1 => {
                let other = teams.create(f.org, &"docs".parse().unwrap(), Some("Preserved")).await.unwrap();
                teams.add_member(other.id, f.carol).await.unwrap();
                teams.set_repository(other.id, f.repo, Permission::Read).await.unwrap();
                let change = f.db.branch_protection().create(f.repo, &rule_input(&f, "docs-only", vec![other.id]), f.alice).await.unwrap();
                let crate::sqlite::protection::RuleChange::Saved(id) = change else { panic!("rule not saved") };
                (teams.update(other.id, &"CORE".parse().unwrap(), Some("Duplicate")).await, Some((other, id)))
            }
            _ => (teams.create(AccountId(i64::MAX), &"bad-owner".parse().unwrap(), None).await.map(|_| ()), None),
        };
        if kind < 2 {
            assert!(matches!(result, Err(StorageError::Conflict("team name"))));
        } else {
            assert!(matches!(result, Err(StorageError::Sqlite(_))));
        }
        let current = f.team().await;
        assert_eq!(
            (current.id, current.name, current.description, current.created_at),
            (original.id, original.name, original.description, original.created_at)
        );
        if let Some((before, rule)) = target {
            let after = teams.find_by_name(f.org, "docs").await.unwrap().unwrap();
            assert_eq!(
                (after.id, after.name, after.description, after.created_at),
                (before.id, before.name, before.description, before.created_at)
            );
            assert!(teams.is_member(before.id, f.carol).await.unwrap());
            assert_eq!(
                teams.grants_for_repository(f.repo).await.unwrap().iter().find(|grant| grant.team.id == before.id).unwrap().permission,
                Permission::Read
            );
            assert_eq!(f.db.branch_protection().get(f.repo, rule).await.unwrap().unwrap().rule.allowed_teams, [before.id]);
        }
        assert!(teams.is_member(f.team, f.bob).await.unwrap());
        assert!(f.db.authority_domain.snapshot().uncertain);
        let fresh = teams.create(f.org, &"later".parse().unwrap(), None).await.unwrap();
        teams.update(fresh.id, &"later".parse().unwrap(), Some("Allowed")).await.unwrap();
        teams.delete(fresh.id).await.unwrap();
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn lifecycle_shares_membership_and_protection_fifo_without_guarding_reads() {
    let f = Fixture::new().await;
    let teams = f.db.teams();
    let protection = f.db.branch_protection();
    let rule = rule_input(&f, "main", vec![f.team]);
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut membership = Box::pin(teams.add_member(f.team, f.carol));
    let mut create_rule = Box::pin(protection.create(f.repo, &rule, f.alice));
    let mut create = Box::pin(apply(&f, &teams, Operation::Create));
    let mut update = Box::pin(apply(&f, &teams, Operation::Update));
    let mut delete = Box::pin(apply(&f, &teams, Operation::Delete));
    let mut canceled = Box::pin(apply(&f, &teams, Operation::Create));
    pending(membership.as_mut()).await;
    pending(create_rule.as_mut()).await;
    pending(create.as_mut()).await;
    pending(update.as_mut()).await;
    pending(delete.as_mut()).await;
    pending(canceled.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 6);
    assert_eq!(timeout(WAIT, teams.find_by_name(f.org, "core")).await.unwrap().unwrap().unwrap().id, f.team);
    assert_eq!(timeout(WAIT, teams.list(f.org)).await.unwrap().unwrap().len(), 1);
    drop(canceled);
    drop(quiet);
    assert_eq!(timeout(WAIT, membership).await.unwrap().unwrap(), TeamChange::Done);
    assert!(teams.is_member(f.team, f.carol).await.unwrap());
    let crate::sqlite::protection::RuleChange::Saved(id) = timeout(WAIT, create_rule).await.unwrap().unwrap() else {
        panic!("rule not saved")
    };
    assert_eq!(protection.get(f.repo, id).await.unwrap().unwrap().rule.allowed_teams, [f.team]);
    timeout(WAIT, create).await.unwrap().unwrap();
    timeout(WAIT, update).await.unwrap().unwrap();
    timeout(WAIT, delete).await.unwrap().unwrap();
    assert!(teams.find_by_name(f.org, "fresh").await.unwrap().is_some());
    assert_eq!(f.db.branch_protection().get(f.repo, id).await.unwrap().unwrap().rule.allowed_teams, Vec::<TeamId>::new());
    clean(&f.db).await;
}

#[tokio::test]
async fn finite_capacity_and_unpolled_close_preserve_typed_lifecycle_refusals() {
    let f = Fixture::new().await;
    let teams = f.db.teams();
    let original = f.team().await;
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut queued = Vec::new();
    for _ in 0..128 {
        let mut operation = Box::pin(apply(&f, &teams, Operation::Create));
        pending(operation.as_mut()).await;
        queued.push(operation);
    }
    for kind in OPERATIONS {
        assert!(matches!(
            timeout(WAIT, apply(&f, &teams, kind)).await.unwrap(),
            Err(StorageError::AdmissionUnavailable { scope: "team", reason: AdmissionReason::Capacity })
        ));
    }
    drop(queued);
    drop(quiet);
    clean(&f.db).await;
    let database = f.db.clone();
    drop(database.close());
    for fresh in [false, true] {
        let handle = if fresh { database.teams() } else { teams.clone() };
        for kind in OPERATIONS {
            assert!(matches!(
                apply(&f, &handle, kind).await,
                Err(StorageError::AdmissionUnavailable { scope: "team", reason: AdmissionReason::Stopped })
            ));
        }
        assert!(matches!(
            handle.add_member(f.team, f.carol).await,
            Err(StorageError::AdmissionUnavailable { scope: "team", reason: AdmissionReason::Stopped })
        ));
    }
    let current = f.team().await;
    assert_eq!((current.id, current.name, current.description), (original.id, original.name, original.description));
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn acquired_lifecycle_can_finish_after_stop() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let teams = f.db.teams();
        let mut operation = Box::pin(apply(&f, &teams, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1);
        drop(f.db.authority_domain.stop_writers());
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        timeout(WAIT, operation).await.unwrap().unwrap();
        timeout(WAIT, f.db.authority_domain.stop_writers().drain(Instant::now() + WAIT)).await.unwrap().unwrap();
        assert!(!f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn acquired_lifecycle_loss_under_sql_writer_is_uncertain() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let teams = f.db.teams();
        let mut operation = Box::pin(apply(&f, &teams, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "enrollment only, not SQLite worker-entry proof");
        drop(operation);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}
