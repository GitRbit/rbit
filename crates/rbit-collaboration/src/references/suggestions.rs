//! Bounded mention and thread suggestions using the caller's authorized repository scope.
use crate::{
    threads::{Issue, IssueKind, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::RepoId;
use rbit_identity_models::Account;
use std::future::Future;
pub trait SuggestionThreadPort: Sync {
    type Error;
    fn numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> impl Future<Output = Result<Vec<IssueRef>, Self::Error>> + Send;
    fn search(
        &self,
        repo: RepoId,
        words: Vec<String>,
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = Result<Vec<Issue>, Self::Error>> + Send;
}
pub trait SuggestionPort: Sync {
    type Error;
    type Threads: SuggestionThreadPort<Error = Self::Error>;
    fn accounts(&self, prefix: &str) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
    fn threads(&self) -> Self::Threads;
}
#[derive(Debug)]
pub enum SuggestionError<E> {
    Port(E),
    NotFound,
}
#[derive(Debug, Clone, Copy)]
pub struct ReferenceSuggestions {
    repo: RepoId,
    can_read: bool,
}
impl ReferenceSuggestions {
    pub fn new(repo: RepoId, can_read: bool) -> Self {
        Self { repo, can_read }
    }
    pub async fn list<P: SuggestionPort>(
        &self,
        port: &P,
        prefix: &str,
        accounts: bool,
    ) -> Result<Vec<(String, String)>, SuggestionError<P::Error>> {
        if !self.can_read {
            return Err(SuggestionError::NotFound);
        }
        let prefix: String = prefix.chars().take(100).collect();
        if accounts {
            return Ok(port
                .accounts(&prefix)
                .await
                .map_err(SuggestionError::Port)?
                .into_iter()
                .map(|account| (format!("@{}", account.name), account.label().to_owned()))
                .collect());
        }
        let threads = port.threads();
        let issues = if let Ok(number) = IssueNumber::parse(&prefix) {
            return Ok(threads
                .numbers(self.repo, &[number])
                .await
                .map_err(SuggestionError::Port)?
                .into_iter()
                .map(|issue| {
                    (
                        format!("#{}", issue.number.get()),
                        format!("{} · {}", if issue.kind == IssueKind::Pull { "PR" } else { "Issue" }, issue.title),
                    )
                })
                .collect());
        } else {
            let words = prefix.split_whitespace().map(str::to_owned).collect();
            threads.search(self.repo, words, 10, 0).await.map_err(SuggestionError::Port)?
        };
        Ok(issues
            .into_iter()
            .map(|issue| {
                (format!("#{}", issue.number.get()), format!("{} · {}", if issue.is_pull() { "PR" } else { "Issue" }, issue.title))
            })
            .collect())
    }
}
