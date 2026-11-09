//! Who may be asked to review a pull request: named people who can read the repository,
//! never its author, at most [`MAX_REQUESTED_REVIEWERS`](super::review_threads::MAX_REQUESTED_REVIEWERS) at once.

use std::future::Future;

use rbit_contracts::AccountId;
use rbit_identity_models::{Account, AccountKind};

use super::review_threads::{AUTHOR_REVIEW_REFUSAL, requests_fit, reviewer_refusal};

/// Account and access lookups for one pull request.
pub trait ReviewRequestPort: Sync {
    type Error;

    fn account_named(&self, name: &str) -> impl Future<Output = Result<Option<Account>, Self::Error>> + Send;
    /// Whether `account` can read the pull request's repository now.
    fn can_read(&self, account: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// The people already asked, in request order.
    fn requested(&self) -> impl Future<Output = Result<Vec<AccountId>, Self::Error>> + Send;
}

/// Refusals keep the legacy service's text; lookups' errors pass through.
#[derive(Debug)]
pub enum ReviewRequestError<E> {
    Port(E),
    Invalid(String),
}

/// A validated request: everyone named, and those not asked yet (to record).
#[derive(Debug, Clone)]
pub struct ReviewRequestPlan {
    pub accounts: Vec<Account>,
    pub new: Vec<AccountId>,
}

/// Whether `actor` may ask for or withdraw reviews: triagers, or the author while they
/// may comment.
pub const fn may_request_reviews(can_triage: bool, is_author: bool, can_comment: bool) -> bool {
    can_triage || (is_author && can_comment)
}

/// The names a request lists: trimmed, blanks skipped.
pub fn requested_names(names: &[String]) -> impl Iterator<Item = &str> {
    names.iter().map(|name| name.trim()).filter(|name| !name.is_empty())
}

/// Review request decisions over a port for a pull request by `author`.
#[derive(Debug)]
pub struct ReviewRequests<'a, P> {
    port: &'a P,
    author: Option<AccountId>,
}

impl<'a, P: ReviewRequestPort> ReviewRequests<'a, P> {
    pub fn new(port: &'a P, author: Option<AccountId>) -> Self {
        Self { port, author }
    }

    /// Checks every name in order, refusing the whole request on the first unknown
    /// name, organisation, person who cannot read the repository (the same answer for
    /// each) or the author; then the total against the request limit.
    pub async fn plan(&self, names: &[String]) -> Result<ReviewRequestPlan, ReviewRequestError<P::Error>> {
        let mut accounts = Vec::new();
        for name in requested_names(names) {
            let account = self.port.account_named(name).await.map_err(ReviewRequestError::Port)?;
            let account = match account {
                Some(account)
                    if account.kind == AccountKind::User && self.port.can_read(account.id).await.map_err(ReviewRequestError::Port)? =>
                {
                    account
                }
                _ => return Err(ReviewRequestError::Invalid(reviewer_refusal(name))),
            };
            if Some(account.id) == self.author {
                return Err(ReviewRequestError::Invalid(AUTHOR_REVIEW_REFUSAL.to_owned()));
            }
            accounts.push(account);
        }
        let existing = self.port.requested().await.map_err(ReviewRequestError::Port)?;
        let new: Vec<AccountId> = accounts.iter().map(|account| account.id).filter(|id| !existing.contains(id)).collect();
        requests_fit(existing.len(), new.len()).map_err(ReviewRequestError::Invalid)?;
        Ok(ReviewRequestPlan { accounts, new })
    }

    /// Withdraws the request of each person `names` lists, in order, with `remove`;
    /// unknown names are skipped.
    pub async fn withdraw<Fut>(&self, names: &[String], mut remove: impl FnMut(AccountId) -> Fut) -> Result<(), P::Error>
    where
        Fut: Future<Output = Result<(), P::Error>>,
    {
        for name in requested_names(names) {
            if let Some(account) = self.port.account_named(name).await? {
                remove(account.id).await?;
            }
        }
        Ok(())
    }
}
