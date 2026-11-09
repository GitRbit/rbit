//! SSH sign-in and deploy-key reach decisions, with the lookups they cost.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{
    OwnedRepository, Repository, Visibility,
    aliases::RepositoryAddresses,
    ssh_keys::{DeployKeyId, KeySignIn, SshIdentity, SshKeyDirectory, SshKeyId, StoredKeyHolder, deploy_key_reach, sign_in},
};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn account(id: i64, name: &str) -> Account {
    Account {
        id: AccountId(id),
        kind: AccountKind::User,
        name: name.parse().unwrap(),
        display_name: None,
        bio: None,
        created_at: Timestamp(1),
    }
}

fn user() -> User {
    User { account: account(5, "alice"), email: "alice@example.com".parse().unwrap(), is_admin: false }
}

fn repository(id: i64) -> OwnedRepository {
    OwnedRepository {
        owner: account(5, "alice"),
        repo: Repository {
            id: RepoId(id),
            owner_id: AccountId(5),
            name: format!("project{id}").parse().unwrap(),
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

/// A directory answering one stored holder and recording its lookups.
#[derive(Debug, Default)]
struct Directory {
    holder: Option<StoredKeyHolder>,
    password_change_required: bool,
    calls: Mutex<Vec<&'static str>>,
}

impl SshKeyDirectory for Directory {
    type Error = &'static str;
    fn holder(&self, _fingerprint: &str) -> impl Future<Output = Result<Option<StoredKeyHolder>, Self::Error>> + Send {
        self.calls.lock().unwrap().push("holder");
        ready(Ok(self.holder.clone()))
    }
    fn password_change_required(&self, user: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        assert_eq!(user, AccountId(5));
        self.calls.lock().unwrap().push("password_change_required");
        ready(Ok(self.password_change_required))
    }
}

fn user_key(suspended: bool) -> StoredKeyHolder {
    StoredKeyHolder::User { key: SshKeyId(1), user: user(), suspended }
}

fn deploy_identity(read_only: bool) -> SshIdentity {
    SshIdentity::Deploy { key: DeployKeyId(2), repo: repository(9), read_only }
}

#[test]
fn active_user_keys_sign_their_user_in() {
    let directory = Directory { holder: Some(user_key(false)), ..Directory::default() };
    let signed_in = run(sign_in(&directory, "SHA256:k")).unwrap();
    assert!(matches!(signed_in.into_identity(), Some(SshIdentity::User { key: SshKeyId(1), user }) if user.id() == AccountId(5)));
    assert_eq!(*directory.calls.lock().unwrap(), ["holder", "password_change_required"]);
}

#[test]
fn suspended_users_are_refused_before_the_password_check() {
    let directory = Directory { holder: Some(user_key(true)), password_change_required: true, ..Directory::default() };
    let signed_in = run(sign_in(&directory, "SHA256:k")).unwrap();
    assert!(matches!(&signed_in, KeySignIn::Suspended(user) if user.id() == AccountId(5)));
    assert!(signed_in.into_identity().is_none());
    assert_eq!(*directory.calls.lock().unwrap(), ["holder"]);
}

#[test]
fn users_who_must_change_their_password_are_refused() {
    let directory = Directory { holder: Some(user_key(false)), password_change_required: true, ..Directory::default() };
    assert!(matches!(run(sign_in(&directory, "SHA256:k")).unwrap(), KeySignIn::PasswordChangeRequired));
}

#[test]
fn deploy_keys_and_unknown_keys() {
    let deploy = Directory {
        holder: Some(StoredKeyHolder::Deploy { key: DeployKeyId(2), repo: repository(9), read_only: true }),
        password_change_required: true,
        ..Directory::default()
    };
    let signed_in = run(sign_in(&deploy, "SHA256:d")).unwrap().into_identity();
    assert!(matches!(signed_in, Some(SshIdentity::Deploy { key: DeployKeyId(2), read_only: true, .. })));
    assert_eq!(*deploy.calls.lock().unwrap(), ["holder"], "deploy keys have no password");
    assert!(matches!(run(sign_in(&Directory::default(), "SHA256:x")).unwrap(), KeySignIn::Unknown));
}

/// Addresses holding repository 9 under `alice/project9` (current) or `alice/old` (former).
#[derive(Debug, Default)]
struct Addresses {
    calls: Mutex<Vec<String>>,
}

impl RepositoryAddresses for Addresses {
    type Error = &'static str;
    fn find_current(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.calls.lock().unwrap().push(format!("current:{owner}/{name}"));
        ready(Ok(match name {
            "project9" => Some(repository(9)),
            "project3" => Some(repository(3)),
            _ => None,
        }))
    }
    fn find_old(&self, owner: &str, name: &str) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.calls.lock().unwrap().push(format!("old:{owner}/{name}"));
        ready(Ok((name == "old").then(|| repository(9))))
    }
}

#[test]
fn deploy_keys_reach_only_their_repository_at_their_access() {
    let addresses = Addresses::default();
    let reach = run(deploy_key_reach(&addresses, &deploy_identity(true), "alice", "project9.git")).unwrap().unwrap();
    assert_eq!((reach.repo.repo.id, reach.writable), (RepoId(9), false));
    let reach = run(deploy_key_reach(&addresses, &deploy_identity(false), "alice", "old")).unwrap().unwrap();
    assert_eq!((reach.repo.repo.id, reach.writable), (RepoId(9), true), "old names reach it too");
    assert!(run(deploy_key_reach(&addresses, &deploy_identity(false), "alice", "project3")).unwrap().is_none());
    assert!(run(deploy_key_reach(&addresses, &deploy_identity(false), "alice", "missing")).unwrap().is_none());
}

#[test]
fn user_keys_reach_nothing_without_a_lookup() {
    let addresses = Addresses::default();
    let user_key = SshIdentity::User { key: SshKeyId(1), user: user() };
    assert!(run(deploy_key_reach(&addresses, &user_key, "alice", "project9")).unwrap().is_none());
    assert!(addresses.calls.lock().unwrap().is_empty());
}
