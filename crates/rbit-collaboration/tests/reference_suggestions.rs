#![cfg(feature = "applications")]
//! Suggestion authority, Unicode bounds and numeric/search routing are independently observed.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::{
    references::{ReferenceSuggestions, SuggestionError, SuggestionPort, SuggestionThreadPort},
    threads::{Issue, IssueKind, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind};
use std::sync::Mutex;
#[derive(Default)]
struct Port(Mutex<Vec<String>>);
struct Threads<'a>(&'a Port);
fn call(port: &Port, value: impl Into<String>) {
    port.0.lock().unwrap().push(value.into());
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl<'a> SuggestionPort for &'a Port {
    type Error = ();
    type Threads = Threads<'a>;
    async fn accounts(&self, prefix: &str) -> Result<Vec<Account>, Self::Error> {
        call(self, format!("accounts:{prefix}"));
        Ok(vec![Account {
            id: AccountId(1),
            kind: AccountKind::User,
            name: "Alice".parse().unwrap(),
            display_name: Some("Display".to_owned()),
            bio: None,
            created_at: Timestamp(1),
        }])
    }
    fn threads(&self) -> Self::Threads {
        call(self, "construct_threads");
        Threads(self)
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl SuggestionThreadPort for Threads<'_> {
    type Error = ();
    async fn numbers(&self, repo: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        assert_eq!(repo, RepoId(3));
        call(self.0, format!("numbers:{}", numbers[0].get()));
        Ok(vec![IssueRef {
            id: fixtures::issue().id,
            number: numbers[0],
            kind: IssueKind::Pull,
            state: fixtures::issue().state,
            title: "Pull title".to_owned(),
        }])
    }
    async fn search(&self, repo: RepoId, words: Vec<String>, limit: u32, offset: u64) -> Result<Vec<Issue>, Self::Error> {
        assert_eq!((repo, limit, offset), (RepoId(3), 10, 0));
        call(self.0, format!("search:{words:?}"));
        Ok(vec![
            Issue {
                title: rbit_collaboration::threads::IssueTitle::parse("Issue title").unwrap(),
                number: IssueNumber::new(2).unwrap(),
                ..fixtures::issue()
            },
            Issue {
                kind: IssueKind::Pull,
                title: rbit_collaboration::threads::IssueTitle::parse("Pull title").unwrap(),
                ..fixtures::issue()
            },
        ])
    }
}
#[tokio::test]
async fn denied_and_unpolled_requests_never_query_or_construct_store() {
    let port = Port::default();
    let app = ReferenceSuggestions::new(RepoId(3), false);
    assert!(matches!(app.list(&&port, &"λ".repeat(1000), false).await, Err(SuggestionError::NotFound)));
    let app = ReferenceSuggestions::new(RepoId(3), true);
    drop(app.list(&&port, "3", false));
    assert_eq!(*port.0.lock().unwrap(), Vec::<String>::new());
}
#[tokio::test]
async fn accounts_use_unicode_character_cap_and_stored_name_display_without_thread_store() {
    let port = Port::default();
    let result = ReferenceSuggestions::new(RepoId(3), true).list(&&port, &"λ".repeat(101), true).await.unwrap();
    assert_eq!(result, [("@Alice".to_owned(), "Display".to_owned())]);
    assert_eq!(*port.0.lock().unwrap(), [format!("accounts:{}", "λ".repeat(100))]);
}
#[tokio::test]
async fn numeric_canonical_parse_precedes_search_and_keeps_pr_label() {
    let port = Port::default();
    let result = ReferenceSuggestions::new(RepoId(3), true).list(&&port, "3", false).await.unwrap();
    assert_eq!(result, [("#3".to_owned(), "PR · Pull title".to_owned())]);
    assert_eq!(*port.0.lock().unwrap(), ["construct_threads", "numbers:3"]);
}
#[tokio::test]
async fn noncanonical_numbers_keep_whitespace_search_limit_order_and_issue_pr_labels() {
    for (prefix, words) in [("03", vec!["03"]), (" 3 ", vec!["3"]), ("#3", vec!["#3"]), ("a\tb  c", vec!["a", "b", "c"])] {
        let port = Port::default();
        let result = ReferenceSuggestions::new(RepoId(3), true).list(&&port, prefix, false).await.unwrap();
        assert_eq!(result, [("#2".to_owned(), "Issue · Issue title".to_owned()), ("#1".to_owned(), "PR · Pull title".to_owned())]);
        assert_eq!(*port.0.lock().unwrap(), ["construct_threads".to_owned(), format!("search:{words:?}")]);
    }
}
