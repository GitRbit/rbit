//! Direct rule-graph authority only; external cascade/transfer origins stay separate.
use super::*;
use crate::{AdmissionReason, Database, NewRepository, NewUser, sqlite::orgs::NewOrganization};
use rbit_core::{OrgRole, Visibility};
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
    alice: AccountId,
    bob: AccountId,
    repo: RepoId,
    team: TeamId,
    foreign: TeamId,
    rule: ProtectionId,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let mut users = Vec::new();
        for name in ["alice", "bob"] {
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
        let (alice, bob) = (users[0], users[1]);
        let org = new_org(&db, "acme", alice).await;
        let other = new_org(&db, "other", alice).await;
        db.orgs().add_member(org, bob, OrgRole::Member).await.unwrap();
        let team = db.teams().create(org, &"core".parse().unwrap(), None).await.unwrap().id;
        let foreign = db.teams().create(other, &"core".parse().unwrap(), None).await.unwrap().id;
        let repo = new_repo(&db, org).await;
        let mut fixture = Self { _directory: directory, db, alice, bob, repo, team, foreign, rule: ProtectionId(0) };
        fixture.rule = saved(fixture.db.branch_protection().create(repo, &full(&fixture, "main"), alice).await.unwrap());
        fixture
    }
    async fn graph(&self) -> BranchProtection {
        self.db.branch_protection().get(self.repo, self.rule).await.unwrap().unwrap()
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
async fn new_repo(db: &Database, owner: AccountId) -> RepoId {
    db.repositories()
        .create(NewRepository {
            owner_id: owner,
            name: "private".parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".into(),
        })
        .await
        .unwrap()
        .id
}
fn input(pattern: &str) -> RuleInput {
    RuleInput {
        pattern: BranchPattern::parse(pattern).unwrap(),
        settings: RuleSettings::default(),
        users: Vec::new(),
        teams: Vec::new(),
        contexts: Vec::new(),
    }
}
fn full(f: &Fixture, pattern: &str) -> RuleInput {
    RuleInput {
        settings: RuleSettings {
            deny_force_push: false,
            deny_deletion: false,
            require_pull_request: true,
            required_approvals: 2,
            dismiss_stale_approvals: true,
            block_on_changes_requested: false,
            restrict_pushers: true,
            allow_admin_bypass: true,
            require_status_checks: true,
        },
        users: vec![f.bob, f.bob],
        teams: vec![f.team, f.team],
        contexts: vec![StatusContext::parse("ci/build").unwrap(), StatusContext::parse("ci/build").unwrap()],
        ..input(pattern)
    }
}
fn saved(outcome: RuleChange) -> ProtectionId {
    match outcome {
        RuleChange::Saved(id) => id,
        other => panic!("expected Saved, got {other:?}"),
    }
}
fn same_graph(actual: &BranchProtection, before: &BranchProtection) {
    assert_eq!(actual.id, before.id);
    assert_eq!(actual.repo_id, before.repo_id);
    assert_eq!(actual.rule, before.rule);
    assert_eq!(actual.created_by, before.created_by);
    assert_eq!(actual.created_at, before.created_at);
    assert_eq!(actual.updated_at, before.updated_at);
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
async fn apply(f: &Fixture, store: &BranchProtectionStore, operation: Operation) -> Result<(), StorageError> {
    match operation {
        Operation::Create => store.create(f.repo, &input("new"), f.alice).await.map(|_| ()),
        Operation::Update => store.update(f.repo, f.rule, &input("changed")).await.map(|_| ()),
        Operation::Delete => store.delete(f.repo, f.rule).await.map(|_| ()),
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn acknowledged_graph_replacement_scoped_noops_and_delete_cascades_are_clean() {
    let f = Fixture::new().await;
    let store = f.db.branch_protection();
    let before = f.graph().await;
    assert_eq!(before.created_by, Some(f.alice));
    assert_eq!(before.rule.settings, full(&f, "main").settings);
    assert_eq!(before.rule.allowed_users, [f.bob]);
    assert_eq!(before.rule.allowed_teams, [f.team]);
    assert_eq!(before.rule.required_contexts, [StatusContext::parse("ci/build").unwrap()]);
    let mut change = full(&f, "release/*");
    change.users = vec![f.alice];
    change.contexts = vec![StatusContext::parse("deploy").unwrap()];
    change.settings.allow_admin_bypass = false;
    assert_eq!(store.update(f.repo, f.rule, &change).await.unwrap(), RuleChange::Saved(f.rule));
    assert_eq!(store.update(f.repo, f.rule, &change).await.unwrap(), RuleChange::Saved(f.rule));
    let updated = f.graph().await;
    assert_eq!(updated.rule.pattern, change.pattern);
    assert_eq!(updated.rule.settings, change.settings);
    assert_eq!(updated.rule.allowed_users, [f.alice]);
    assert_eq!(updated.rule.allowed_teams, [f.team]);
    assert!(updated.updated_at >= updated.created_at);
    assert_eq!(updated.rule.required_contexts, change.contexts);
    assert_eq!(updated.created_by, before.created_by);
    assert_eq!(updated.created_at, before.created_at);
    let personal = new_repo(&f.db, f.alice).await;
    assert_eq!(store.update(personal, f.rule, &input("wrong")).await.unwrap(), RuleChange::NotFound);
    assert_eq!(store.update(f.repo, ProtectionId(i64::MAX), &input("missing")).await.unwrap(), RuleChange::NotFound);
    assert!(!store.delete(personal, f.rule).await.unwrap());
    same_graph(&f.graph().await, &updated);
    assert!(store.delete(f.repo, f.rule).await.unwrap());
    assert!(!store.delete(f.repo, f.rule).await.unwrap());
    assert!(!store.delete(f.repo, ProtectionId(i64::MAX)).await.unwrap());
    for query in [
        "SELECT COUNT(*) FROM branch_protection_users WHERE rule_id = ?",
        "SELECT COUNT(*) FROM branch_protection_teams WHERE rule_id = ?",
        "SELECT COUNT(*) FROM branch_protection_required_checks WHERE rule_id = ?",
    ] {
        let count: i64 = sqlx::query_scalar(query).bind(f.rule.0).fetch_one(&f.db.pool).await.unwrap();
        assert_eq!(count, 0);
    }
    let fresh = saved(store.create(f.repo, &input("fresh"), f.alice).await.unwrap());
    let rule = store.get(f.repo, fresh).await.unwrap().unwrap().rule;
    assert_eq!(rule.allowed_users, Vec::<AccountId>::new());
    assert_eq!(rule.allowed_teams, Vec::<TeamId>::new());
    assert_eq!(rule.required_contexts, Vec::<StatusContext>::new());
    clean(&f.db).await;
}

#[tokio::test]
async fn mixed_team_refusals_await_rollback_of_every_tentative_graph_write() {
    let f = Fixture::new().await;
    let store = f.db.branch_protection();
    let before = f.graph().await;
    for invalid in [f.foreign, TeamId(i64::MAX)] {
        let mut rejected = full(&f, "tentative");
        rejected.users = vec![f.alice];
        rejected.contexts = vec![StatusContext::parse("tentative/check").unwrap()];
        rejected.teams = vec![f.team, invalid]; // Context, user and valid team were attempted first.
        assert_eq!(store.create(f.repo, &rejected, f.alice).await.unwrap(), RuleChange::TeamOutsideOrganization);
        assert_eq!(store.list(f.repo).await.unwrap().len(), 1);
        assert_eq!(store.update(f.repo, f.rule, &rejected).await.unwrap(), RuleChange::TeamOutsideOrganization);
        same_graph(&f.graph().await, &before);
        clean(&f.db).await;
    }
    let personal = new_repo(&f.db, f.alice).await;
    assert_eq!(store.create(personal, &full(&f, "main"), f.alice).await.unwrap(), RuleChange::TeamOutsideOrganization);
    assert_eq!(store.list(personal).await.unwrap().into_iter().map(|rule| rule.id).collect::<Vec<_>>(), Vec::<ProtectionId>::new());
    clean(&f.db).await;
}

#[tokio::test]
async fn concurrent_real_last_slot_preserves_cap_and_full_cap_refusal_precedence() {
    let f = Fixture::new().await;
    let store = f.db.branch_protection();
    for number in 1..(MAX_RULES - 1) {
        saved(store.create(f.repo, &input(&format!("branch-{number}")), f.alice).await.unwrap());
    }
    assert_eq!(store.list(f.repo).await.unwrap().len(), MAX_RULES - 1);
    let first = input("last-a");
    let second = input("last-b");
    let (first, second) =
        timeout(WAIT, async { tokio::join!(store.create(f.repo, &first, f.alice), store.create(f.repo, &second, f.alice)) }).await.unwrap();
    let outcomes = [first.unwrap(), second.unwrap()];
    assert_eq!(outcomes.iter().filter(|result| matches!(result, RuleChange::Saved(_))).count(), 1);
    assert_eq!(outcomes.iter().filter(|result| **result == RuleChange::TooManyRules).count(), 1);
    assert_eq!(store.list(f.repo).await.unwrap().len(), MAX_RULES);
    // The existing INSERT SELECT cap refuses even a duplicate pattern when already full.
    assert_eq!(store.create(f.repo, &input("main"), f.alice).await.unwrap(), RuleChange::TooManyRules);
    clean(&f.db).await;
}

#[tokio::test]
async fn duplicate_and_foreign_key_errors_keep_graph_and_sticky_uncertainty() {
    for duplicate in [true, false] {
        let f = Fixture::new().await;
        let store = f.db.branch_protection();
        let before = f.graph().await;
        let result = if duplicate {
            saved(store.create(f.repo, &input("other"), f.alice).await.unwrap());
            store.update(f.repo, f.rule, &input("other")).await
        } else {
            // Deliberately invalid trusted-storage fixture input, not accepted service input.
            let mut rejected = full(&f, "bad-fk");
            rejected.users = vec![AccountId(i64::MAX)];
            store.update(f.repo, f.rule, &rejected).await
        };
        if duplicate {
            assert!(matches!(result, Err(StorageError::Conflict("pattern"))));
        } else {
            assert!(matches!(result, Err(StorageError::Sqlite(_))));
        }
        same_graph(&f.graph().await, &before);
        assert!(f.db.authority_domain.snapshot().uncertain);
        saved(store.create(f.repo, &input("later"), f.alice).await.unwrap());
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn three_entries_share_policy_and_team_queue_without_serializing_reads() {
    let f = Fixture::new().await;
    let store = f.db.branch_protection();
    let policy = f.db.plugin_policy();
    let teams = f.db.teams();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut prepare = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    let mut membership = Box::pin(teams.add_member(f.team, f.bob));
    let mut create = Box::pin(apply(&f, &store, Operation::Create));
    let mut update = Box::pin(apply(&f, &store, Operation::Update));
    let mut delete = Box::pin(apply(&f, &store, Operation::Delete));
    let mut canceled = Box::pin(apply(&f, &store, Operation::Create));
    pending(prepare.as_mut()).await;
    pending(membership.as_mut()).await;
    pending(create.as_mut()).await;
    pending(update.as_mut()).await;
    pending(delete.as_mut()).await;
    pending(canceled.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 6);
    assert_eq!(timeout(WAIT, store.list(f.repo)).await.unwrap().unwrap().len(), 1);
    assert!(timeout(WAIT, store.get(f.repo, f.rule)).await.unwrap().unwrap().is_some());
    assert_eq!(timeout(WAIT, store.teams_of(f.repo, f.bob)).await.unwrap().unwrap(), Vec::<TeamId>::new());
    drop(canceled);
    drop(quiet);
    timeout(WAIT, prepare).await.unwrap().unwrap();
    timeout(WAIT, membership).await.unwrap().unwrap();
    timeout(WAIT, create).await.unwrap().unwrap();
    timeout(WAIT, update).await.unwrap().unwrap();
    timeout(WAIT, delete).await.unwrap().unwrap();
    assert_eq!(store.list(f.repo).await.unwrap().len(), 1);
    assert!(teams.is_member(f.team, f.bob).await.unwrap());
    assert_eq!(store.teams_of(f.repo, f.bob).await.unwrap(), [f.team]);
    clean(&f.db).await;
}

#[tokio::test]
async fn finite_capacity_and_unpolled_close_refuse_each_entry_with_typed_reasons() {
    let f = Fixture::new().await;
    let store = f.db.branch_protection();
    let before = f.graph().await;
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut queued = Vec::new();
    for _ in 0..128 {
        let mut operation = Box::pin(apply(&f, &store, Operation::Create));
        pending(operation.as_mut()).await;
        queued.push(operation);
    }
    for kind in OPERATIONS {
        assert!(matches!(
            timeout(WAIT, apply(&f, &store, kind)).await.unwrap(),
            Err(StorageError::AdmissionUnavailable { scope: "branch protection", reason: AdmissionReason::Capacity })
        ));
    }
    drop(queued);
    drop(quiet);
    clean(&f.db).await;
    let database = f.db.clone();
    drop(database.close());
    for fresh in [false, true] {
        let handle = if fresh { database.branch_protection() } else { store.clone() };
        for kind in OPERATIONS {
            assert!(matches!(
                apply(&f, &handle, kind).await,
                Err(StorageError::AdmissionUnavailable { scope: "branch protection", reason: AdmissionReason::Stopped })
            ));
        }
    }
    same_graph(&f.graph().await, &before);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn acquired_operations_can_finish_after_stop() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.branch_protection();
        let mut operation = Box::pin(apply(&f, &store, kind));
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
async fn acquired_loss_under_real_sql_writer_keeps_settlement_uncertain() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.branch_protection();
        let mut operation = Box::pin(apply(&f, &store, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "enrolled before SQL await, not worker-entry proof");
        drop(operation);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}
