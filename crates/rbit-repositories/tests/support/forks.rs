use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, User};
use rbit_repositories::{
    OwnedRepository, Repository,
    creation::NewRepository,
    forks::{ForkCreationPort, ForkCreationRepository},
};
use std::{
    future::{Future, poll_fn, ready},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};
pub fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
#[path = "fork_models.rs"]
mod models;
pub use models::{repository, user};
#[derive(Debug, Default)]
pub struct ForkFailures {
    pub owner: bool,
    pub copy: bool,
    pub record: bool,
}
#[derive(Debug, Default)]
pub struct CleanupFailures {
    pub disk: bool,
    pub sql: bool,
}
#[derive(Debug, Default)]
pub struct Bridge {
    pub calls: Mutex<Vec<String>>,
    pub row: Mutex<Option<Repository>>,
    pub failures: ForkFailures,
    pub cleanup: CleanupFailures,
    pub fail_pushed: bool,
    pub pause: AtomicBool,
}
impl Bridge {
    pub fn log(&self, value: impl Into<String>) {
        self.calls.lock().unwrap().push(value.into());
    }
    pub fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl ForkCreationRepository for &Bridge {
    type Error = &'static str;
    fn create(&self, new: NewRepository) -> impl Future<Output = Result<Repository, Self::Error>> + Send {
        self.log("create");
        let mut current = self.row.lock().unwrap();
        let result = if current.is_some() {
            Err("unique")
        } else {
            let repo = Repository {
                id: RepoId(9),
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
            *current = Some(repo.clone());
            Ok(repo)
        };
        ready(result)
    }
    fn delete(&self, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("delete-sql");
        if !self.cleanup.sql {
            self.row.lock().unwrap().take();
        }
        ready(if self.cleanup.sql { Err("delete-sql") } else { Ok(()) })
    }
    fn mark_pushed(&self, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("pushed");
        ready(if self.fail_pushed { Err("pushed") } else { Ok(()) })
    }
}
impl<'a> ForkCreationPort for &'a Bridge {
    type Error = &'static str;
    type CleanupError = &'static str;
    type Store = &'a Bridge;
    fn owner(&self, actor: &User, owner: &str) -> impl Future<Output = Result<Account, Self::Error>> + Send {
        self.log(format!("owner:{owner}"));
        ready(if self.failures.owner { Err("owner") } else { Ok(actor.account.clone()) })
    }
    fn store(&self) -> Self::Store {
        self.log("store");
        self
    }
    fn copy_disk(&self, _: RepoId, _: AccountId, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("copy");
        poll_fn(|_| {
            if self.pause.load(Ordering::SeqCst) {
                Poll::Pending
            } else {
                Poll::Ready(if self.failures.copy { Err("copy") } else { Ok(()) })
            }
        })
    }
    fn record(&self, _: RepoId, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("record");
        ready(if self.failures.record { Err("record") } else { Ok(()) })
    }
    fn delete_disk(&self, _: AccountId, _: RepoId) -> impl Future<Output = Result<(), Self::CleanupError>> + Send {
        self.log("delete-disk");
        ready(if self.cleanup.disk { Err("delete-disk") } else { Ok(()) })
    }
    fn copy_failed(&self, error: &Self::Error, _: &OwnedRepository) {
        self.log(format!("failed:{error}"));
    }
    fn cleanup_failed(&self, error: &Self::CleanupError) {
        self.log(format!("cleanup-failed:{error}"));
    }
    fn pushed_failed(&self, error: &&'static str) {
        self.log(format!("pushed-failed:{error}"));
    }
    fn created(&self, _: &User, _: &OwnedRepository, _: &Account, _: &Repository) {
        self.log("RepositoryForked");
    }
}
