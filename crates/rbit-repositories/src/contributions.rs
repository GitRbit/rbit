//! Visibility-filtered contribution counting over default-branch history.
//!
//! Rules:
//! * An organization's calendar counts every commit on the default branches of its
//!   repositories that the viewer may read.
//! * A user's calendar scans every repository the viewer may read, but counts only
//!   commits authored with the user's noreply address, which is public. The account
//!   email is private: matching it for anyone else would let a viewer confirm a guessed
//!   address by committing with it to a repository they can read, so it counts only
//!   when the user or a site administrator is looking. Addresses compare ignoring ASCII
//!   case.
//! * A commit reachable from several repositories counts once; a repository counts
//!   when at least one of its commits in the window counted.
//! * Days are Unix days (UTC, Euclidean, so pre-1970 seconds fall on negative days); the
//!   window is half-open, `start..end`, in seconds.
//!
//! History scanning, caching and worker admission stay with the caller.

use std::{
    collections::{BTreeMap, HashSet},
    future::Future,
};

use rbit_contracts::AccountId;
use rbit_identity_models::{Account, User};

use crate::OwnedRepository;

const DAY: i64 = 86_400;

/// One commit of a default-branch history: `(commit id, author seconds, author email)`.
pub type HistoryEntry = (String, i64, String);

#[derive(Debug, Default, Clone)]
pub struct Contributions {
    /// Unix day (UTC) to authored commit count.
    pub days: BTreeMap<i64, usize>,
    pub repositories: usize,
    pub partial: bool,
}

/// The existing visibility-filtered repository listings.
pub trait ContributionRepository: Sync {
    type Error: Send;
    fn owned_readable(
        &self,
        owner: AccountId,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
    fn readable(
        &self,
        viewer: Option<AccountId>,
        viewer_is_admin: bool,
    ) -> impl Future<Output = Result<Vec<OwnedRepository>, Self::Error>> + Send;
}

/// The repositories whose histories `account`'s calendar scans for `viewer`: one listing query.
pub async fn repositories<S: ContributionRepository>(
    store: &S,
    viewer: Option<&User>,
    account: &Account,
) -> Result<Vec<OwnedRepository>, S::Error> {
    let (id, admin) = (viewer.map(User::id), viewer.is_some_and(|viewer| viewer.is_admin));
    if account.is_organization() { store.owned_readable(account.id, id, admin).await } else { store.readable(id, admin).await }
}

/// Whether `account`'s calendar filters authors at all; organizations count every commit.
pub fn filters_authors(account: &Account) -> bool {
    !account.is_organization()
}

/// The author addresses that count as `account`'s commits for `viewer`.
pub fn author_emails(viewer: Option<&User>, account: AccountId, noreply: String, private: &str) -> Vec<String> {
    let mut emails = vec![noreply];
    if viewer.is_some_and(|viewer| viewer.id() == account || viewer.is_admin) {
        emails.push(private.to_owned());
    }
    emails
}

/// Accumulates an account calendar across repositories.
#[derive(Debug)]
pub struct AccountTally {
    emails: Option<Vec<String>>,
    start: i64,
    end: i64,
    seen: HashSet<String>,
    result: Contributions,
}

impl AccountTally {
    /// `emails = None` counts every author (organizations).
    pub fn new(emails: Option<Vec<String>>, start: i64, end: i64) -> Self {
        Self { emails, start, end, seen: HashSet::new(), result: Contributions::default() }
    }

    pub fn add(&mut self, history: &[HistoryEntry], partial: bool) {
        self.result.partial |= partial;
        let mut contributed = false;
        for (id, seconds, author) in history {
            if !(self.start..self.end).contains(seconds) {
                continue;
            }
            if self.emails.as_ref().is_some_and(|emails| !emails.iter().any(|email| author.eq_ignore_ascii_case(email))) {
                continue;
            }
            contributed = true;
            if self.seen.insert(id.clone()) {
                *self.result.days.entry(seconds.div_euclid(DAY)).or_default() += 1;
            }
        }
        self.result.repositories += usize::from(contributed);
    }

    pub fn finish(self) -> Contributions {
        self.result
    }
}

/// One repository's calendar: every author, exact second bounds.
pub fn count_days(history: &[HistoryEntry], partial: bool, start: i64, end: i64) -> Contributions {
    let mut result = Contributions { partial, ..Contributions::default() };
    for (_, seconds, _) in history {
        if (start..end).contains(seconds) {
            *result.days.entry(seconds.div_euclid(DAY)).or_default() += 1;
        }
    }
    result.repositories = usize::from(!result.days.is_empty());
    result
}

/// Whole-day windows can be answered from all-time daily totals.
pub fn whole_days(start: i64, end: i64) -> bool {
    start.rem_euclid(DAY) == 0 && end.rem_euclid(DAY) == 0
}

/// The whole-day window `start..end` of all-time daily totals.
pub fn day_window(daily: &Contributions, start: i64, end: i64) -> Contributions {
    let days: BTreeMap<_, _> = if start < end {
        daily.days.range(start.div_euclid(DAY)..end.div_euclid(DAY)).map(|(&day, &count)| (day, count)).collect()
    } else {
        BTreeMap::new()
    };
    Contributions { repositories: usize::from(!days.is_empty()), days, partial: daily.partial }
}
