//! Contribution selection, private-email rule and tallying without Git or SQL.
#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::AccountId;
use rbit_identity_models::{AccountKind, User};
use rbit_repositories::{
    OwnedRepository,
    contributions::{
        AccountTally, ContributionRepository, Contributions, HistoryEntry, author_emails, count_days, day_window, filters_authors,
        repositories, whole_days,
    },
};
use std::{
    collections::BTreeMap,
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
fn entry(id: &str, seconds: i64, author: &str) -> HistoryEntry {
    (id.into(), seconds, author.into())
}
#[derive(Default)]
struct Store {
    calls: Mutex<Vec<String>>,
}
impl ContributionRepository for Store {
    type Error = &'static str;
    fn owned_readable(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.calls.lock().unwrap().push(format!("owned:{}:{viewer:?}:{admin}", owner.0));
        ready(Ok(vec![models::repository(owner.0)]))
    }
    fn readable(&self, viewer: Option<AccountId>, admin: bool) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send {
        self.calls.lock().unwrap().push(format!("readable:{viewer:?}:{admin}"));
        ready(Ok(Vec::new()))
    }
}
fn viewer(id: i64, admin: bool) -> User {
    let mut user = models::user();
    user.account.id = AccountId(id);
    user.is_admin = admin;
    user
}
#[test]
fn organizations_list_their_readable_repositories_and_users_every_readable_one() {
    let store = Store::default();
    let mut org = models::user().account;
    org.kind = AccountKind::Organization;
    assert_eq!(run(repositories(&store, Some(&viewer(7, true)), &org)).unwrap().len(), 1);
    run(repositories(&store, None, &models::user().account)).unwrap();
    assert_eq!(*store.calls.lock().unwrap(), ["owned:1:Some(AccountId(7)):true", "readable:None:false"]);
    assert!(!filters_authors(&org));
    assert!(filters_authors(&models::user().account));
}
#[test]
fn the_private_email_counts_only_for_the_account_itself_or_an_administrator() {
    let noreply = || "1+alice@noreply.test".to_owned();
    assert_eq!(author_emails(None, AccountId(1), noreply(), "alice@private.test"), ["1+alice@noreply.test"]);
    assert_eq!(author_emails(Some(&viewer(2, false)), AccountId(1), noreply(), "alice@private.test"), ["1+alice@noreply.test"]);
    for allowed in [viewer(1, false), viewer(2, true)] {
        assert_eq!(
            author_emails(Some(&allowed), AccountId(1), noreply(), "alice@private.test"),
            ["1+alice@noreply.test", "alice@private.test"]
        );
    }
}
#[test]
fn account_tally_filters_authors_counts_shared_commits_once_and_marks_partial() {
    let emails = Some(vec!["a@x.test".to_owned()]);
    let mut tally = AccountTally::new(emails, 0, 2 * 86_400);
    tally.add(&[entry("c1", 10, "A@X.test"), entry("c2", 20, "other@x.test"), entry("c3", 2 * 86_400, "a@x.test")], false);
    tally.add(&[entry("c1", 10, "a@x.test"), entry("c4", 86_400, "a@x.test")], true);
    tally.add(&[entry("c5", 30, "other@x.test")], false);
    let result = tally.finish();
    assert_eq!(result.days, BTreeMap::from([(0, 1), (1, 1)]));
    assert_eq!(result.repositories, 2, "the third repository had no counted commit");
    assert!(result.partial);
    let mut all = AccountTally::new(None, i64::MIN, i64::MAX);
    all.add(&[entry("c5", -1, "other@x.test")], false);
    assert_eq!(all.finish().days, BTreeMap::from([(-1, 1)]));
}
#[test]
fn daily_counts_keep_second_bounds_negative_dates_and_partial_status() {
    let records = [entry("a", -1, "a@b"), entry("b", 0, "a@b"), entry("c", 86_400, "a@b")];
    let all = count_days(&records, true, i64::MIN, i64::MAX);
    assert_eq!(all.days, BTreeMap::from([(-1, 1), (0, 1), (1, 1)]));
    assert!(all.partial);
    assert_eq!(count_days(&records, true, -1, 86_400).days, BTreeMap::from([(-1, 1), (0, 1)]));
    assert_eq!(count_days(&records, true, 1, 86_400).repositories, 0);
}
#[test]
fn whole_day_windows_slice_daily_totals() {
    assert!(whole_days(0, 86_400));
    assert!(whole_days(-86_400, 0));
    assert!(!whole_days(1, 86_400));
    let daily = Contributions { days: BTreeMap::from([(-1, 2), (0, 1), (3, 4)]), repositories: 1, partial: true };
    let window = day_window(&daily, -86_400, 86_400);
    assert_eq!(window.days, BTreeMap::from([(-1, 2), (0, 1)]));
    assert_eq!((window.repositories, window.partial), (1, true));
    let empty = day_window(&daily, 86_400, 0);
    assert_eq!((empty.days.len(), empty.repositories), (0, 0));
}
