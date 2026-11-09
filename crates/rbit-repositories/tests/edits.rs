//! Edit refusal order and the single push-gate update, without Git or SQL.
#[path = "support/fork_models.rs"]
mod models;
use rbit_identity_models::User;
use rbit_repositories::{
    ARCHIVED_READ_ONLY,
    edits::{
        BranchUpdate, CommitInput, CommitTarget, EditAccess, EditPort, EditRefusal, MIRROR_READ_ONLY, RepositoryEdits, admit_upload,
        require_upload,
    },
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
    mirror: bool,
    refusal: Option<&'static str>,
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
impl EditPort for Port {
    type Error = &'static str;
    fn is_mirror(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.log("mirror".into());
        ready(Ok(self.mirror))
    }
    fn branch_tip(&self, branch: &str) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send {
        self.log(format!("tip:{branch}"));
        ready(Ok(matches!(branch, "main" | "taken").then(|| "c1".to_owned())))
    }
    fn valid_branch_name(&self, name: &str) -> bool {
        !name.contains(' ')
    }
    fn push_refusal(&self, _: &User, update: BranchUpdate) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send {
        self.log(format!("gate:{}:{:?}:{}", update.name, update.before, update.after));
        ready(Ok(self.refusal.map(str::to_owned)))
    }
    fn branch_update_refusal(&self, _: &User, branch: &str) -> impl Future<Output = Result<Option<String>, Self::Error>> + Send {
        self.log(format!("direct:{branch}"));
        ready(Ok(self.refusal.map(str::to_owned)))
    }
}
fn input(parent: &str, target: CommitTarget) -> CommitInput {
    CommitInput { branch: "main".into(), parent: parent.into(), target, title: String::new(), description: String::new() }
}
fn writable_repo() -> rbit_repositories::Repository {
    let mut repo = models::repository(9).repo;
    repo.archived_at = None;
    repo
}
#[test]
fn write_access_then_archive_then_mirror_are_checked_first() {
    let (actor, port, mut repo) = (models::user(), Port { mirror: true, ..Port::default() }, writable_repo());
    let check = |repo: &rbit_repositories::Repository, can_write| {
        run(RepositoryEdits::new(&port, repo, can_write).check(&actor, &input("c1", CommitTarget::Branch)))
    };
    assert!(matches!(check(&repo, false), Err(EditRefusal::Forbidden)));
    assert!(matches!(check(&repo, true), Err(EditRefusal::Invalid(message)) if message == MIRROR_READ_ONLY));
    repo.archived_at = models::repository(9).repo.archived_at;
    assert!(matches!(check(&repo, true), Err(EditRefusal::Invalid(message)) if message == ARCHIVED_READ_ONLY));
    assert_eq!(port.trace(), ["mirror"], "archives refuse without the mirror query");
    assert!(!run(RepositoryEdits::new(&port, &writable_repo(), true).can_edit()).unwrap());
}
#[test]
fn branch_target_checks_existence_movement_then_one_keeping_update() {
    let (actor, repo) = (models::user(), writable_repo());
    let port = Port::default();
    let edits = RepositoryEdits::new(&port, &repo, true);
    let mut missing = input("c1", CommitTarget::Branch);
    missing.branch = "gone".into();
    assert!(matches!(run(edits.check(&actor, &missing)), Err(EditRefusal::NotFound)));
    assert!(
        matches!(run(edits.check(&actor, &input("c0", CommitTarget::Branch))), Err(EditRefusal::BranchMoved { current: Some(tip) }) if tip == "c1")
    );
    run(edits.check(&actor, &input("c1", CommitTarget::Branch))).unwrap();
    assert_eq!(port.trace(), ["mirror", "tip:gone", "mirror", "tip:main", "mirror", "tip:main", "gate:refs/heads/main:Some(\"c1\"):c1"]);
    let refused = Port { refusal: Some("protected"), ..Port::default() };
    let edits = RepositoryEdits::new(&refused, &repo, true);
    assert!(
        matches!(run(edits.check(&actor, &input("c1", CommitTarget::Branch))), Err(EditRefusal::Protected(reason)) if reason == "protected")
    );
}
#[test]
fn new_branch_names_are_validated_before_the_creating_update() {
    let (actor, repo) = (models::user(), writable_repo());
    let port = Port { refusal: Some("no new branches"), ..Port::default() };
    let edits = RepositoryEdits::new(&port, &repo, true);
    for (name, message) in [
        ("", "enter a name for the new branch"),
        ("bad name", "bad name is not a valid branch name"),
        ("taken", "a branch called taken already exists"),
        ("fresh", "no new branches"),
    ] {
        let result = run(edits.check(&actor, &input("stale", CommitTarget::NewBranch(name.into()))));
        assert!(matches!(result, Err(EditRefusal::Invalid(text)) if text == message), "{name}");
    }
    assert_eq!(port.trace().last().unwrap(), "gate:refs/heads/fresh:None:c1", "a new branch ignores a moved parent");
}
#[test]
fn access_reports_the_direct_refusal_reason() {
    let (actor, repo) = (models::user(), writable_repo());
    assert_eq!(run(RepositoryEdits::new(&Port::default(), &repo, true).access(&actor, "main")).unwrap(), EditAccess::Direct);
    let port = Port { refusal: Some("needs review"), ..Port::default() };
    assert_eq!(
        run(RepositoryEdits::new(&port, &repo, true).access(&actor, "main")).unwrap(),
        EditAccess::NewBranchOnly("needs review".into())
    );
    assert!(matches!(run(RepositoryEdits::new(&port, &repo, false).access(&actor, "main")), Err(EditRefusal::Forbidden)));
}
#[test]
fn uploads_refuse_duplicates_then_excess_and_empty_commits() {
    assert_eq!(admit_upload(["a.txt"].into_iter(), "a.txt", 1, 1), Err("a.txt was chosen twice".to_owned()));
    assert_eq!(admit_upload(["a.txt"].into_iter(), "b.txt", 1, 1), Err("at most 1 files can be uploaded at once".to_owned()));
    assert_eq!(admit_upload(std::iter::empty(), "b.txt", 0, 1), Ok(()));
    assert_eq!(require_upload(0), Err("choose at least one file to upload".to_owned()));
    assert_eq!(require_upload(2), Ok(()));
}
