//! Read gate, store work order and listing bounds without SQL or a runtime.
#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::{AccountId, RepoId};
use rbit_repositories::{
    OwnedRepository,
    stars::{MAX_LISTED, RepositoryStars, StarError, StarRepository, StarSummary, Stargazer},
};
use std::{
    collections::HashMap,
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
struct Store {
    calls: Mutex<Vec<String>>,
}
impl Store {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl StarRepository for Store {
    type Error = &'static str;
    fn summary(&self, repo: RepoId, viewer: Option<AccountId>) -> impl Future<Output = Result<StarSummary, Self::Error>> + Send {
        self.log(format!("summary:{}:{viewer:?}", repo.0));
        ready(Ok(StarSummary { count: 3, starred_by_viewer: viewer.is_some() }))
    }
    fn counts(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashMap<RepoId, i64>, Self::Error>> + Send {
        self.log(format!("counts:{repos:?}"));
        ready(Ok(HashMap::new()))
    }
    fn star(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log(format!("star:{}:{}", account.0, repo.0));
        ready(Ok(()))
    }
    fn unstar(&self, account: AccountId, repo: RepoId) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.log(format!("unstar:{}:{}", account.0, repo.0));
        ready(Ok(()))
    }
    fn stargazers(&self, repo: RepoId, limit: u32) -> impl Future<Output = Result<Vec<Stargazer>, Self::Error>> + Send {
        self.log(format!("stargazers:{}:{limit}", repo.0));
        ready(Ok(Vec::new()))
    }
    fn starred_by(
        &self,
        account: AccountId,
        viewer: Option<AccountId>,
        admin: bool,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("starred_by:{}:{viewer:?}:{admin}:{limit}", account.0));
        ready(Ok(Vec::new()))
    }
}
#[test]
fn unreadable_repositories_do_no_store_work() {
    let (store, user, repo) = (Store::default(), models::user(), models::repository(9));
    let stars = RepositoryStars::new(&store);
    assert!(matches!(run(stars.star(&user, &repo, false)), Err(StarError::NotFound)));
    assert!(matches!(run(stars.unstar(&user, &repo, false)), Err(StarError::NotFound)));
    assert!(matches!(run(stars.stargazers(&repo, false)), Err(StarError::NotFound)));
    drop(stars.star(&user, &repo, true));
    assert_eq!(store.trace(), Vec::<String>::new());
}
#[test]
fn commands_write_then_answer_with_the_viewer_summary() {
    let (store, user, repo) = (Store::default(), models::user(), models::repository(9));
    let stars = RepositoryStars::new(&store);
    assert!(run(stars.star(&user, &repo, true)).unwrap().starred_by_viewer);
    run(stars.unstar(&user, &repo, true)).unwrap();
    assert_eq!(store.trace(), ["star:1:9", "summary:9:Some(AccountId(1))", "unstar:1:9", "summary:9:Some(AccountId(1))"]);
}
#[test]
fn listings_are_bounded_and_viewer_filtered_in_one_query() {
    let (store, user, repo) = (Store::default(), models::user(), models::repository(9));
    let stars = RepositoryStars::new(&store);
    run(stars.stargazers(&repo, true)).unwrap();
    run(stars.starred_by(None, &user.account)).unwrap();
    let mut admin = user.clone();
    admin.is_admin = true;
    run(stars.starred_by(Some(&admin), &user.account)).unwrap();
    run(stars.counts(&[models::repository(4), models::repository(5)])).unwrap();
    run(stars.summary(RepoId(9), None)).unwrap();
    assert_eq!(MAX_LISTED, 500);
    assert_eq!(
        store.trace(),
        [
            "stargazers:9:500",
            "starred_by:1:None:false:500",
            "starred_by:1:Some(AccountId(1)):true:500",
            "counts:[RepoId(4), RepoId(5)]",
            "summary:9:None",
        ]
    );
}
