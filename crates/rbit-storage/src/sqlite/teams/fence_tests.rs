//! Four direct team membership/grant APIs only, with actual permission effects.
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
    org: AccountId,
    alice: AccountId,
    bob: AccountId,
    carol: AccountId,
    outsider: AccountId,
    team: TeamId,
    repo: RepoId,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        let mut users = Vec::new();
        for name in ["alice", "bob", "carol", "outsider"] {
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
        let (alice, bob, carol, outsider) = (users[0], users[1], users[2], users[3]);
        let org = db
            .orgs()
            .create(NewOrganization { name: "acme".parse().unwrap(), display_name: None, description: None, creator: alice })
            .await
            .unwrap()
            .account
            .id;
        db.orgs().add_member(org, bob, OrgRole::Member).await.unwrap();
        db.orgs().add_member(org, carol, OrgRole::Member).await.unwrap();
        let team = db.teams().create(org, &"core".parse().unwrap(), None).await.unwrap().id;
        db.teams().add_member(team, bob).await.unwrap();
        let repo = new_repo(&db, org).await;
        Self { _directory: directory, db, org, alice, bob, carol, outsider, team, repo }
    }
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
#[derive(Debug, Clone, Copy)]
enum Operation {
    Add,
    Remove,
    Set,
    Ungrant,
}
const OPERATIONS: [Operation; 4] = [Operation::Add, Operation::Remove, Operation::Set, Operation::Ungrant];
async fn apply(f: &Fixture, store: &TeamStore, operation: Operation) -> Result<(), StorageError> {
    match operation {
        Operation::Add => store.add_member(f.team, f.carol).await.map(|_| ()),
        Operation::Remove => store.remove_member(f.team, f.bob).await.map(|_| ()),
        Operation::Set => store.set_repository(f.team, f.repo, Permission::Admin).await.map(|_| ()),
        Operation::Ungrant => store.remove_repository(f.team, f.repo).await.map(|_| ()),
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
async fn acknowledged_changes_noops_and_same_org_refusals_settle_cleanly() {
    let f = Fixture::new().await;
    let store = f.db.teams();
    assert_eq!(store.add_member(f.team, f.alice).await.unwrap(), TeamChange::Done);
    assert_eq!(store.add_member(f.team, f.outsider).await.unwrap(), TeamChange::OutsideOrganization);
    assert_eq!(store.add_member(f.team, AccountId(i64::MAX)).await.unwrap(), TeamChange::OutsideOrganization);
    assert_eq!(store.add_member(TeamId(i64::MAX), f.bob).await.unwrap(), TeamChange::OutsideOrganization);
    assert!(!store.remove_member(f.team, f.carol).await.unwrap());
    assert!(!store.remove_member(f.team, AccountId(i64::MAX)).await.unwrap());
    assert!(!store.remove_member(TeamId(i64::MAX), f.bob).await.unwrap());
    assert!(store.remove_member(f.team, f.bob).await.unwrap());
    assert!(!store.remove_member(f.team, f.bob).await.unwrap());
    for permission in Permission::ALL {
        assert_eq!(store.set_repository(f.team, f.repo, permission).await.unwrap(), TeamChange::Done);
        assert_eq!(store.set_repository(f.team, f.repo, permission).await.unwrap(), TeamChange::Done);
        assert_eq!(store.grants_for_repository(f.repo).await.unwrap()[0].permission, permission);
    }
    let personal = new_repo(&f.db, f.alice).await;
    let foreign =
        f.db.orgs()
            .create(NewOrganization { name: "other".parse().unwrap(), display_name: None, description: None, creator: f.alice })
            .await
            .unwrap()
            .account
            .id;
    let foreign_repo = new_repo(&f.db, foreign).await;
    for repo in [personal, foreign_repo, RepoId(i64::MAX)] {
        assert_eq!(store.set_repository(f.team, repo, Permission::Write).await.unwrap(), TeamChange::OutsideOrganization);
    }
    assert_eq!(store.set_repository(TeamId(i64::MAX), f.repo, Permission::Write).await.unwrap(), TeamChange::OutsideOrganization);
    assert!(!store.remove_repository(TeamId(i64::MAX), f.repo).await.unwrap());
    assert!(!store.remove_repository(f.team, personal).await.unwrap());
    assert!(!store.remove_repository(f.team, RepoId(i64::MAX)).await.unwrap());
    assert!(store.remove_repository(f.team, f.repo).await.unwrap());
    assert!(!store.remove_repository(f.team, f.repo).await.unwrap());
    drop(timeout(WAIT, f.db.authority_domain.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn membership_and_repository_grants_have_distinct_real_permission_effects() {
    let f = Fixture::new().await;
    let store = f.db.teams();
    // Protection team attribution depends on membership, even with no explicit repo grant.
    assert!(store.grants_for_repository(f.repo).await.unwrap().is_empty());
    assert_eq!(f.db.branch_protection().teams_of(f.repo, f.bob).await.unwrap(), [f.team]);
    store.add_member(f.team, f.carol).await.unwrap();
    store.set_repository(f.team, f.repo, Permission::Write).await.unwrap();
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.bob).await.unwrap().team, Some(Permission::Write));
    assert!(store.remove_member(f.team, f.bob).await.unwrap());
    assert_eq!(f.db.branch_protection().teams_of(f.repo, f.bob).await.unwrap(), Vec::<TeamId>::new());
    assert_eq!(store.grants_for_repository(f.repo).await.unwrap()[0].permission, Permission::Write);
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.bob).await.unwrap().team, None);
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.carol).await.unwrap().team, Some(Permission::Write));
    let other = store.create(f.org, &"docs".parse().unwrap(), None).await.unwrap().id;
    store.add_member(other, f.carol).await.unwrap();
    store.set_repository(other, f.repo, Permission::Read).await.unwrap();
    assert!(store.remove_repository(f.team, f.repo).await.unwrap());
    assert!(store.is_member(f.team, f.carol).await.unwrap());
    assert_eq!(f.db.repositories().grants(f.repo, f.org, f.carol).await.unwrap().team, Some(Permission::Read));
    assert_eq!(f.db.branch_protection().teams_of(f.repo, f.carol).await.unwrap(), [f.team, other]);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn duplicate_keeps_business_error_and_unknown_state_without_blocking_later_writes() {
    let f = Fixture::new().await;
    let store = f.db.teams();
    assert!(matches!(store.add_member(f.team, f.bob).await, Err(StorageError::Conflict("team member"))));
    assert_eq!(store.members(f.team).await.unwrap().iter().map(|user| user.id).collect::<Vec<_>>(), [f.bob]);
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(store.add_member(f.team, f.carol).await.unwrap(), TeamChange::Done);
    assert_eq!(store.set_repository(f.team, f.repo, Permission::Admin).await.unwrap(), TeamChange::Done);
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
}

