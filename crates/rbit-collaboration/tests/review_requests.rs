#![cfg(feature = "applications")]
//! Review request refusals, limits and withdrawal order without persistence.

use std::sync::Mutex;

use rbit_collaboration::pulls::{
    ReviewRequestError, ReviewRequestPort, ReviewRequests, may_request_reviews, review_threads::joins_pending_review,
};
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::{Account, AccountKind};

#[derive(Default)]
struct Directory {
    calls: Mutex<Vec<String>>,
    existing: Vec<AccountId>,
}

impl Directory {
    fn call(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

/// `ada` (1) and `bob` (2) read the repository, `eve` (3) does not, `acme` (4) is an
/// organisation; other names are unknown.
fn known(name: &str) -> Option<Account> {
    let (id, kind) = match name {
        "ada" => (1, AccountKind::User),
        "bob" => (2, AccountKind::User),
        "eve" => (3, AccountKind::User),
        "acme" => (4, AccountKind::Organization),
        _ => return None,
    };
    Some(Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(1) })
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted lookups stay lazy until poll like the real adapter")]
impl ReviewRequestPort for Directory {
    type Error = &'static str;

    async fn account_named(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        self.call(format!("named {name}"));
        Ok(known(name))
    }
    async fn can_read(&self, account: AccountId) -> Result<bool, Self::Error> {
        self.call(format!("read {}", account.0));
        Ok(account.0 != 3)
    }
    async fn requested(&self) -> Result<Vec<AccountId>, Self::Error> {
        self.call("requested".into());
        Ok(self.existing.clone())
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ReviewRequestError<&'static str>>) -> String {
    match result {
        Err(ReviewRequestError::Invalid(message)) => message,
        other => panic!("expected an invalid request, got {other:?}"),
    }
}

#[tokio::test]
async fn requests_are_refused_as_a_whole_with_one_answer() {
    let directory = Directory::default();
    let requests = ReviewRequests::new(&directory, Some(AccountId(2)));
    for name in ["ghost", "acme", "eve"] {
        assert_eq!(invalid(requests.plan(&names(&["ada", name])).await), format!("{name} cannot be asked to review this pull request"));
    }
    assert_eq!(invalid(requests.plan(&names(&["bob"])).await), "the author cannot review their own pull request");
    assert_eq!(
        directory.calls(),
        [
            "named ada",
            "read 1",
            "named ghost",
            "named ada",
            "read 1",
            "named acme",
            "named ada",
            "read 1",
            "named eve",
            "read 3",
            "named bob",
            "read 2"
        ],
        "organisations are refused before any access lookup; nothing is read after a refusal"
    );
}

#[tokio::test]
async fn new_requests_skip_existing_ones_and_respect_the_limit() {
    let directory = Directory { existing: vec![AccountId(1)], ..Directory::default() };
    let plan = ReviewRequests::new(&directory, None).plan(&names(&[" ada ", "", "bob"])).await.unwrap();
    assert_eq!(plan.accounts.iter().map(|account| account.id.0).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(plan.new, [AccountId(2)]);
    let full = Directory { existing: (10..25).map(AccountId).collect(), ..Directory::default() };
    assert_eq!(invalid(ReviewRequests::new(&full, None).plan(&names(&["ada"])).await), "at most 15 reviewers can be requested");
}

#[tokio::test]
async fn withdrawals_run_name_by_name_and_skip_unknown_names() {
    let directory = Directory::default();
    ReviewRequests::new(&directory, None)
        .withdraw(&names(&["ada", "ghost", " bob "]), |id| {
            directory.call(format!("remove {}", id.0));
            async { Ok(()) }
        })
        .await
        .unwrap();
    assert_eq!(directory.calls(), ["named ada", "remove 1", "named ghost", "named bob", "remove 2"]);
}

#[tokio::test]
async fn rules_for_who_requests_and_where_comments_go() {
    assert!(may_request_reviews(true, false, false) && may_request_reviews(false, true, true));
    assert!(!may_request_reviews(false, true, false) && !may_request_reviews(false, false, true));
    let asked = Mutex::new(false);
    let pending = async {
        *asked.lock().unwrap() = true;
        Ok::<_, ()>(false)
    };
    assert_eq!(joins_pending_review(true, pending).await, Ok(true));
    assert!(!*asked.lock().unwrap(), "starting a review never looks for a pending one");
    assert_eq!(joins_pending_review(false, async { Ok::<_, ()>(true) }).await, Ok(true));
    assert_eq!(joins_pending_review(false, async { Ok::<_, ()>(false) }).await, Ok(false));
}
