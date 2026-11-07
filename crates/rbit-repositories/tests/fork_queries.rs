#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::{AccountId, RepoId};
use rbit_repositories::{
    OwnedRepository, Visibility,
    forks::{ForkParentAccess, ForkParentError, ForkQueries, ForkReadRepository, ForkViewer},
};
use std::{
    collections::HashSet,
    future::{Future, ready},
    sync::Mutex,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
#[derive(Default)]
struct ParentAccess {
    allowed: bool,
    error: bool,
}
#[derive(Default)]
struct Store {
    calls: Mutex<Vec<String>>,
    chain: i64,
    cycle: bool,
    private_at: Option<i64>,
    fail: bool,
    access: ParentAccess,
}
impl Store {
    fn log(&self, value: impl Into<String>) {
        self.calls.lock().unwrap().push(value.into());
    }
    fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl ForkReadRepository for Store {
    type Error = &'static str;
    fn parent_of(&self, repo: RepoId) -> impl Future<Output = Result<Option<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("parent:{}", repo.0));
        let next = if self.cycle { Some(repo.0) } else { (repo.0 < self.chain).then_some(repo.0 + 1) };
        ready(if self.fail {
            Err("store")
        } else {
            Ok(next.map(|id| {
                let mut repo = models::repository(id);
                repo.repo.visibility = if self.private_at == Some(id) { Visibility::Private } else { Visibility::Public };
                repo
            }))
        })
    }
    fn forks_among(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashSet<RepoId>, Self::Error>> + Send {
        self.log(format!("flags:{repos:?}"));
        ready(Ok(repos.iter().copied().collect()))
    }
    fn count_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.log(format!("count:{}:{viewer:?}:{admin}", parent.0));
        ready(Ok(7))
    }
    fn list_readable(
        &self,
        parent: RepoId,
        viewer: Option<AccountId>,
        admin: bool,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("list:{}:{viewer:?}:{admin}:{limit}:{offset}", parent.0));
        ready(if self.fail { Err("list") } else { Ok(Vec::new()) })
    }
}
impl ForkParentAccess for Store {
    type Error = &'static str;
    fn can_open(&self, _: &OwnedRepository) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.log("open");
        ready(if self.access.error { Err("access") } else { Ok(self.access.allowed) })
    }
}
#[test]
fn parent_public_fast_path_private_suppression_and_errors_match() {
    let public = Store { chain: 1, ..Store::default() };
    assert!(run(ForkQueries::new(&public).parent(RepoId(0), &public)).unwrap().is_some());
    assert_eq!(public.trace(), ["parent:0"]);
    for visible in [false, true] {
        let private = Store {
            chain: 1,
            private_at: Some(1),
            access: ParentAccess { allowed: visible, ..ParentAccess::default() },
            ..Store::default()
        };
        assert_eq!(run(ForkQueries::new(&private).parent(RepoId(0), &private)).unwrap().is_some(), visible);
        assert_eq!(private.trace(), ["parent:0", "open"]);
    }
    let bad = Store { chain: 1, private_at: Some(1), access: ParentAccess { error: true, ..ParentAccess::default() }, ..Store::default() };
    assert!(matches!(run(ForkQueries::new(&bad).parent(RepoId(0), &bad)), Err(ForkParentError::Access("access"))));
    let missing = Store::default();
    assert!(run(ForkQueries::new(&missing).parent(RepoId(0), &missing)).unwrap().is_none());
    assert_eq!(missing.trace(), ["parent:0"]);
}
#[test]
fn page_and_default_list_keep_raw_limit_offsets_viewer_and_sequential_count() {
    let store = Store::default();
    let viewer = ForkViewer { account: Some(AccountId(8)), is_admin: true };
    let app = ForkQueries::new(&store);
    assert_eq!(run(app.page(viewer, RepoId(4), 0, u32::MAX)).unwrap().1, 7);
    assert_eq!(store.trace(), [format!("list:4:Some(AccountId(8)):true:0:{}", u32::MAX), "count:4:Some(AccountId(8)):true".into()]);
    store.calls.lock().unwrap().clear();
    run(app.list(ForkViewer { account: None, is_admin: false }, RepoId(4))).unwrap();
    assert_eq!(store.trace(), ["list:4:None:false:100:0", "count:4:None:false"]);
    let fail = Store { fail: true, ..Store::default() };
    assert!(run(ForkQueries::new(&fail).page(viewer, RepoId(4), 1, 0)).is_err());
    assert_eq!(fail.trace(), ["list:4:Some(AccountId(8)):true:1:0"]);
}
#[test]
fn ancestry_preserves_deleted_parent_private_cycle_and_exact_64_lookup_boundary() {
    for (chain, expected, lookups) in [(0, false, 1), (63, false, 64), (64, true, 64), (1000, true, 64)] {
        let store = Store { chain, ..Store::default() };
        assert_eq!(run(ForkQueries::new(&store).has_private_ancestor(RepoId(0))).unwrap(), expected);
        assert_eq!(store.trace().len(), lookups);
    }
    let private = Store { chain: 9, private_at: Some(2), ..Store::default() };
    assert!(run(ForkQueries::new(&private).has_private_ancestor(RepoId(0))).unwrap());
    assert_eq!(private.trace(), ["parent:0", "parent:1"]);
    let cycle = Store { cycle: true, ..Store::default() };
    assert!(run(ForkQueries::new(&cycle).has_private_ancestor(RepoId(0))).unwrap());
    assert_eq!(cycle.trace(), ["parent:0"]);
    let fail = Store { fail: true, ..Store::default() };
    assert_eq!(run(ForkQueries::new(&fail).has_private_ancestor(RepoId(0))), Err("store"));
}
#[test]
fn fork_flags_and_count_use_only_the_existing_store_operations() {
    let store = Store::default();
    let app = ForkQueries::new(&store);
    assert!(run(app.is_fork(RepoId(4))).unwrap());
    assert_eq!(run(app.count(ForkViewer { account: None, is_admin: false }, RepoId(4))).unwrap(), 7);
    assert_eq!(store.trace(), ["flags:[RepoId(4)]", "count:4:None:false"]);
    assert_eq!(format!("{app:?}"), "ForkQueries { .. }");
}