#[tokio::test]
async fn four_entries_share_policy_queue_while_reads_and_queued_cancel_stay_clean() {
    let f = Fixture::new().await;
    let store = f.db.teams();
    let policy = f.db.plugin_policy();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut prepare = Box::pin(policy.prepare(f.alice, Timestamp(1)));
    pending(prepare.as_mut()).await;
    let mut add = Box::pin(apply(&f, &store, Operation::Add));
    let mut set = Box::pin(apply(&f, &store, Operation::Set));
    let mut remove = Box::pin(apply(&f, &store, Operation::Remove));
    let mut ungrant = Box::pin(apply(&f, &store, Operation::Ungrant));
    let mut canceled = Box::pin(apply(&f, &store, Operation::Add));
    pending(add.as_mut()).await;
    pending(set.as_mut()).await;
    pending(remove.as_mut()).await;
    pending(ungrant.as_mut()).await;
    pending(canceled.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 6);
    assert!(timeout(WAIT, store.is_member(f.team, f.bob)).await.unwrap().unwrap());
    assert!(timeout(WAIT, store.grants_for_repository(f.repo)).await.unwrap().unwrap().is_empty());
    drop(canceled);
    assert_eq!(f.db.authority_domain.snapshot().queued, 5);
    drop(quiet);
    timeout(WAIT, prepare).await.unwrap().unwrap();
    timeout(WAIT, add).await.unwrap().unwrap();
    timeout(WAIT, set).await.unwrap().unwrap();
    timeout(WAIT, remove).await.unwrap().unwrap();
    timeout(WAIT, ungrant).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
    assert!(store.is_member(f.team, f.carol).await.unwrap());
    assert!(!store.is_member(f.team, f.bob).await.unwrap());
    assert!(store.grants_for_repository(f.repo).await.unwrap().is_empty());
}

#[tokio::test]
async fn finite_domain_capacity_refuses_each_entry_before_any_sql() {
    let f = Fixture::new().await;
    let store = f.db.teams();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut queued = Vec::new();
    for _ in 0..128 {
        let mut operation = Box::pin(apply(&f, &store, Operation::Add));
        pending(operation.as_mut()).await;
        queued.push(operation);
    }
    assert_eq!(f.db.authority_domain.snapshot().queued, 128);
    for operation in OPERATIONS {
        assert!(matches!(
            timeout(WAIT, apply(&f, &store, operation)).await.unwrap(),
            Err(StorageError::AdmissionUnavailable { scope: "team", reason: AdmissionReason::Capacity })
        ));
    }
    assert!(store.is_member(f.team, f.bob).await.unwrap());
    assert!(!store.is_member(f.team, f.carol).await.unwrap());
    assert!(store.grants_for_repository(f.repo).await.unwrap().is_empty());
    drop(queued);
    assert_eq!(f.db.authority_domain.snapshot().queued, 0);
    drop(quiet);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn old_cloned_and_fresh_handles_reject_all_four_after_unpolled_close() {
    let f = Fixture::new().await;
    let database = f.db.clone();
    let old = f.db.teams().clone();
    drop(database.close()); // Writer stop is synchronous; pool closure is not polled.
    for fresh in [false, true] {
        let store = if fresh { database.teams() } else { old.clone() };
        for operation in OPERATIONS {
            assert!(matches!(
                apply(&f, &store, operation).await,
                Err(StorageError::AdmissionUnavailable { scope: "team", reason: AdmissionReason::Stopped })
            ));
        }
    }
    assert!(old.is_member(f.team, f.bob).await.unwrap());
    assert!(!old.is_member(f.team, f.carol).await.unwrap());
    assert!(old.grants_for_repository(f.repo).await.unwrap().is_empty());
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn acquired_entries_can_finish_after_terminal_stop() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.teams();
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
async fn acquired_loss_is_unknown_for_each_entry_while_sql_writer_is_held() {
    for kind in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.teams();
        let mut operation = Box::pin(apply(&f, &store, kind));
        pending(operation.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "enrollment before SQL await, not worker-entry proof");
        drop(operation);
        assert!(f.db.authority_domain.snapshot().uncertain);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}
