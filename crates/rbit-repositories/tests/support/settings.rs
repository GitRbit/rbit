//! Static first-poll scripted effects; no runtime dependency or general executor.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{
    OwnedRepository, RepoName, Repository, Visibility,
    settings::{RepoGeneralSettings, RepositorySettingsPort, RepositorySettingsRepository},
};
use std::{
    collections::HashMap,
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
            name: "alice".parse().unwrap(),
            kind: AccountKind::User,
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "alice@example.test".parse().unwrap(),
        is_admin: false,
    }
}
pub fn repo() -> OwnedRepository {
    OwnedRepository {
        owner: user().account,
        repo: Repository {
            id: RepoId(2),
            owner_id: AccountId(1),
            name: "demo".parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            pushed_at: None,
            archived_at: None,
        },
    }
}
#[derive(Debug, Default)]
pub struct Failures {
    pub update: bool,
    pub restore: bool,
    pub disk: bool,
}

#[derive(Debug, Default)]
pub struct Bridge {
    pub log: Mutex<Vec<String>>,
    pub settings: Mutex<Option<RepoGeneralSettings>>,
    pub archived: Mutex<bool>,
    pub publications: Mutex<HashMap<String, bool>>,
    pub private_ancestor: bool,
    pub failures: Failures,
    pub pause_archive_effect: bool,
}
impl Bridge {
    pub fn log(&self, s: impl Into<String>) {
        self.log.lock().unwrap().push(s.into());
    }
    pub fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}
#[derive(Debug)]
pub struct Rows<'a>(&'a Bridge);
impl RepositorySettingsRepository for Rows<'_> {
    type Error = &'static str;
    fn update_general(&self, _: RepoId, settings: &RepoGeneralSettings) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.0.log(format!("update:{}", settings.default_branch));
        if !self.0.failures.update {
            *self.0.settings.lock().unwrap() = Some(settings.clone());
        }
        ready(if self.0.failures.update { Err("update") } else { Ok(()) })
    }
    fn rename(&self, _: RepoId, _: &RepoName) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.0.log("rename");
        ready(Ok(()))
    }
    fn archive(&self, _: RepoId, _: AccountId, _: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.0.log("archive");
        let mut state = self.0.archived.lock().unwrap();
        let changed = !*state;
        *state = true;
        ready(Ok(changed))
    }
    fn unarchive(&self, _: RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.0.log("unarchive");
        let mut state = self.0.archived.lock().unwrap();
        let changed = *state;
        *state = false;
        ready(Ok(changed))
    }
    fn delete(&self, _: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.0.log("delete");
        ready(Ok(()))
    }
}
impl<'a> RepositorySettingsPort for &'a Bridge {
    type Error = &'static str;
    type StoreError = &'static str;
    type Store = Rows<'a>;
    fn store(&self) -> Self::Store {
        self.log("store");
        Rows(self)
    }
    fn has_private_ancestor(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.log("ancestor");
        ready(Ok(self.private_ancestor))
    }
    fn branch_exists(&self, branch: &str) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.log(format!("branch:{branch}"));
        ready(Ok(true))
    }
    fn set_head(&self, branch: &str) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log(format!("head:{branch}"));
        ready(if self.failures.restore && branch == "main" { Err("restore") } else { Ok(()) })
    }
    fn restore_failed(&self, _: &Self::Error) {
        self.log("restore_failed");
    }
    fn updated(&self, _: &RepoGeneralSettings) {
        self.log("updated");
    }
    fn renamed(&self, _: &RepoName) {
        self.log("renamed");
    }
    fn archive_committed(&self, _: &User, archived: bool) -> impl Future<Output = ()> + Send {
        self.log(format!("archive_effect:{archived}"));
        poll_fn(move |_| {
            if self.pause_archive_effect {
                Poll::Pending
            } else {
                self.publications.lock().unwrap().insert("archive".to_owned(), archived);
                Poll::Ready(())
            }
        })
    }
    fn close_reader(&self) {
        self.log("close");
    }
    fn delete_disk(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log("disk");
        ready(if self.failures.disk { Err("disk") } else { Ok(()) })
    }
    fn deleted(&self, _: &User, full_name: String) {
        self.log(format!("deleted:{full_name}"));
    }
}
