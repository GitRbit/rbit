//! Exact lookup and permission work, including absent/current-address fast paths.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind};
use rbit_repositories::{
    OwnedRepository, Repository, Visibility,
    aliases::{AliasPort, RepositoryAliases},
};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};
fn repository() -> OwnedRepository {
    OwnedRepository {
        owner: Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        repo: Repository {
            id: RepoId(9),
            owner_id: AccountId(1),
            name: "project".parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".into(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            pushed_at: None,
            archived_at: None,
        },
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("unexpected suspension"),
    }
}
#[derive(Debug, Default)]
struct Port {
    current: bool,
    old: bool,
    readable: bool,
    calls: Mutex<Vec<String>>,
}
impl Port {
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}
impl AliasPort for Port {
    type Error = &'static str;
    type Opened = RepoId;
    fn find_current(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.record(format!("current:{owner}/{name}"));
        ready(Ok(self.current.then(repository)))
    }
    fn find_old(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.record(format!("old:{owner}/{name}"));
        ready(Ok(self.old.then(repository)))
    }
    fn open(&self, repo: OwnedRepository) -> impl Future<Output = Result<RepoId, Self::Error>> + Send {
        self.record("open".into());
        ready(if self.readable { Ok(repo.repo.id) } else { Err("not found") })
    }
    fn readable(&self, _: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.record("readable".into());
        ready(Ok(self.readable))
    }
    fn not_found(&self) -> Self::Error {
        "not found"
    }
}
#[test]
fn current_private_address_wins_without_alias_lookup_even_when_unreadable() {
    let port = Port { current: true, old: true, ..Port::default() };
    assert_eq!(run(RepositoryAliases::open(&port, "alice", "project.git")), Err("not found"));
    assert_eq!(*port.calls.lock().unwrap(), ["current:alice/project", "open"]);
}
#[test]
fn missing_alias_performs_no_permission_or_open_work_and_preserves_browser_name() {
    let port = Port::default();
    assert!(run(RepositoryAliases::moved_to(&port, "alice", "project.git")).unwrap().is_none());
    assert_eq!(*port.calls.lock().unwrap(), ["old:alice/project.git"]);
}
#[test]
fn old_private_destination_is_not_revealed_and_git_fallback_uses_one_open() {
    let port = Port { old: true, ..Port::default() };
    assert!(run(RepositoryAliases::moved_to(&port, "alice", "project")).unwrap().is_none());
    assert_eq!(*port.calls.lock().unwrap(), ["old:alice/project", "readable"]);
    let port = Port { old: true, readable: true, ..Port::default() };
    assert_eq!(run(RepositoryAliases::open(&port, "alice", "project.git")), Ok(RepoId(9)));
    assert_eq!(*port.calls.lock().unwrap(), ["current:alice/project", "old:alice/project", "open"]);
}
#[test]
fn unpolled_alias_commands_do_no_lookup_work() {
    let port = Port::default();
    drop(RepositoryAliases::open(&port, "alice", "project"));
    drop(RepositoryAliases::moved_to(&port, "alice", "project"));
    assert_eq!(*port.calls.lock().unwrap(), Vec::<String>::new());
}
