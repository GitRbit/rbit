//! Existing-organization role/base APIs only; real trigger effects, no lifecycle claim.
use super::*;
use crate::{Database, NewRepository, NewUser};
use rbit_core::{Permission, RepoId, Visibility, membership::TeamId};
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
        db.orgs().add_member(org, bob, OrgRole::Member).await.unwrap();
        Self { _directory: directory, db, org, alice, bob, carol }
    }
    async fn team_and_repo(&self, org: AccountId) -> (TeamId, RepoId) {
        let team = self.db.teams().create(org, &"core".parse().unwrap(), None).await.unwrap().id;
        self.db.teams().add_member(team, self.bob).await.unwrap();
        let repo = new_repo(&self.db, org).await;
        self.db.teams().set_repository(team, repo, Permission::Write).await.unwrap();
        self.db.collaborators().add(repo, self.bob, Permission::Read, self.alice).await.unwrap();
        (team, repo)
    }
}
async fn new_org(db: &Database, name: &str, creator: AccountId) -> AccountId {
    db.orgs()
        .create(NewOrganization {
            name: name.parse().unwrap(),
            display_name: Some("Display".into()),
            description: Some("About".into()),
            creator,
        })
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
#[derive(Debug, Clone, Copy)]
enum Operation {
    Add,
    Set,
    Remove,
    Base,
}
const OPERATIONS: [Operation; 4] = [Operation::Add, Operation::Set, Operation::Remove, Operation::Base];
async fn apply(f: &Fixture, store: &OrgStore, operation: Operation) -> Result<(), StorageError> {
    match operation {
        Operation::Add => store.add_member(f.org, f.carol, OrgRole::Member).await,
        Operation::Set => store.set_role(f.org, f.bob, OrgRole::Owner).await.map(|_| ()),
        Operation::Remove => store.remove_member(f.org, f.bob).await.map(|_| ()),
        Operation::Base => store.set_base_permission(f.org, BasePermission::Read).await,
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
async fn business_outcomes_and_base_profile_preservation_settle_cleanly() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    assert_eq!(store.set_role(f.org, f.alice, OrgRole::Member).await.unwrap(), MembershipChange::LastOwner);
    assert_eq!(store.remove_member(f.org, f.alice).await.unwrap(), MembershipChange::LastOwner);
    assert_eq!(store.set_role(f.org, f.alice, OrgRole::Owner).await.unwrap(), MembershipChange::Done);
    assert_eq!(store.set_role(f.org, f.carol, OrgRole::Owner).await.unwrap(), MembershipChange::NotMember);
    assert_eq!(store.remove_member(f.org, f.carol).await.unwrap(), MembershipChange::NotMember);
    store.add_member(f.org, f.carol, OrgRole::Owner).await.unwrap();
    assert_eq!(store.set_role(f.org, f.alice, OrgRole::Member).await.unwrap(), MembershipChange::Done);
    assert_eq!(store.remove_member(f.org, f.bob).await.unwrap(), MembershipChange::Done);
    for base in BasePermission::ALL {
        store.set_base_permission(f.org, base).await.unwrap();
        store.set_base_permission(f.org, base).await.unwrap();
        assert_eq!(store.base_permission(f.org).await.unwrap(), base);
        assert_eq!(store.find(f.org).await.unwrap().unwrap().description.as_deref(), Some("About"));
    }
    // A nondefault value catches a profile update that incorrectly resets the
    // permission to the schema's Write default.
    store.set_base_permission(f.org, BasePermission::Read).await.unwrap();
    store
        .update_profile(f.org, OrgProfileUpdate { display_name: Some("Changed".into()), description: Some("New".into()), website: None })
        .await
        .unwrap();
    assert_eq!(store.base_permission(f.org).await.unwrap(), BasePermission::Read);
    let quiet = timeout(WAIT, f.db.authority_domain.drain(Instant::now() + WAIT)).await.unwrap().unwrap();
    drop(quiet);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn concurrent_owner_demotions_or_removals_leave_exactly_one_owner() {
    for remove in [false, true] {
        let f = Fixture::new().await;
        let store = f.db.orgs();
        store.set_role(f.org, f.bob, OrgRole::Owner).await.unwrap();
        let (a, b) = if remove {
            tokio::join!(store.remove_member(f.org, f.alice), store.remove_member(f.org, f.bob))
        } else {
            tokio::join!(store.set_role(f.org, f.alice, OrgRole::Member), store.set_role(f.org, f.bob, OrgRole::Member))
        };
        let outcomes = [a.unwrap(), b.unwrap()];
        assert_eq!(outcomes.iter().filter(|v| **v == MembershipChange::Done).count(), 1);
        assert_eq!(outcomes.iter().filter(|v| **v == MembershipChange::LastOwner).count(), 1);
        assert_eq!(store.members(f.org).await.unwrap().iter().filter(|v| v.role == OrgRole::Owner).count(), 1);
        assert!(!f.db.authority_domain.snapshot().uncertain);
    }
}

#[tokio::test]
async fn returned_removal_includes_real_trigger_revocation_only_for_its_org() {
    let f = Fixture::new().await;
    let other = new_org(&f.db, "other", f.alice).await;
    f.db.orgs().add_member(other, f.bob, OrgRole::Member).await.unwrap();
    let (team, repo) = f.team_and_repo(f.org).await;
    let (other_team, other_repo) = f.team_and_repo(other).await;
    let personal = new_repo(&f.db, f.alice).await;
    f.db.collaborators().add(personal, f.bob, Permission::Write, f.alice).await.unwrap();
    assert_eq!(f.db.orgs().remove_member(f.org, f.bob).await.unwrap(), MembershipChange::Done);
    assert!(!f.db.teams().is_member(team, f.bob).await.unwrap());
    let retained: Option<String> = sqlx::query_scalar("SELECT permission FROM team_repositories WHERE team_id = ? AND repo_id = ?")
        .bind(team.0)
        .bind(repo.0)
        .fetch_optional(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(retained.as_deref(), Some("write"), "removing one member must not revoke the entire team's repository grant");
    assert_eq!(f.db.repositories().grants(repo, f.org, f.bob).await.unwrap().team, None);
    assert!(f.db.teams().is_member(other_team, f.bob).await.unwrap());
    assert_eq!(f.db.collaborators().permission(repo, f.bob).await.unwrap(), None);
    assert_eq!(f.db.collaborators().permission(other_repo, f.bob).await.unwrap(), Some(Permission::Read));
    assert_eq!(f.db.collaborators().permission(personal, f.bob).await.unwrap(), Some(Permission::Write));
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn injected_trigger_abort_never_certifies_or_partially_revokes() {
    let f = Fixture::new().await;
    let (team, repo) = f.team_and_repo(f.org).await;
    sqlx::query(
        "CREATE TRIGGER org_fence_failure BEFORE DELETE ON collaborators BEGIN SELECT RAISE(ABORT, 'injected trigger failure'); END",
    )
    .execute(&f.db.pool)
    .await
    .unwrap();
    assert!(f.db.orgs().remove_member(f.org, f.bob).await.is_err());
    // Actual SQLite statement rollback includes the earlier team deletion in the
    // AFTER org_members trigger; this injection is not uncertain-COMMIT/crash proof.
    assert_eq!(f.db.orgs().role_of(f.org, f.bob).await.unwrap(), Some(OrgRole::Member));
    assert!(f.db.teams().is_member(team, f.bob).await.unwrap());
    assert_eq!(f.db.collaborators().permission(repo, f.bob).await.unwrap(), Some(Permission::Read));
    assert!(f.db.authority_domain.snapshot().uncertain);
    f.db.orgs().set_base_permission(f.org, BasePermission::None).await.unwrap();
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn all_four_queue_with_credentials_and_reads_stay_independent() {
    let f = Fixture::new().await;
    let store = f.db.orgs();
    let profiles = f.db.profiles();
    let quiet = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut credential = Box::pin(profiles.set_password_hash(f.alice, "changed"));
    pending(credential.as_mut()).await;
    let mut add = Box::pin(apply(&f, &store, Operation::Add));
    let mut set = Box::pin(apply(&f, &store, Operation::Set));
    let mut remove = Box::pin(apply(&f, &store, Operation::Remove));
    let mut base = Box::pin(apply(&f, &store, Operation::Base));
    pending(add.as_mut()).await;
    pending(set.as_mut()).await;
    pending(remove.as_mut()).await;
    pending(base.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 5);
    assert_eq!(timeout(WAIT, store.role_of(f.org, f.bob)).await.unwrap().unwrap(), Some(OrgRole::Member));
    assert_eq!(timeout(WAIT, store.base_permission(f.org)).await.unwrap().unwrap(), BasePermission::Write);
    drop(credential);
    drop(set);
    assert_eq!(f.db.authority_domain.snapshot().queued, 3);
    drop(quiet);
    timeout(WAIT, add).await.unwrap().unwrap();
    timeout(WAIT, remove).await.unwrap().unwrap();
    timeout(WAIT, base).await.unwrap().unwrap();
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn old_cloned_and_new_handles_reject_all_four_after_unpolled_close() {
    let f = Fixture::new().await;
    let old = f.db.orgs();
    let database = f.db.clone();
    drop(database.close());
    for fresh in [false, true] {
        let store = if fresh { database.orgs() } else { old.clone() };
        for operation in OPERATIONS {
            assert!(matches!(apply(&f, &store, operation).await, Err(StorageError::Conflict("organization membership writers stopped"))));
        }
    }
    assert_eq!(old.role_of(f.org, f.bob).await.unwrap(), Some(OrgRole::Member));
    assert_eq!(old.base_permission(f.org).await.unwrap(), BasePermission::Write);
    assert!(!f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn acquired_loss_all_four_is_unknown_with_real_sql_writer_lock_held() {
    for operation in OPERATIONS {
        let f = Fixture::new().await;
        let mut lock = f.db.pool.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *lock).await.unwrap();
        let store = f.db.orgs();
        let mut command = Box::pin(apply(&f, &store, operation));
        pending(command.as_mut()).await;
        assert_eq!(f.db.authority_domain.snapshot().active, 1, "enrolled while lock held, not SQL worker-entry proof");
        drop(command);
        assert!(f.db.authority_domain.snapshot().uncertain);
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        assert!(matches!(f.db.authority_domain.drain(Instant::now() + WAIT).await, Err(rbit_authority_fence::DrainError::Uncertain)));
    }
}

#[tokio::test]
async fn duplicate_and_foreign_key_errors_stick_without_disabling_passive_writes() {
    for duplicate in [false, true] {
        let f = Fixture::new().await;
        let store = f.db.orgs();
        let result = if duplicate {
            store.add_member(f.org, f.bob, OrgRole::Owner).await
        } else {
            store.set_base_permission(AccountId(i64::MAX), BasePermission::Read).await
        };
        if duplicate {
            assert!(matches!(result, Err(StorageError::Conflict("member"))));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(store.role_of(f.org, f.bob).await.unwrap(), Some(OrgRole::Member));
        assert!(f.db.authority_domain.snapshot().uncertain);
        store.set_base_permission(f.org, BasePermission::Read).await.unwrap();
        assert_eq!(store.base_permission(f.org).await.unwrap(), BasePermission::Read);
        assert!(f.db.authority_domain.snapshot().uncertain);
    }
}
