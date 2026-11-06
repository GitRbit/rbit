#![cfg(feature = "applications")]
//! Ordered batched reference resolution does not look up remote repository names.
use rbit_collaboration::{
    IssueState,
    references::{ReferenceReadPort, ReferenceResolver, scan_plain},
    threads::{IssueKind, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind};
use std::sync::Mutex;
#[derive(Default)]
struct Port(Mutex<Vec<(Vec<IssueNumber>, Vec<String>)>>);
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl ReferenceReadPort for Port {
    type Error = ();
    async fn resolve_batch(
        &self,
        repo: RepoId,
        numbers: &[IssueNumber],
        names: &[String],
    ) -> Result<(Vec<IssueRef>, Vec<Account>), Self::Error> {
        assert_eq!(repo, RepoId(3));
        self.0.lock().unwrap().push((numbers.to_vec(), names.to_vec()));
        Ok((
            vec![IssueRef {
                id: rbit_collaboration::threads::IssueId(1),
                number: IssueNumber::new(1).unwrap(),
                kind: IssueKind::Issue,
                state: IssueState::Open,
                title: "Found".into(),
            }],
            vec![Account {
                id: AccountId(7),
                kind: AccountKind::User,
                name: "Bob".parse().unwrap(),
                display_name: None,
                bio: None,
                created_at: Timestamp(1),
            }],
        ))
    }
}
#[tokio::test]
async fn first_occurrence_batching_and_stored_case_mapping_preserve_contracts() {
    let port = Port::default();
    let app = ReferenceResolver::new(&port);
    let set = app.resolve(RepoId(3), &["#2 #1 @Bob alice/private#99", "#1 @bob @alice"], scan_plain).await.unwrap();
    assert_eq!(
        port.0.lock().unwrap()[0],
        (vec![IssueNumber::new(2).unwrap(), IssueNumber::new(1).unwrap()], vec!["bob".into(), "alice".into()])
    );
    assert_eq!(set.account("BOB"), Some("Bob"));
    assert_eq!(set.issue(IssueNumber::new(1).unwrap()).unwrap().title, "Found");
}
#[tokio::test]
async fn unpolled_resolver_does_not_scan_or_construct_batches() {
    let port = Port::default();
    let app = ReferenceResolver::new(&port);
    drop(app.resolve(RepoId(3), &["#1"], |_| panic!("scan")));
    assert_eq!(port.0.lock().unwrap().len(), 0);
}
