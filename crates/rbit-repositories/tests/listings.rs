//! Listing viewer facts, limits and query parsing without SQL.
#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::AccountId;
use rbit_repositories::{
    ArchivedFilter, OwnedRepository, RepoSort,
    discovery::{DASHBOARD_LIMIT, RepoSearch, RepositoryListingCatalog, RepositoryListings},
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
struct Catalog {
    calls: Mutex<Vec<String>>,
}
impl Catalog {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}
impl RepositoryListingCatalog for Catalog {
    type Error = &'static str;
    fn owned_by(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("owned:{}:{viewer:?}:{admin}", owner.0));
        ready(Ok(vec![models::repository(1)]))
    }
    fn for_member(&self, member: AccountId, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("member:{}:{limit}", member.0));
        ready(Ok(Vec::new()))
    }
    fn count_for_member(&self, member: AccountId) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.log(format!("count:{}", member.0));
        ready(Ok(42))
    }
    fn page_for_member(
        &self,
        member: AccountId,
        limit: u32,
        offset: u32,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("page:{}:{limit}:{offset}", member.0));
        ready(Ok(Vec::new()))
    }
    fn recent_public(&self, limit: u32) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.log(format!("recent:{limit}"));
        ready(Ok(Vec::new()))
    }
}
#[test]
fn listings_pass_viewer_facts_and_limits_to_one_statement_each() {
    let catalog = Catalog::default();
    let listings = RepositoryListings::new(&catalog);
    let mut admin = models::user();
    admin.is_admin = true;
    let owner = models::user().account;
    assert_eq!(run(listings.for_owner(None, &owner)).unwrap().len(), 1);
    run(listings.for_owner(Some(&admin), &owner)).unwrap();
    run(listings.all_for_owner(AccountId(5))).unwrap();
    run(listings.dashboard(&admin)).unwrap();
    assert_eq!(run(listings.member_page(&admin, 10, 20)).unwrap().1, 42);
    run(listings.recent_public(8)).unwrap();
    assert_eq!(DASHBOARD_LIMIT, 100);
    assert_eq!(
        *catalog.calls.lock().unwrap(),
        [
            "owned:1:None:false",
            "owned:1:Some(AccountId(1)):true",
            "owned:5:None:true",
            "member:1:100",
            "count:1",
            "page:1:10:20",
            "recent:8",
        ]
    );
}
#[test]
fn explore_query_values_fall_back_leniently() {
    let search = RepoSearch::from_query(Some("  rust  "), Some("name"), Some("true"), Some("3"));
    assert_eq!((search.text.as_str(), search.sort, search.archived, search.page), ("rust", RepoSort::Name, ArchivedFilter::Only, 3));
    let fallback = RepoSearch::from_query(None, Some("stars"), Some("maybe"), Some("two"));
    assert_eq!(
        (fallback.text.as_str(), fallback.sort, fallback.archived, fallback.page),
        ("", RepoSort::default(), ArchivedFilter::default(), 1)
    );
    assert_eq!(RepoSearch::from_query(None, None, None, Some("0")).page, 0, "page 0 is clamped later by discovery");
}
