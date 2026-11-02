//! Two direct organisation lifecycle entries; existing membership tests stay separate.
use super::*;
use crate::{
    AdmissionReason, Database, NewRepository, NewUser,
    sqlite::webhooks::{NewDelivery, NewWebhook},
};
use rbit_core::{
    Permission, RepoId, Visibility,
    webhook::{WebhookEvent, WebhookOwner, WebhookUrl},
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
}
impl Fixture {
    async fn new() -> Self {
        Self::open(false).await
    }
    async fn with_child_failure() -> Self {
        Self::open(true).await
    }
    async fn open(child_failure: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sql.db")).await.unwrap();
        if child_failure {
            // Install fixture schema before store activity; disarm with DML, not live DDL.
            sqlx::query("CREATE TABLE org_lifecycle_test_gate (armed INTEGER NOT NULL)").execute(&db.pool).await.unwrap();
            sqlx::query("INSERT INTO org_lifecycle_test_gate (armed) VALUES (1)").execute(&db.pool).await.unwrap();
            sqlx::query("CREATE TRIGGER org_lifecycle_failure BEFORE DELETE ON teams WHEN (SELECT armed FROM org_lifecycle_test_gate) = 1 BEGIN SELECT RAISE(ABORT, 'injected child failure'); END").execute(&db.pool).await.unwrap();
        }
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
        let org = db.orgs().create(input("acme", alice)).await.unwrap().account.id;
        Self { _directory: directory, db, org, alice, bob }
    }
    async fn repo(&self, owner: AccountId) -> RepoId {
        self.db
            .repositories()
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
}
fn input(name: &str, creator: AccountId) -> NewOrganization {
    NewOrganization { name: name.parse().unwrap(), display_name: Some("Display".into()), description: Some("About".into()), creator }
}
async fn clean(db: &Database) {
    drop(timeout(WAIT, db.authority_domain.drain(Instant::now() + WAIT)).await.unwrap().unwrap());
    assert!(!db.authority_domain.snapshot().uncertain);
}
#[derive(Debug, Clone, Copy)]
enum Operation {
    Create,
    Delete,
}
const OPERATIONS: [Operation; 2] = [Operation::Create, Operation::Delete];
async fn apply(f: &Fixture, store: &OrgStore, operation: Operation) -> Result<(), StorageError> {
    match operation {
        Operation::Create => store.create(input("fresh", f.alice)).await.map(|_| ()),
        Operation::Delete => store.delete_if_empty(f.org).await.map(|_| ()),
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>)
where
    F::Output: std::fmt::Debug,
{
    poll_fn(|cx| {
        let observed = future.as_mut().poll(cx);
        assert!(observed.is_pending(), "expected blocked future, got {observed:?}");
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn created_account_profile_and_owner_are_committed_with_typed_result() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    let org = store.create(input("rockets", f.bob)).await.unwrap();
    assert_eq!(org.account.kind, AccountKind::Organization);
    assert_eq!(org.account.name.as_str(), "rockets");
    assert_eq!(org.account.display_name.as_deref(), Some("Display"));
    assert_eq!(org.description.as_deref(), Some("About"));
    assert_eq!(org.website, None);
    assert_eq!(org.account.bio, None);
    let saved = store.find_by_name("ROCKETS").await.unwrap().unwrap();
    assert_eq!((saved.account.id, saved.account.created_at), (org.account.id, org.account.created_at));
    assert_eq!(store.base_permission(org.account.id).await.unwrap(), BasePermission::Write);
    let members = store.members(org.account.id).await.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!((members[0].user.id, members[0].role), (f.bob, OrgRole::Owner));
    let profiles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM org_profiles WHERE account_id = ?")
        .bind(org.account.id.0)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(profiles, 1);
    assert!(f.db.accounts().find_login("rockets").await.unwrap().is_none());
    clean(&f.db).await;
}

#[tokio::test]
async fn conditional_false_is_clean_and_never_deletes_user_or_populated_org() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    store.add_member(f.org, f.bob, OrgRole::Member).await.unwrap();
    let team = f.db.teams().create(f.org, &"core".parse().unwrap(), None).await.unwrap();
    f.db.teams().add_member(team.id, f.bob).await.unwrap();
    let repo = f.repo(f.org).await;
    let before = store.find(f.org).await.unwrap().unwrap();
    for id in [f.org, f.alice, AccountId(i64::MAX)] {
        assert!(!store.delete_if_empty(id).await.unwrap());
    }
    let after = store.find(f.org).await.unwrap().unwrap();
    assert_eq!((after.account.id, after.description), (before.account.id, before.description));
    assert_eq!(store.role_of(f.org, f.bob).await.unwrap(), Some(OrgRole::Member));
    assert!(f.db.teams().is_member(team.id, f.bob).await.unwrap());
    assert_eq!(f.db.accounts().find_login("alice").await.unwrap().unwrap().user.id(), f.alice);
    clean(&f.db).await;
    f.db.repositories().delete(repo).await.unwrap();
    assert!(store.delete_if_empty(f.org).await.unwrap());
    assert!(!store.delete_if_empty(f.org).await.unwrap());
    clean(&f.db).await;
}

#[tokio::test]
async fn empty_org_delete_includes_supported_children_and_preserves_other_authority() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    store.add_member(f.org, f.bob, OrgRole::Member).await.unwrap();
    let team = f.db.teams().create(f.org, &"core".parse().unwrap(), Some("About")).await.unwrap();
    f.db.teams().add_member(team.id, f.bob).await.unwrap();
    let other = store.create(input("other", f.alice)).await.unwrap().account.id;
    store.add_member(other, f.bob, OrgRole::Member).await.unwrap();
    let other_team = f.db.teams().create(other, &"core".parse().unwrap(), None).await.unwrap();
    f.db.teams().add_member(other_team.id, f.bob).await.unwrap();
    let repo = f.repo(other).await;
    f.db.teams().set_repository(other_team.id, repo, Permission::Read).await.unwrap();
    f.db.collaborators().add(repo, f.bob, Permission::Write, f.alice).await.unwrap();
    let hooks = f.db.webhooks();
    let hook = hooks
        .create(NewWebhook {
            id: None,
            owner: WebhookOwner::Organization(f.org),
            url: WebhookUrl::parse("https://example.com/hook").unwrap(),
            secret: None,
            events: vec![WebhookEvent::Push],
            active: true,
            created_by: Some(f.alice),
        })
        .await
        .unwrap();
    let deliveries = hooks
        .enqueue(vec![NewDelivery {
            hook_id: hook.id,
            guid: "org-lifecycle-fixture".into(),
            event: WebhookEvent::Ping,
            action: None,
            payload: "{}".into(),
        }])
        .await
        .unwrap();
    assert_eq!(hooks.count_deliveries(hook.id).await.unwrap(), 1);
    assert!(store.delete_if_empty(f.org).await.unwrap());
    for query in [
        "SELECT COUNT(*) FROM accounts WHERE id = ?",
        "SELECT COUNT(*) FROM org_profiles WHERE account_id = ?",
        "SELECT COUNT(*) FROM org_members WHERE org_id = ?",
        "SELECT COUNT(*) FROM teams WHERE org_id = ?",
        "SELECT COUNT(*) FROM webhooks WHERE org_id = ?",
    ] {
        let count: i64 = sqlx::query_scalar(query).bind(f.org.0).fetch_one(&f.db.pool).await.unwrap();
        assert_eq!(count, 0);
    }
    assert!(!f.db.teams().is_member(team.id, f.bob).await.unwrap());
    assert!(hooks.find_delivery(hook.id, deliveries[0]).await.unwrap().is_none());
    assert_eq!(store.role_of(other, f.bob).await.unwrap(), Some(OrgRole::Member));
    assert!(f.db.teams().is_member(other_team.id, f.bob).await.unwrap());
    assert_eq!(f.db.repositories().grants(repo, other, f.bob).await.unwrap().team, Some(Permission::Read));
    assert_eq!(f.db.collaborators().permission(repo, f.bob).await.unwrap(), Some(Permission::Write));
    for (name, id) in [("alice", f.alice), ("bob", f.bob)] {
        assert_eq!(f.db.accounts().find_login(name).await.unwrap().unwrap().user.id(), id);
    }
    clean(&f.db).await;
}

#[tokio::test]
async fn duplicate_and_late_creator_fk_error_are_unknown_without_committed_partial_org() {
    for name in ["ALICE", "ACME", "bad-creator"] {
        let f = Fixture::new().await;
        let store = f.db.orgs();
        let creator = if name == "bad-creator" { AccountId(i64::MAX) } else { f.alice };
        let result = store.create(input(name, creator)).await;
        if name == "bad-creator" {
            assert!(matches!(result, Err(StorageError::Sqlite(_))));
            assert!(store.find_by_name(name).await.unwrap().is_none());
        } else {
            assert!(matches!(result, Err(StorageError::Conflict("name"))));
        }
        let account_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts").fetch_one(&f.db.pool).await.unwrap();
        let profile_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM org_profiles").fetch_one(&f.db.pool).await.unwrap();
        let member_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM org_members").fetch_one(&f.db.pool).await.unwrap();
        assert_eq!((account_count, profile_count, member_count), (3, 1, 1));
        assert!(f.db.authority_domain.snapshot().uncertain);
        let fresh = store.create(input("later", f.alice)).await.unwrap();
        assert!(store.delete_if_empty(fresh.account.id).await.unwrap());
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn injected_child_delete_abort_is_not_clean_or_partial_success() {
    let f = Fixture::with_child_failure().await;
    let store = f.db.orgs();
    store.add_member(f.org, f.bob, OrgRole::Member).await.unwrap();
    let team = f.db.teams().create(f.org, &"core".parse().unwrap(), None).await.unwrap();
    f.db.teams().add_member(team.id, f.bob).await.unwrap();
    let failure = store.delete_if_empty(f.org).await.unwrap_err();
    assert!(
        matches!(&failure, StorageError::Sqlite(sqlx::Error::Database(error))
        if error.message() == "injected child failure" && error.code().as_deref() == Some("1811")),
        "{failure:?}"
    );
    assert!(store.find(f.org).await.unwrap().is_some());
    assert_eq!(store.role_of(f.org, f.bob).await.unwrap(), Some(OrgRole::Member));
    assert!(f.db.teams().is_member(team.id, f.bob).await.unwrap());
    assert!(f.db.authority_domain.snapshot().uncertain);
    // Statement abort only; outer transaction cleanup is unacknowledged, not a lost-COMMIT/crash proof.
    sqlx::query("UPDATE org_lifecycle_test_gate SET armed = 0").execute(&f.db.pool).await.unwrap();
    assert!(store.delete_if_empty(f.org).await.unwrap());
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn lifecycle_shares_membership_and_team_fifo_without_guarding_reads() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    let teams = f.db.teams();
    let team = teams.create(f.org, &"core".parse().unwrap(), None).await.unwrap();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut membership = Box::pin(store.add_member(f.org, f.bob, OrgRole::Member));
    let mut team_member = Box::pin(teams.add_member(team.id, f.bob));
    let mut create = Box::pin(apply(&f, &store, Operation::Create));
    let mut delete = Box::pin(apply(&f, &store, Operation::Delete));
    let mut canceled = Box::pin(apply(&f, &store, Operation::Create));
    pending(membership.as_mut()).await;
    pending(team_member.as_mut()).await;
    pending(create.as_mut()).await;
    pending(delete.as_mut()).await;
    pending(canceled.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 5);
    assert_eq!(timeout(WAIT, store.find(f.org)).await.unwrap().unwrap().unwrap().account.id, f.org);
    assert_eq!(timeout(WAIT, store.count_repositories(f.org)).await.unwrap().unwrap(), 0);
    drop(canceled);
    assert_eq!(f.db.authority_domain.snapshot().queued, 4);
    drop(quiet);
    timeout(WAIT, membership).await.unwrap().unwrap();
    assert_eq!(timeout(WAIT, team_member).await.unwrap().unwrap(), crate::sqlite::teams::TeamChange::Done);
    assert!(teams.is_member(team.id, f.bob).await.unwrap());
    timeout(WAIT, create).await.unwrap().unwrap();
    timeout(WAIT, delete).await.unwrap().unwrap();
    assert!(store.find_by_name("fresh").await.unwrap().is_some());
    assert!(store.find(f.org).await.unwrap().is_none());
    clean(&f.db).await;
}

#[tokio::test]
async fn capacity_and_terminal_close_preserve_distinct_lifecycle_and_member_scopes() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut queued = Vec::new();
    for _ in 0..128 {
        let mut command = Box::pin(apply(&f, &store, Operation::Create));
        pending(command.as_mut()).await;
        queued.push(command);
    }
    for operation in OPERATIONS {
        assert!(matches!(
            timeout(WAIT, apply(&f, &store, operation)).await.unwrap(),
            Err(StorageError::AdmissionUnavailable { scope: "organization", reason: AdmissionReason::Capacity })
        ));
    }
    drop(queued);
    drop(quiet);
    clean(&f.db).await;
    let database = f.db.clone();
    drop(database.close());
    for fresh in [false, true] {
        let handle = if fresh { database.orgs() } else { store.clone() };
        for operation in OPERATIONS {
            assert!(matches!(
                apply(&f, &handle, operation).await,
                Err(StorageError::AdmissionUnavailable { scope: "organization", reason: AdmissionReason::Stopped })
            ));
        }
        assert!(matches!(
            handle.add_member(f.org, f.bob, OrgRole::Member).await,
            Err(StorageError::AdmissionUnavailable { scope: "organization membership", reason: AdmissionReason::Stopped })
        ));
    }
    assert!(store.find(f.org).await.unwrap().is_some());
    assert!(store.find_by_name("fresh").await.unwrap().is_none());
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn already_acquired_lifecycle_can_finish_after_stop() {
    for operation in OPERATIONS {
        let f = Fixture::new().await;
        // Hold every actual ordinary-pool permit. The Store can acquire its
        // enrollment, but cannot begin SQL until these connections return.
        // This is explicitly a pool-acquire boundary, not driver-entry evidence.
        let held = timeout(WAIT, async {
            let mut held = Vec::new();
            for _ in 0..f.db.pool.options().get_max_connections() {
                held.push(f.db.pool.acquire().await.unwrap());
            }
            held
        })
        .await
        .unwrap();
        let store = f.db.orgs();
        let mut command = Box::pin(apply(&f, &store, operation));
        poll_fn(|cx| {
            let observed = command.as_mut().poll(cx);
            assert!(observed.is_pending(), "{operation:?} returned before pool release: {observed:?}");
            Poll::Ready(())
        })
        .await;
        let enrolled = f.db.authority_domain.snapshot();
        assert_eq!(enrolled.active, 1);
        assert_eq!(enrolled.queued, 0);
        drop(f.db.authority_domain.stop_writers());
        // SQLx returns dropped connections asynchronously. The successful awaited
        // Store result below, not this Drop, acknowledges continuation completion.
        drop(held);
        timeout(WAIT, command).await.unwrap().unwrap();
        match operation {
            Operation::Create => {
                let created = store.find_by_name("fresh").await.unwrap().unwrap();
                assert_eq!(created.description.as_deref(), Some("About"));
                assert_eq!(store.role_of(created.account.id, f.alice).await.unwrap(), Some(OrgRole::Owner));
                let profiles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM org_profiles WHERE account_id = ?")
                    .bind(created.account.id.0)
                    .fetch_one(&f.db.pool)
                    .await
                    .unwrap();
                assert_eq!(profiles, 1);
            }
            Operation::Delete => assert!(store.find(f.org).await.unwrap().is_none()),
        }
        for (name, id) in [("alice", f.alice), ("bob", f.bob)] {
            assert_eq!(f.db.accounts().find_login(name).await.unwrap().unwrap().user.id(), id);
        }
        timeout(WAIT, f.db.authority_domain.stop_writers().drain(Instant::now() + WAIT)).await.unwrap().unwrap();
        assert!(!f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn acquired_lifecycle_loss_under_sql_writer_is_unknown() {
    for operation in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.orgs();
        let mut command = Box::pin(apply(&f, &store, operation));
        pending(command.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "enrollment only, not SQLite worker-entry proof");
        drop(command);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(f.db.authority_domain.snapshot().uncertain);
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}
