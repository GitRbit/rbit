//! Static label port adapter; SQL bodies and transaction boundaries remain authoritative.

use rbit_collaboration::labels::{Label, LabelFields, LabelId, LabelRepository, LabelUsage};
use rbit_core::{RepoId, Timestamp};

use crate::{StorageError, sqlite::issue_meta::IssueMetaStore};

impl LabelRepository for IssueMetaStore {
    type Error = StorageError;

    async fn labels(&self, repo: RepoId) -> Result<Vec<Label>, Self::Error> {
        Self::labels(self, repo).await
    }

    async fn label_usage(&self, repo: RepoId, kind: &str) -> Result<Vec<LabelUsage>, Self::Error> {
        Self::label_usage(self, repo, kind).await
    }

    async fn find_label_by_name(&self, repo: RepoId, name: &str) -> Result<Option<Label>, Self::Error> {
        Self::find_label_by_name(self, repo, name).await
    }

    async fn labels_named(&self, repo: RepoId, names: &[String]) -> Result<Vec<Label>, Self::Error> {
        Self::labels_named(self, repo, names).await
    }

    async fn count_labels(&self, repo: RepoId) -> Result<u64, Self::Error> {
        Self::count_labels(self, repo).await
    }

    async fn create_label(&self, repo: RepoId, fields: &LabelFields, at: Timestamp) -> Result<Label, Self::Error> {
        Self::create_label(self, repo, fields, at).await
    }

    async fn update_label(&self, repo: RepoId, id: LabelId, fields: &LabelFields, at: Timestamp) -> Result<Option<Label>, Self::Error> {
        Self::update_label(self, repo, id, fields, at).await
    }

    async fn delete_label(&self, repo: RepoId, id: LabelId) -> Result<Option<Label>, Self::Error> {
        Self::delete_label(self, repo, id).await
    }

    async fn create_missing_labels(&self, repo: RepoId, labels: &[LabelFields], at: Timestamp) -> Result<u64, Self::Error> {
        Self::create_missing_labels(self, repo, labels, at).await
    }
}
