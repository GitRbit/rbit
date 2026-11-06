#![cfg(feature = "applications")]
//! Author visibility, ordered dedup and existing partial reference/history effects.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::{
    references::{CrossReferencePort, CrossReferences, RemoteRef, RemoteReferencePort, scan_plain},
    threads::{Issue, IssueId, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::{OwnedRepository, Repository, Visibility};
use std::{collections::HashSet, sync::Mutex};
fn user() -> User {
    User {
        account: Account {
            id: AccountId(7),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}
fn issue(id: IssueId, repo: RepoId) -> Issue {
    Issue { id, repo_id: repo, number: IssueNumber::new(u32::try_from(id.0).unwrap()).unwrap(), ..fixtures::issue() }
}
fn repo(id: RepoId) -> OwnedRepository {
    OwnedRepository {
        owner: user().account,
        repo: Repository {
            id,
            owner_id: AccountId(7),
            name: "repo".parse().unwrap(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".into(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            pushed_at: None,
            archived_at: Some(Timestamp(2)),
        },
    }
}
struct State {
    calls: Vec<String>,
    local: Vec<IssueId>,
    active: bool,
    readable: bool,
    links: HashSet<(IssueId, IssueId)>,
    fail_history: bool,
}
impl Default for State {
    fn default() -> Self {
        Self { calls: Vec::new(), local: Vec::new(), active: true, readable: true, links: HashSet::new(), fail_history: false }
    }
}
struct Port(Mutex<State>);
struct Remote<'a>(&'a Port);
// Keep the concrete remote owned by reference without copying user/context state.
#[derive(Clone, Copy)]
struct View<'a>(&'a Port);
fn call(port: &Port, value: impl Into<String>) {
    port.0.lock().unwrap().calls.push(value.into());
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl RemoteReferencePort for Remote<'_> {
    type Error = &'static str;
    async fn find(&self, remote: &RemoteRef) -> Result<Option<OwnedRepository>, Self::Error> {
        call(self.0, format!("repo:{}", remote.repo));
        Ok(Some(repo(if remote.repo == "same" { RepoId(3) } else { RepoId(4) })))
    }
    async fn can_read(&self, _: &User, _: &OwnedRepository) -> Result<bool, Self::Error> {
        call(self.0, "access");
        Ok(self.0.0.lock().unwrap().readable)
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl<'a> CrossReferencePort for View<'a> {
    type Error = &'static str;
    type Remote = Remote<'a>;
    async fn numbers(&self, _: RepoId, _: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        call(self.0, "numbers");
        Ok(self
            .0
            .0
            .lock()
            .unwrap()
            .local
            .iter()
            .map(|id| IssueRef {
                id: *id,
                number: IssueNumber::new(u32::try_from(id.0).unwrap()).unwrap(),
                kind: fixtures::issue().kind,
                state: fixtures::issue().state,
                title: String::new(),
            })
            .collect())
    }
    async fn issue(&self, id: IssueId) -> Result<Option<Issue>, Self::Error> {
        call(self.0, format!("issue:{}", id.0));
        Ok(Some(issue(id, RepoId(3))))
    }
    async fn active_user(&self, _: AccountId) -> Result<Option<User>, Self::Error> {
        call(self.0, "actor");
        Ok(self.0.0.lock().unwrap().active.then(user))
    }
    fn repositories(&self) -> Self::Remote {
        call(self.0, "repositories");
        Remote(self.0)
    }
    async fn by_number(&self, repo: RepoId, number: IssueNumber) -> Result<Option<Issue>, Self::Error> {
        call(self.0, format!("target:{}", number.get()));
        Ok(Some(issue(IssueId(i64::from(number.get())), repo)))
    }
    async fn link(&self, target: IssueId, source: IssueId, at: Timestamp) -> Result<bool, Self::Error> {
        assert_eq!(at, Timestamp(9));
        call(self.0, format!("link:{}", target.0));
        Ok(self.0.0.lock().unwrap().links.insert((target, source)))
    }
    async fn record(&self, target: IssueId, _: IssueId, _: Option<AccountId>) -> Result<(), Self::Error> {
        call(self.0, format!("history:{}", target.0));
        if self.0.0.lock().unwrap().fail_history { Err("history failed") } else { Ok(()) }
    }
}
#[tokio::test]
async fn empty_and_unpolled_scans_do_no_port_or_clock_work() {
    let port = Port(Mutex::new(State::default()));
    let view = View(&port);
    let app = CrossReferences::new(&view);
    let source = fixtures::issue();
    app.record(&source, Some(AccountId(7)), "plain", scan_plain, || panic!("clock")).await.unwrap();
    drop(app.record(&source, Some(AccountId(7)), "#2", scan_plain, || panic!("clock")));
    assert_eq!(port.0.lock().unwrap().calls, Vec::<String>::new());
}
#[tokio::test]
async fn local_lookup_actor_then_remote_visibility_precede_ordered_dedup_effects() {
    let port = Port(Mutex::new(State { local: vec![IssueId(1), IssueId(2)], ..State::default() }));
    let view = View(&port);
    CrossReferences::new(&view)
        .record(&fixtures::issue(), Some(AccountId(7)), "#1 #2 alice/same#2 alice/foreign#3", scan_plain, || Timestamp(9))
        .await
        .unwrap();
    assert_eq!(
        port.0.lock().unwrap().calls,
        [
            "numbers",
            "issue:1",
            "issue:2",
            "actor",
            "repositories",
            "repo:same",
            "target:2",
            "repo:foreign",
            "access",
            "target:3",
            "link:2",
            "history:2",
            "link:3",
            "history:3"
        ]
    );
}
#[tokio::test]
async fn missing_or_inactive_actor_skips_remote_lookup_but_preserves_local_links() {
    for actor in [None, Some(AccountId(7))] {
        let port = Port(Mutex::new(State { local: vec![IssueId(2)], active: false, ..State::default() }));
        let view = View(&port);
        CrossReferences::new(&view).record(&fixtures::issue(), actor, "#2 alice/foreign#3", scan_plain, || Timestamp(9)).await.unwrap();
        let calls = &port.0.lock().unwrap().calls;
        assert!(!calls.iter().any(|call| call.starts_with("repo")));
        assert!(calls.iter().any(|call| call == "link:2"));
    }
}
#[tokio::test]
async fn private_remote_is_not_fetched_and_history_failure_keeps_link_without_repair() {
    let port = Port(Mutex::new(State { readable: false, ..State::default() }));
    let view = View(&port);
    CrossReferences::new(&view)
        .record(&fixtures::issue(), Some(AccountId(7)), "alice/foreign#3", scan_plain, || panic!("clock"))
        .await
        .unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["numbers", "actor", "repositories", "repo:foreign", "access"]);
    let port = Port(Mutex::new(State { local: vec![IssueId(2)], fail_history: true, ..State::default() }));
    let view = View(&port);
    let app = CrossReferences::new(&view);
    assert_eq!(app.record(&fixtures::issue(), None, "#2", scan_plain, || Timestamp(9)).await.unwrap_err(), "history failed");
    assert_eq!(port.0.lock().unwrap().links.len(), 1);
    port.0.lock().unwrap().calls.clear();
    app.record(&fixtures::issue(), None, "#2", scan_plain, || Timestamp(9)).await.unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["numbers", "issue:2", "link:2"]);
}
