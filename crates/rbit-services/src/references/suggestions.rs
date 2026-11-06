//! Concrete suggestion stores are acquired only after the feature's authorization/prefix gates.
use super::ports::ReferenceView;
use crate::ServiceError;
use rbit_collaboration::{
    references::{SuggestionPort, SuggestionThreadPort},
    threads::{Issue, IssueNumber},
    timeline::IssueRef,
};
use rbit_core::{Account, RepoId};
use rbit_storage::{
    Database,
    sqlite::{issue_search::SearchFilter, issues::IssueStore},
};
pub(super) struct SuggestionThreads<'a> {
    database: &'a Database,
    store: IssueStore,
}
impl<'a> SuggestionPort for ReferenceView<'a> {
    type Error = ServiceError;
    type Threads = SuggestionThreads<'a>;
    async fn accounts(&self, prefix: &str) -> Result<Vec<Account>, Self::Error> {
        Ok(self.database.issue_meta().mention_candidates(prefix).await?)
    }
    fn threads(&self) -> Self::Threads {
        SuggestionThreads { database: self.database, store: self.database.issues() }
    }
}
impl SuggestionThreadPort for SuggestionThreads<'_> {
    type Error = ServiceError;
    async fn numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        Ok(self.store.find_numbers(repo, numbers).await?)
    }
    async fn search(&self, repo: RepoId, words: Vec<String>, limit: u32, offset: u64) -> Result<Vec<Issue>, Self::Error> {
        let mut filter = SearchFilter::new(repo);
        filter.text = words;
        Ok(self.database.issue_search().search(&filter, limit, offset).await?)
    }
}
