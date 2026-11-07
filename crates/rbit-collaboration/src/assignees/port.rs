//! One request/store view retains original roster/grant/mutation/effect ordering.

use crate::threads::IssueId;
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::{Account, User};
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssigneeAttachmentKind {
    Assigned,
    Unassigned,
}

pub trait AssigneeAttachmentPort: Sync {
    type Error;
    fn current(&self) -> impl Future<Output = Result<Vec<(IssueId, Account)>, Self::Error>> + Send;
    /// Validate each new ID, in order, as an active user with write access. Existing
    /// assignees are intentionally not revalidated by this compatibility workflow.
    fn resolve_new(&self, ids: &[AccountId]) -> impl Future<Output = Result<Vec<User>, Self::Error>> + Send;
    fn remove(&self, id: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn add(&self, id: AccountId, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn record(&self, kind: AssigneeAttachmentKind, id: AccountId) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn publish(&self, kind: AssigneeAttachmentKind, id: AccountId);
}
