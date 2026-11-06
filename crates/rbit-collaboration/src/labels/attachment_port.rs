//! Static request scope and mutation view preserve existing handle/query timing.

use super::{Label, LabelId, LabelSnapshot};
use crate::threads::IssueId;
use rbit_contracts::Timestamp;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelAttachmentKind {
    Labeled,
    Unlabeled,
}

pub trait LabelAttachmentPort: Sync {
    type Error;
    type Mutation: LabelAttachmentMutation<Error = Self::Error>;
    /// Resolve each supplied, already-distinct ID in input order and reject foreign IDs.
    fn resolve(&self, ids: &[LabelId]) -> impl Future<Output = Result<Vec<Label>, Self::Error>> + Send;
    /// Create the original single store view only after successful add resolution.
    fn mutation(&self) -> Self::Mutation;
}

pub trait LabelAttachmentMutation: Sync {
    type Error;
    fn current(&self) -> impl Future<Output = Result<Vec<(IssueId, LabelSnapshot)>, Self::Error>> + Send;
    fn remove(&self, id: LabelId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn add(&self, id: LabelId, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn record(&self, kind: LabelAttachmentKind, label: &LabelSnapshot) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn publish(&self, kind: LabelAttachmentKind, label: LabelSnapshot);
}
