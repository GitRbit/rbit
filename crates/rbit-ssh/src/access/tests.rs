// The fakes answer at once; their methods stay `async fn` like the real adapters.
#![allow(unknown_lints, clippy::unused_async_trait_impl)]

use std::sync::Mutex;

use rbit_core::{Account, AccountId, AccountKind, Timestamp};
use rbit_services::ssh_keys::SshKeyId;
use rbit_transport::Service;

use super::*;

fn user(id: i64) -> User {
    User {
        account: Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1_792_022_400),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}

fn identity(key: i64, user_id: i64) -> SshIdentity {
    SshIdentity::User { key: SshKeyId(key), user: user(user_id) }
}

fn command(service: Service) -> GitCommand {
    GitCommand { service, owner: "alice".to_owned(), repo: "project".to_owned() }
}

/// Lookups answering from fixed outcomes and recording the order of their calls.
#[derive(Debug, Default)]
struct Port {
    suspended: bool,
    found: bool,
    writable: bool,
    /// `None`: the mirror lookup must not happen.
    mirror: Option<bool>,
    calls: Mutex<Vec<&'static str>>,
}

impl Port {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl SshRepositories for Port {
    type Repository = ();

    async fn is_suspended(&self, _user: AccountId) -> Result<bool, ServiceError> {
        self.calls.lock().unwrap().push("is_suspended");
        Ok(self.suspended)
    }

    async fn open_for_user(&self, _user: &User, _owner: &str, _name: &str) -> Result<(), ServiceError> {
        self.calls.lock().unwrap().push("open_for_user");
        if self.found { Ok(()) } else { Err(ServiceError::NotFound) }
    }

    async fn open_for_deploy_key(&self, _identity: &SshIdentity, _owner: &str, _name: &str) -> Result<(), ServiceError> {
        self.calls.lock().unwrap().push("open_for_deploy_key");
        Err(ServiceError::NotFound)
    }

    fn can_write(&self, (): &()) -> bool {
        self.calls.lock().unwrap().push("can_write");
        self.writable
    }

    async fn is_mirror(&self, (): &()) -> Result<bool, ServiceError> {
        self.calls.lock().unwrap().push("is_mirror");
        Ok(self.mirror.expect("mirror lookup"))
    }
}

#[tokio::test]
async fn suspension_is_checked_before_the_repository_opens() {
    let port = Port { suspended: true, found: true, ..Port::default() };
    let refused = authorize(&port, &identity(1, 5), &command(Service::UploadPack), true).await;
    assert_eq!(refused, Err("this account has been suspended".to_owned()));
    assert_eq!(port.calls(), ["is_suspended"]);
}

#[tokio::test]
async fn missing_repositories_name_what_was_asked_for() {
    let port = Port::default();
    let refused = authorize(&port, &identity(1, 5), &command(Service::UploadPack), true).await;
    assert_eq!(refused, Err("repository alice/project not found, or you do not have access to it".to_owned()));
}

#[tokio::test]
async fn fetches_need_no_write_or_mirror_check() {
    let port = Port { found: true, mirror: Some(true), ..Port::default() };
    assert_eq!(authorize(&port, &identity(1, 5), &command(Service::UploadPack), true).await, Ok(()));
    assert_eq!(port.calls(), ["is_suspended", "open_for_user"]);
}

#[tokio::test]
async fn pushes_check_write_access_then_mirrors() {
    let reader = Port { found: true, mirror: Some(true), ..Port::default() };
    let refused = authorize(&reader, &identity(1, 5), &command(Service::ReceivePack), true).await;
    assert_eq!(refused, Err("you do not have permission to push to alice/project".to_owned()));
    assert!(!reader.calls().contains(&"is_mirror"));

    // Left to the push gate: the mirror is still refused before git starts.
    let deferred = Port { found: true, mirror: Some(true), ..Port::default() };
    let refused = authorize(&deferred, &identity(1, 5), &command(Service::ReceivePack), false).await;
    assert_eq!(refused, Err(rbit_services::mirrors::PUSH_REFUSED.to_owned()));
    assert_eq!(deferred.calls(), ["is_suspended", "open_for_user", "is_mirror"]);

    let writer = Port { found: true, writable: true, mirror: Some(false), ..Port::default() };
    assert_eq!(authorize(&writer, &identity(1, 5), &command(Service::ReceivePack), true).await, Ok(()));
}

#[test]
fn read_only_deploy_keys_name_the_repository() {
    assert_eq!(read_only_deploy_key("alice/project"), "this deploy key is read-only; it cannot push to alice/project");
}

#[test]
fn keys_stay_with_their_holder() {
    assert!(same_key_holder(&identity(1, 5), &identity(1, 5)));
    assert!(!same_key_holder(&identity(1, 5), &identity(1, 6)), "a re-registered key belongs to someone else");
    assert!(!same_key_holder(&identity(1, 5), &identity(2, 5)));
}
