//! Static adapter for `rbit_collaboration::pulls::ReviewRequests`: accounts by name,
//! their current read access to the pull request's repository, and existing requests.

use rbit_collaboration::pulls::{ReviewRequestError, ReviewRequestPort};
use rbit_core::{Account, AccountId};

use super::{PullContext, PullService};
use crate::ServiceError;

pub(super) fn request_error(error: ReviewRequestError<ServiceError>) -> ServiceError {
    match error {
        ReviewRequestError::Port(error) => error,
        ReviewRequestError::Invalid(message) => ServiceError::Invalid(message),
    }
}

/// Lookups for one pull request.
pub(super) struct RequestView<'a> {
    pub service: &'a PullService,
    pub ctx: &'a PullContext,
}

impl ReviewRequestPort for RequestView<'_> {
    type Error = ServiceError;

    async fn account_named(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        Ok(self.service.ctx().database.accounts().find_account_by_name(name).await?)
    }

    async fn can_read(&self, account: AccountId) -> Result<bool, Self::Error> {
        Ok(self.service.access_of(account, &self.ctx.repo().repo).await?.can_read())
    }

    async fn requested(&self) -> Result<Vec<AccountId>, Self::Error> {
        Ok(self.service.ctx().database.reviews().requested(self.ctx.issue().id).await?)
    }
}
