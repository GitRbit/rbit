//! Policy and effect ordering without Git, SQL or an async runtime.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{
    OwnedRepository, Repository, Visibility,
    creation::{RepositoryMembership, RepositoryOwnerCatalog},
    transfer::{RepositoryTransfer, TransferError, TransferPort, TransferRepository, TransferWriteError},
};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};

fn account(id: i64, name: &str, kind: AccountKind) -> Account {
    Account { id: AccountId(id), name: name.parse().unwrap(), kind, display_name: None, bio: None, created_at: Timestamp(1) }
}
fn inputs() -> (User, OwnedRepository) {
    let owner = account(1, "alice", AccountKind::User);
    let actor = User { account: owner.clone(), email: "alice@example.com".parse().unwrap(), is_admin: false };
    let repo = Repository {
        id: RepoId(9),
        owner_id: owner.id,
        name: "project".parse().unwrap(),
        description: None,
        visibility: Visibility::Private,
        default_branch: "main".into(),
        created_at: Timestamp(1),
        updated_at: Timestamp(2),
        pushed_at: None,
        archived_at: Some(Timestamp(3)),
    };
    (actor, OwnedRepository { owner, repo })
}
fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}
#[derive(Debug, Default)]
struct TransferFailures {
    conflict: bool,
    rollback: bool,
}
type Calls = Arc<Mutex<Vec<&'static str>>>;
/// One destination lookup's catalog; the target is `bob`.
#[derive(Debug)]
struct Owners {
    calls: Calls,
    target: Account,
    member: bool,
}
impl RepositoryOwnerCatalog for Owners {
    type Error = &'static str;
    fn owner_named(&self, _: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send {
        self.calls.lock().unwrap().push("owner");
        ready(Ok(Some(self.target.clone())))
    }
    fn membership(&self, _: AccountId, _: AccountId) -> impl Future<Output = Result<Option<RepositoryMembership>, Self::Error>> + Send {
        self.calls.lock().unwrap().push("member");
        ready(Ok(self.member.then_some(RepositoryMembership::Member)))
    }
}
#[derive(Debug)]
struct Port {
    calls: Calls,
    target: Account,
    suspended: bool,
    failures: TransferFailures,
    member: bool,
    organizations: Vec<Account>,
}
impl Port {
    fn new() -> Self {
        Self {
            calls: Calls::default(),
            target: account(2, "bob", AccountKind::Organization),
            suspended: false,
            failures: TransferFailures::default(),
            member: true,
            organizations: Vec::new(),
        }
    }
    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}
impl TransferRepository for Port {
    type Error = &'static str;
    type Owners = Owners;
    type Organizations = Vec<Account>;
    fn owners(&self) -> Owners {
        Owners { calls: self.calls.clone(), target: self.target.clone(), member: self.member }
    }
    fn is_suspended(&self, _: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.record("suspended");
        ready(Ok(self.suspended))
    }
    fn organizations_of(&self, _: AccountId) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send {
        self.record("organizations");
        ready(Ok(self.organizations.clone()))
    }
    fn transfer(&self, _: RepoId, _: AccountId) -> impl Future<Output = Result<(), TransferWriteError<Self::Error>>> + Send {
        self.record("sql");
        ready(if self.failures.conflict { Err(TransferWriteError::Conflict) } else { Ok(()) })
    }
}
impl TransferPort for Port {
    type Store = Self;
    type Error = &'static str;
    type MoveError = &'static str;
    fn repository(&self) -> &Self {
        self
    }
    fn close_reader(&self) {
        self.record("close");
    }
    fn relocate(&self, _: RepoId, from: AccountId, _: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.record(if from == AccountId(1) { "move" } else { "restore" });
        ready(if from != AccountId(1) && self.failures.rollback { Err("rollback") } else { Ok(()) })
    }
    fn move_error(&self, error: Self::MoveError) -> Self::Error {
        error
    }
    fn restore_failed(&self, _: &Self::Error, _: &str) {
        self.record("restore_failed");
    }
    fn transferred(&self, _: &User, _: &Repository, _: &OwnedRepository, _: String) -> impl Future<Output = ()> + Send {
        self.record("publish_audit");
        ready(())
    }
}
#[test]
fn forbidden_invalid_and_unpolled_commands_do_no_port_work() {
    let (actor, repo) = inputs();
    let port = Port::new();
    drop(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project"));
    assert!(matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, false, "", "")), Err(TransferError::Forbidden)));
    for (owner, confirmation) in [("  ", ""), ("bob", "wrong")] {
        assert!(matches!(
            run(RepositoryTransfer::execute(&port, &actor, &repo, true, owner, confirmation)),
            Err(TransferError::Invalid(_))
        ));
    }
    assert_eq!(port.calls(), Vec::<&str>::new());
}
#[test]
fn archived_transfer_keeps_snapshot_and_order_without_archive_guard() {
    let (actor, repo) = inputs();
    let port = Port::new();
    let moved = run(RepositoryTransfer::execute(&port, &actor, &repo, true, " bob ", " alice/project ")).unwrap();
    assert_eq!(moved.repo.owner_id, AccountId(2));
    assert_eq!(moved.repo.archived_at, repo.repo.archived_at);
    assert_eq!(moved.repo.updated_at, repo.repo.updated_at);
    assert_eq!(port.calls(), ["owner", "member", "close", "move", "sql", "publish_audit"]);
}
#[test]
fn suspension_and_denial_precede_reader_close_and_side_effects() {
    let (mut actor, repo) = inputs();
    actor.is_admin = true;
    let mut port = Port::new();
    port.target.kind = AccountKind::User;
    port.suspended = true;
    assert!(
        matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")), Err(TransferError::Invalid(message)) if message == "bob is suspended")
    );
    assert_eq!(port.calls(), ["owner", "suspended"]);
    actor.is_admin = false;
    for kind in [AccountKind::User, AccountKind::Organization] {
        let mut port = Port::new();
        port.target.kind = kind;
        port.member = false;
        assert!(
            matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")), Err(TransferError::Invalid(message)) if message == "you cannot transfer repositories to bob")
        );
        let expected: &[&str] = if kind == AccountKind::User { &["owner"] } else { &["owner", "member"] };
        assert_eq!(port.calls(), expected);
    }
}
#[test]
fn site_admin_lookup_and_organization_skip_are_distinct_fast_paths() {
    let (mut actor, repo) = inputs();
    actor.is_admin = true;
    let port = Port::new();
    run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")).unwrap();
    assert_eq!(port.calls(), ["owner", "close", "move", "sql", "publish_audit"]);
    let mut port = Port::new();
    port.target.kind = AccountKind::User;
    run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")).unwrap();
    assert_eq!(port.calls(), ["owner", "suspended", "close", "move", "sql", "publish_audit"]);
}
#[test]
fn own_account_is_resolved_without_lookup_and_rejected_as_the_same_owner() {
    let (actor, repo) = inputs();
    let port = Port::new();
    assert!(
        matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "ALICE", "alice/project")), Err(TransferError::Invalid(message)) if message == "alice/project already belongs to alice")
    );
    assert_eq!(port.calls(), ["suspended"]);
}
#[test]
fn conflict_is_preserved_when_compensating_move_also_fails() {
    let (actor, repo) = inputs();
    let mut port = Port::new();
    port.failures.conflict = true;
    port.failures.rollback = true;
    assert!(
        matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")), Err(TransferError::Invalid(message)) if message == "bob/project already exists; rename one of them first")
    );
    assert_eq!(port.calls(), ["owner", "member", "close", "move", "sql", "restore", "restore_failed"]);
}
#[test]
fn suggestions_need_administration_and_exclude_the_current_owner_in_order() {
    let (actor, mut repo) = inputs();
    let mut port = Port::new();
    port.organizations = vec![account(3, "acme", AccountKind::Organization), account(4, "beta", AccountKind::Organization)];
    drop(RepositoryTransfer::suggested_owners(&port, &actor, &repo, true));
    assert!(matches!(run(RepositoryTransfer::suggested_owners(&port, &actor, &repo, false)), Err(TransferError::Forbidden)));
    assert_eq!(port.calls(), Vec::<&str>::new());
    let names = |owners: Vec<Account>| owners.into_iter().map(|account| account.name.to_string()).collect::<Vec<_>>();
    assert_eq!(names(run(RepositoryTransfer::suggested_owners(&port, &actor, &repo, true)).unwrap()), ["acme", "beta"]);
    repo.repo.owner_id = AccountId(3);
    assert_eq!(names(run(RepositoryTransfer::suggested_owners(&port, &actor, &repo, true)).unwrap()), ["alice", "beta"]);
    assert_eq!(port.calls(), ["organizations", "organizations"]);
}
