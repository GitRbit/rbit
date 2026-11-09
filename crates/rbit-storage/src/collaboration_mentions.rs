//! Static dashboard mention port adapter; the idempotent insert and batch SQL are unchanged.

use rbit_collaboration::{issues::DashboardMentionPort, threads::IssueId};
use rbit_core::{Account, AccountId};

use crate::{StorageError, sqlite::Database};

impl DashboardMentionPort for Database {
    type Error = StorageError;

    async fn accounts_named(&self, names: &[String]) -> Result<Vec<Account>, Self::Error> {
        self.issue_meta().accounts_named(names).await
    }

    async fn record_mentions(&self, issue: IssueId, people: &[AccountId]) -> Result<(), Self::Error> {
        self.issue_search().record_mentions(issue, people).await
    }

    async fn mentions_indexed(&self) -> Result<bool, Self::Error> {
        self.issue_search().mentions_indexed().await
    }

    async fn mention_sources(&self, source: u8, after: i64) -> Result<Vec<(i64, i64, String)>, Self::Error> {
        self.issue_search().mention_sources(source, after).await
    }

    async fn finish_mentions_index(&self) -> Result<(), Self::Error> {
        self.issue_search().finish_mentions_index().await
    }
}
