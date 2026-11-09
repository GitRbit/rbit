//! The dashboard's Mentioned view: who a thread's Markdown mentions, kept once indexed.
//! Which events and bodies are indexed is the issue event subscriber's choice.

use std::future::Future;

use rbit_contracts::AccountId;
use rbit_identity_models::Account;

use crate::threads::IssueId;

/// Historical body sources the one-time backfill walks, in order: issue and pull request
/// bodies, comments, submitted reviews and their line comments (pending reviews excluded
/// by persistence).
pub const MENTION_SOURCES: u8 = 4;

/// Mention persistence. Recording is idempotent; organisations are filtered here.
pub trait DashboardMentionPort: Sync {
    type Error;

    /// Accounts (people and organisations) among `names`, case-insensitively.
    fn accounts_named(&self, names: &[String]) -> impl Future<Output = Result<Vec<Account>, Self::Error>> + Send;
    fn record_mentions(&self, issue: IssueId, people: &[AccountId]) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn mentions_indexed(&self) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    /// One bounded batch of `(row id, thread id, body)` of `source` after row `after`.
    fn mention_sources(&self, source: u8, after: i64) -> impl Future<Output = Result<Vec<(i64, i64, String)>, Self::Error>> + Send;
    fn finish_mentions_index(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Mention recording over a port and a Markdown scanner (`@name`s outside code).
#[derive(Debug)]
pub struct DashboardMentions<'a, P, S> {
    port: &'a P,
    scan: S,
}

impl<'a, P: DashboardMentionPort, S: Fn(&str) -> Vec<String> + Sync> DashboardMentions<'a, P, S> {
    pub fn new(port: &'a P, scan: S) -> Self {
        Self { port, scan }
    }

    /// Records the people `text` mentions on `issue`. Organisations and unknown names are
    /// ignored; text without mentions touches no store.
    pub async fn record(&self, issue: IssueId, text: &str) -> Result<(), P::Error> {
        let names = (self.scan)(text);
        if !names.is_empty() {
            let accounts = self.port.accounts_named(&names).await?;
            let ids: Vec<_> = accounts.iter().filter(|account| !account.is_organization()).map(|account| account.id).collect();
            self.port.record_mentions(issue, &ids).await?;
        }
        Ok(())
    }

    /// One-time, restart-safe upgrade of existing bodies; a no-op once complete.
    pub async fn index_existing(&self) -> Result<(), P::Error> {
        if self.port.mentions_indexed().await? {
            return Ok(());
        }
        for source in 0..MENTION_SOURCES {
            let mut after = 0;
            loop {
                let rows = self.port.mention_sources(source, after).await?;
                if rows.is_empty() {
                    break;
                }
                for (id, issue, body) in rows {
                    self.record(IssueId(issue), &body).await?;
                    after = id;
                }
            }
        }
        self.port.finish_mentions_index().await
    }
}
