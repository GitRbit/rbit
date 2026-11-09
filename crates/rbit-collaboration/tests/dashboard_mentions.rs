#![cfg(feature = "applications")]
//! Dashboard mention recording and the one-time backfill without concrete persistence.

use std::sync::Mutex;

use rbit_collaboration::{
    issues::{DashboardMentionPort, DashboardMentions},
    threads::IssueId,
};
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::{Account, AccountKind};

#[derive(Default)]
struct Mentions {
    calls: Mutex<Vec<String>>,
    indexed: bool,
}

impl Mentions {
    fn record(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

fn account(id: i64, name: &str, kind: AccountKind) -> Account {
    Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(1) }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl DashboardMentionPort for Mentions {
    type Error = &'static str;

    async fn accounts_named(&self, names: &[String]) -> Result<Vec<Account>, Self::Error> {
        self.record(format!("accounts {}", names.join(",")));
        Ok(vec![account(7, "ada", AccountKind::User), account(8, "acme", AccountKind::Organization), account(9, "bob", AccountKind::User)])
    }

    async fn record_mentions(&self, issue: IssueId, people: &[AccountId]) -> Result<(), Self::Error> {
        self.record(format!("record {} {:?}", issue.0, people.iter().map(|id| id.0).collect::<Vec<_>>()));
        Ok(())
    }

    async fn mentions_indexed(&self) -> Result<bool, Self::Error> {
        self.record("indexed?".to_owned());
        Ok(self.indexed)
    }

    async fn mention_sources(&self, source: u8, after: i64) -> Result<Vec<(i64, i64, String)>, Self::Error> {
        self.record(format!("sources {source} {after}"));
        // Source 1 has two batches; the others are empty.
        Ok(match (source, after) {
            (1, 0) => vec![(3, 10, "@ada".to_owned()), (5, 11, "nobody".to_owned())],
            (1, 5) => vec![(9, 12, "@bob".to_owned())],
            _ => Vec::new(),
        })
    }

    async fn finish_mentions_index(&self) -> Result<(), Self::Error> {
        self.record("finish".to_owned());
        Ok(())
    }
}

fn scan(text: &str) -> Vec<String> {
    text.split_whitespace().filter_map(|word| word.strip_prefix('@')).map(str::to_owned).collect()
}

#[tokio::test]
async fn people_are_recorded_and_organisations_ignored() {
    let port = Mentions::default();
    let mentions = DashboardMentions::new(&port, scan);
    mentions.record(IssueId(4), "thanks @ada and @acme cc @bob").await.unwrap();
    assert_eq!(port.calls(), ["accounts ada,acme,bob", "record 4 [7, 9]"]);

    let quiet = Mentions::default();
    DashboardMentions::new(&quiet, scan).record(IssueId(4), "no mentions here").await.unwrap();
    assert!(quiet.calls().is_empty(), "text without mentions touches no store");
}

#[tokio::test]
async fn the_backfill_walks_every_source_in_batches_once() {
    let port = Mentions::default();
    DashboardMentions::new(&port, scan).index_existing().await.unwrap();
    assert_eq!(
        port.calls(),
        [
            "indexed?",
            "sources 0 0",
            "sources 1 0",
            "accounts ada",
            "record 10 [7, 9]",
            "sources 1 5",
            "accounts bob",
            "record 12 [7, 9]",
            "sources 1 9",
            "sources 2 0",
            "sources 3 0",
            "finish",
        ]
    );

    let done = Mentions { indexed: true, ..Mentions::default() };
    DashboardMentions::new(&done, scan).index_existing().await.unwrap();
    assert_eq!(done.calls(), ["indexed?"], "a completed index is not scanned again");
}
