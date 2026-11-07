//! Scripted static creation port; ordinary effects complete in one poll, disk can be held.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{
    Repository,
    creation::{
        CreateRepository, NewRepository, RepositoryCreationPort, RepositoryCreationRepository, RepositoryMembership, RepositoryOwnerCatalog,
    },
};
use std::{
    collections::VecDeque,
    future::{Future, poll_fn, ready},
    sync::Mutex,
    task::{Context, Poll, Waker},
};
pub fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous fixture unexpectedly pending"),
    }
}
pub fn user() -> User {
    User {
        account: Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "alice@example.test".parse().unwrap(),
        is_admin: false,
    }
}
pub fn input(name: &str) -> CreateRepository {
    CreateRepository { name: name.to_owned(), ..CreateRepository::default() }
}
#[derive(Default)]
pub struct CreationFailures {
    pub disk: bool,
    pub rollback: bool,
    pub grant: bool,
}
pub struct Bridge {
    pub owner: Option<Account>,
    pub roles: Mutex<VecDeque<Option<RepositoryMembership>>>,
    pub row: Mutex<Option<Repository>>,
    pub disk: Mutex<bool>,
    pub calls: Mutex<Vec<String>>,
    pub failures: CreationFailures,
    pub pause_disk: bool,
}
impl Default for Bridge {
    fn default() -> Self {
        Self {
            owner: Some(Account { id: AccountId(3), kind: AccountKind::Organization, name: "acme".parse().unwrap(), ..user().account }),
            roles: Mutex::new(VecDeque::new()),
            row: Mutex::new(None),
            disk: Mutex::new(false),
            calls: Mutex::new(Vec::new()),
            failures: CreationFailures::default(),
            pause_disk: false,
        }
    }
}
impl Bridge {
    fn log(&self, value: impl Into<String>) {
        self.calls.lock().unwrap().push(value.into());
    }
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl RepositoryOwnerCatalog for Bridge {
    type Error = &'static str;
    fn owner_named(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send {
        self.log(format!("owner:{name}"));
        ready(Ok(self.owner.clone()))
    }
    fn membership(&self, _: AccountId, _: AccountId) -> impl Future<Output = Result<Option<RepositoryMembership>, Self::Error>> + Send {
        self.log("membership");
        ready(Ok(self.roles.lock().unwrap().pop_front().unwrap_or(Some(RepositoryMembership::Member))))
    }
}
impl RepositoryCreationRepository for Bridge {
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send {
        self.log("create");
        let row = Repository {
            id: RepoId(2),
            owner_id: new.owner_id,
            name: new.name,
            description: new.description,
            visibility: new.visibility,
            default_branch: new.default_branch,
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            pushed_at: None,
            archived_at: None,
        };
        *self.row.lock().unwrap() = Some(row.clone());
        ready(Ok(row))
    }
    fn delete(&self, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("rollback");
        if !self.failures.rollback {
            self.row.lock().unwrap().take();
        }
        ready(if self.failures.rollback { Err("rollback") } else { Ok(()) })
    }
    fn grant_creator_admin(&self, _: RepoId, _: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("grant_admin");
        ready(if self.failures.grant { Err("grant") } else { Ok(()) })
    }
}
impl RepositoryCreationPort for Bridge {
    type Error = &'static str;
    type Store = Self;
    fn repository(&self) -> &Self::Store {
        self
    }
    fn default_branch(&self) -> &'static str {
        self.log("default_branch");
        "main"
    }
    fn create_disk(&self, _: AccountId, _: RepoId, _: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("disk");
        poll_fn(move |_| {
            if self.pause_disk {
                Poll::Pending
            } else if self.failures.disk {
                Poll::Ready(Err("disk"))
            } else {
                *self.disk.lock().unwrap() = true;
                Poll::Ready(Ok(()))
            }
        })
    }
    fn disk_failed(&self, _: &Self::Error, _: &Repository) {
        self.log("disk_failed");
    }
    fn created(&self, _: &User, _: &Account, _: &Repository) {
        self.log("created");
    }
}
