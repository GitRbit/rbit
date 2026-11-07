#![cfg(feature = "applications")]
//! Commit and merge target policy without Git command or storage implementation coupling.
#[path = "support/thread_fixtures.rs"]
mod fixtures;
use rbit_collaboration::{
    IssueState,
    references::{CommitReference, CommitReferencePort, CommitReferences, MergeIssueClosures, MergeReferencePort, scan_plain},
    threads::{Issue, IssueId, IssueKind, IssueNumber, IssueText},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::{collections::HashSet, sync::Mutex};
struct State {
    calls: Vec<String>,
    links: HashSet<(IssueId, String)>,
    fail_history: bool,
    default_branch: bool,
    pull: Option<Issue>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            links: HashSet::new(),
            fail_history: false,
            default_branch: true,
            pull: Some(Issue { body: IssueText::body("Fixes #1 fixes #2").unwrap(), ..fixtures::issue() }),
        }
    }
}
struct Port(Mutex<State>);
fn targets() -> Vec<IssueRef> {
    (1..=2)
        .map(|id| IssueRef {
            id: IssueId(id),
            number: IssueNumber::new(u32::try_from(id).unwrap()).unwrap(),
            kind: if id == 1 { IssueKind::Issue } else { IssueKind::Pull },
            state: IssueState::Open,
            title: String::new(),
        })
        .collect()
}
fn call(port: &Port, value: impl Into<String>) {
    port.0.lock().unwrap().calls.push(value.into());
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl CommitReferencePort for Port {
    type Error = &'static str;
    async fn targets(&self, _: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        call(self, "targets");
        Ok(targets().into_iter().filter(|target| numbers.contains(&target.number)).collect())
    }
    async fn commit_link(&self, target: IssueId, commit: &str, at: Timestamp) -> Result<bool, Self::Error> {
        assert_eq!(at, Timestamp(9));
        call(self, format!("link:{}:{commit}", target.0));
        Ok(self.0.lock().unwrap().links.insert((target, commit.into())))
    }
    async fn record_commit(&self, target: IssueId, commit: &str, _: &str, _: Option<AccountId>) -> Result<(), Self::Error> {
        call(self, format!("history:{}:{commit}", target.0));
        if self.0.lock().unwrap().fail_history { Err("history") } else { Ok(()) }
    }
    async fn close_commit(&self, target: IssueId, commit: &str, _: Option<AccountId>) -> Result<(), Self::Error> {
        call(self, format!("close:{}:{commit}", target.0));
        Ok(())
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl MergeReferencePort for Port {
    type Error = &'static str;
    async fn merged_into_default(&self, _: IssueId) -> Result<bool, Self::Error> {
        call(self, "default");
        Ok(self.0.lock().unwrap().default_branch)
    }
    async fn pull_issue(&self, _: IssueId) -> Result<Option<Issue>, Self::Error> {
        call(self, "pull");
        Ok(self.0.lock().unwrap().pull.clone())
    }
    async fn closing_targets(&self, _: RepoId, numbers: &[IssueNumber]) -> Result<Vec<IssueRef>, Self::Error> {
        call(self, "targets");
        Ok(targets().into_iter().filter(|target| numbers.contains(&target.number)).collect())
    }
    async fn close_merge(&self, target: IssueId, number: IssueNumber, _: Option<AccountId>) -> Result<(), Self::Error> {
        call(self, format!("merge:{}:{}", target.0, number.get()));
        Ok(())
    }
}
#[tokio::test]
async fn supplied_oldest_first_commits_and_issue_only_default_closure_keep_effect_order() {
    let port = Port(Mutex::new(State::default()));
    let commits = [
        CommitReference { id: "a", summary: "Fixes #1", body: Some("see #2") },
        CommitReference { id: "b", summary: "Fixes #1 #2", body: None },
    ];
    CommitReferences::new(&port).record(RepoId(3), None, true, commits.into_iter(), || Timestamp(9)).await.unwrap();
    assert_eq!(
        port.0.lock().unwrap().calls,
        [
            "targets",
            "link:1:a",
            "history:1:a",
            "close:1:a",
            "link:2:a",
            "history:2:a",
            "targets",
            "link:1:b",
            "history:1:b",
            "close:1:b",
            "link:2:b",
            "history:2:b"
        ]
    );
}
#[tokio::test]
async fn feature_branch_reference_then_default_replay_can_close_without_duplicate_history() {
    let port = Port(Mutex::new(State::default()));
    let commit = CommitReference { id: "full-oid", summary: "Fixes #1", body: None };
    let app = CommitReferences::new(&port);
    app.record(RepoId(3), None, false, std::iter::once(commit), || Timestamp(9)).await.unwrap();
    port.0.lock().unwrap().calls.clear();
    app.record(RepoId(3), None, true, std::iter::once(commit), || Timestamp(9)).await.unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["targets", "link:1:full-oid", "close:1:full-oid"]);
}
#[tokio::test]
async fn no_local_references_and_unpolled_work_skip_storage_and_clock() {
    let port = Port(Mutex::new(State::default()));
    let app = CommitReferences::new(&port);
    let commit = CommitReference { id: "oid", summary: "alice/repo#1 @alice", body: None };
    app.record(RepoId(3), None, true, std::iter::once(commit), || panic!("clock")).await.unwrap();
    drop(app.record(RepoId(3), None, true, std::iter::once(commit), || panic!("clock")));
    assert_eq!(port.0.lock().unwrap().calls, Vec::<String>::new());
}
#[tokio::test]
async fn failed_history_commits_link_and_prevents_current_attempt_closure() {
    let port = Port(Mutex::new(State { fail_history: true, ..State::default() }));
    let commit = CommitReference { id: "oid", summary: "Fixes #1", body: None };
    assert_eq!(
        CommitReferences::new(&port).record(RepoId(3), None, true, std::iter::once(commit), || Timestamp(9)).await.unwrap_err(),
        "history"
    );
    assert_eq!(port.0.lock().unwrap().calls, ["targets", "link:1:oid", "history:1:oid"]);
    assert_eq!(port.0.lock().unwrap().links.len(), 1);
}
#[tokio::test]
async fn merge_default_gate_then_missing_pull_or_no_closes_keep_early_paths() {
    let port = Port(Mutex::new(State { default_branch: false, ..State::default() }));
    let app = MergeIssueClosures::new(&port);
    app.close(IssueId(9), IssueNumber::new(9).unwrap(), None, |_| panic!("scan")).await.unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["default"]);
    port.0.lock().unwrap().calls.clear();
    port.0.lock().unwrap().default_branch = true;
    port.0.lock().unwrap().pull = None;
    app.close(IssueId(9), IssueNumber::new(9).unwrap(), None, |_| panic!("scan")).await.unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["default", "pull"]);
}
#[tokio::test]
async fn merged_pull_closes_only_issue_targets_with_fixed_source_number() {
    let port = Port(Mutex::new(State::default()));
    MergeIssueClosures::new(&port).close(IssueId(9), IssueNumber::new(9).unwrap(), Some(AccountId(7)), scan_plain).await.unwrap();
    assert_eq!(port.0.lock().unwrap().calls, ["default", "pull", "targets", "merge:1:9"]);
}
