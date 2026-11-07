//! Policy and effect ordering without Git, SQL or an async runtime.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{
    OwnedRepository, Repository, Visibility,
    transfer::{RepositoryTransfer, TransferError, TransferPort, TransferWriteError},
};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
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
#[derive(Debug)]
struct Port {
    calls: Mutex<Vec<&'static str>>,
    target: Account,
    suspended: bool,
    failures: TransferFailures,
    denied: bool,
}
impl Port {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            target: account(2, "bob", AccountKind::User),
            suspended: false,
            failures: TransferFailures::default(),
            denied: false,
        }
    }
    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}
impl TransferPort for Port {
    type Error = &'static str;
    type MoveError = &'static str;
    fn find_owner(&self, _: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send {
        self.record("find");
        ready(Ok(Some(self.target.clone())))
    }
    fn authorized_owner(&self, _: &User, _: &str) -> impl Future<Output = Result<Account, TransferError<Self::Error>>> + Send {
        self.record("authorize");
        ready(if self.denied { Err(TransferError::Invalid("old owner error".into())) } else { Ok(self.target.clone()) })
    }
    fn is_suspended(&self, _: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.record("suspended");
        ready(Ok(self.suspended))
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
    fn transfer(&self, _: RepoId, _: AccountId) -> impl Future<Output = Result<(), TransferWriteError<Self::Error>>> + Send {
        self.record("sql");
        ready(if self.failures.conflict { Err(TransferWriteError::Conflict) } else { Ok(()) })
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
    assert_eq!(port.calls(), ["authorize", "suspended", "close", "move", "sql", "publish_audit"]);
}
#[test]
fn suspension_and_denial_precede_reader_close_and_side_effects() {
    let (actor, repo) = inputs();
    let mut port = Port::new();
    port.suspended = true;
    assert!(
        matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")), Err(TransferError::Invalid(message)) if message == "bob is suspended")
    );
    assert_eq!(port.calls(), ["authorize", "suspended"]);
    let mut port = Port::new();
    port.denied = true;
    assert!(
        matches!(run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")), Err(TransferError::Invalid(message)) if message == "you cannot transfer repositories to bob")
    );
    assert_eq!(port.calls(), ["authorize"]);
}
#[test]
fn site_admin_lookup_and_organization_skip_are_distinct_fast_paths() {
    let (mut actor, repo) = inputs();
    actor.is_admin = true;
    let mut port = Port::new();
    port.target.kind = AccountKind::Organization;
    run(RepositoryTransfer::execute(&port, &actor, &repo, true, "bob", "alice/project")).unwrap();
    assert_eq!(port.calls(), ["find", "close", "move", "sql", "publish_audit"]);
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
    assert_eq!(port.calls(), ["authorize", "suspended", "close", "move", "sql", "restore", "restore_failed"]);
}
