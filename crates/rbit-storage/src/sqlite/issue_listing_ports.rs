//! Static collaboration read ports delegate to the original statements and row decoders.
use super::{issue_meta::IssueMetaStore, issue_search::IssueSearchStore};
use crate::StorageError;
use rbit_collaboration::{
    issues::{IssueMetadataPort, IssueSearchPort, MetadataRows, OneIssueMetadata, SearchFilter},
    labels::LabelSnapshot,
    threads::{Issue, IssueId},
};
impl IssueSearchPort for IssueSearchStore {
    type Error = StorageError;
    async fn state_counts(&self, filter: &SearchFilter) -> Result<(u64, u64), Self::Error> {
        IssueSearchStore::state_counts(self, filter).await
    }
    async fn search(&self, filter: &SearchFilter, limit: u32, offset: u64) -> Result<Vec<Issue>, Self::Error> {
        IssueSearchStore::search(self, filter, limit, offset).await
    }
}
impl IssueMetadataPort for IssueMetaStore {
    type Error = StorageError;
    async fn one(&self, issue: IssueId) -> Result<OneIssueMetadata, Self::Error> {
        self.metadata_of(issue).await
    }
    async fn labels(&self, issues: &[IssueId]) -> Result<Vec<(IssueId, LabelSnapshot)>, Self::Error> {
        self.labels_of(issues).await
    }
    async fn batch(&self, issues: &[IssueId]) -> Result<MetadataRows, Self::Error> {
        tokio::try_join!(self.labels_of(issues), self.assignees_of(issues), self.milestones_of(issues))
    }
}
