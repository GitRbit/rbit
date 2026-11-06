//! Persistence owns SQL constraints, ordering, transactions and affected-row identity.

use std::future::Future;

use rbit_contracts::{RepoId, Timestamp};

use super::{Label, LabelFields, LabelId, LabelUsage};

/// No method requires a boxed future, task or a second policy lookup.
/// Count and insertion remain separate statements: this is not a snapshot or quota lock.
pub trait LabelRepository: Sync {
    type Error;

    fn labels(&self, repo: RepoId) -> impl Future<Output = Result<Vec<Label>, Self::Error>> + Send;
    fn label_usage(&self, repo: RepoId, kind: &str) -> impl Future<Output = Result<Vec<LabelUsage>, Self::Error>> + Send;
    fn find_label_by_name(&self, repo: RepoId, name: &str) -> impl Future<Output = Result<Option<Label>, Self::Error>> + Send;
    fn labels_named(&self, repo: RepoId, names: &[String]) -> impl Future<Output = Result<Vec<Label>, Self::Error>> + Send;
    fn count_labels(&self, repo: RepoId) -> impl Future<Output = Result<u64, Self::Error>> + Send;
    fn create_label(&self, repo: RepoId, fields: &LabelFields, at: Timestamp) -> impl Future<Output = Result<Label, Self::Error>> + Send;
    fn update_label(
        &self,
        repo: RepoId,
        id: LabelId,
        fields: &LabelFields,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Label>, Self::Error>> + Send;
    fn delete_label(&self, repo: RepoId, id: LabelId) -> impl Future<Output = Result<Option<Label>, Self::Error>> + Send;
    fn create_missing_labels(
        &self,
        repo: RepoId,
        labels: &[LabelFields],
        at: Timestamp,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}
