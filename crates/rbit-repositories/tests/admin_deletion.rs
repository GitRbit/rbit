//! Administrator deletion order and failure handling without SQL or disk.
#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::{AccountId, RepoId};
use rbit_identity_models::User;
use rbit_repositories::{
    OwnedRepository,
    settings::{AdminDeletionError, AdminDeletionPort, AdminRepositoryDeletion},
};
use std::{
    future::{Future, ready},
    sync::Mutex,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
#[derive(Default)]
struct Port {
    gone: bool,
    disk_fails: bool,
    calls: Mutex<Vec<String>>,
}
impl Port {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl AdminDeletionPort for Port {
    type Error = &'static str;
    type DiskError = &'static str;
    fn delete_metadata(&self, repo: RepoId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.log(format!("metadata:{}", repo.0));
        ready(Ok(!self.gone))
    }
    fn delete_disk(&self, owner: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::DiskError>> + Send {
        self.log(format!("disk:{}:{}", owner.0, repo.0));
        ready(if self.disk_fails { Err("disk") } else { Ok(()) })
    }
    fn disk_failed(&self, error: &Self::DiskError, _: AccountId, _: RepoId) {
        self.log(format!("logged:{error}"));
    }
    fn deleted(&self, _: &User, repo: &OwnedRepository) -> impl Future<Output = ()> + Send {
        self.log(format!("published:{}", repo.full_name()));
        ready(())
    }
}
#[test]
fn confirmation_precedes_any_work_and_metadata_precedes_disk() {
    let (actor, repo) = (models::user(), models::repository(9));
    let port = Port::default();
    let refused = run(AdminRepositoryDeletion::delete(&port, &actor, &repo, "alice/other"));
    assert!(
        matches!(refused, Err(AdminDeletionError::Invalid(message)) if message == "type the repository's full name exactly to confirm")
    );
    assert_eq!(port.trace(), Vec::<String>::new());
    run(AdminRepositoryDeletion::delete(&port, &actor, &repo, " alice/source ")).unwrap();
    assert_eq!(port.trace(), ["metadata:9", "disk:1:9", "published:alice/source"]);
}
#[test]
fn a_missing_row_is_not_found_and_a_disk_failure_is_only_logged() {
    let (actor, repo) = (models::user(), models::repository(9));
    let gone = Port { gone: true, ..Port::default() };
    assert!(matches!(run(AdminRepositoryDeletion::delete(&gone, &actor, &repo, "alice/source")), Err(AdminDeletionError::NotFound)));
    assert_eq!(gone.trace(), ["metadata:9"]);
    let failing = Port { disk_fails: true, ..Port::default() };
    run(AdminRepositoryDeletion::delete(&failing, &actor, &repo, "alice/source")).unwrap();
    assert_eq!(failing.trace(), ["metadata:9", "disk:1:9", "logged:disk", "published:alice/source"]);
}
