//! Per-commit and merged-pull target policy, independent of Git enumeration and command setup.
use super::{Scan, scan_plain};
use crate::{
    threads::{Issue, IssueId, IssueKind, IssueNumber},
    timeline::IssueRef,
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::future::Future;
#[derive(Debug, Clone, Copy)]
pub struct CommitReference<'a> {
    pub id: &'a str,
    pub summary: &'a str,
    pub body: Option<&'a str>,
}
pub trait CommitReferencePort: Sync {
    type Error;
    fn targets(&self, repo: RepoId, numbers: &[IssueNumber]) -> impl Future<Output = Result<Vec<IssueRef>, Self::Error>> + Send;
    fn commit_link(&self, target: IssueId, commit: &str, at: Timestamp) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn record_commit(
        &self,
        target: IssueId,
        commit: &str,
        summary: &str,
        actor: Option<AccountId>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn close_commit(&self, target: IssueId, commit: &str, actor: Option<AccountId>)
    -> impl Future<Output = Result<(), Self::Error>> + Send;
}
pub trait MergeReferencePort: Sync {
    type Error;
    fn merged_into_default(&self, pull: IssueId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn pull_issue(&self, pull: IssueId) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    fn closing_targets(&self, repo: RepoId, numbers: &[IssueNumber]) -> impl Future<Output = Result<Vec<IssueRef>, Self::Error>> + Send;
    fn close_merge(
        &self,
        target: IssueId,
        pull: IssueNumber,
        actor: Option<AccountId>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}
#[derive(Debug)]
pub struct CommitReferences<'a, P> {
    port: &'a P,
}
impl<'a, P: CommitReferencePort> CommitReferences<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    /// The caller supplies the original oldest-first iterator, without a DTO Vec or copied OIDs.
    pub async fn record<'c>(
        &self,
        repo: RepoId,
        actor: Option<AccountId>,
        default_branch: bool,
        commits: impl Iterator<Item = CommitReference<'c>>,
        mut now: impl FnMut() -> Timestamp,
    ) -> Result<(), P::Error> {
        for commit in commits {
            let message = match commit.body {
                Some(body) => format!("{}\n\n{body}", commit.summary),
                None => commit.summary.to_owned(),
            };
            let scan = scan_plain(&message);
            if scan.local.is_empty() {
                continue;
            }
            for target in self.port.targets(repo, &scan.local).await? {
                if self.port.commit_link(target.id, commit.id, now()).await? {
                    self.port.record_commit(target.id, commit.id, commit.summary, actor).await?;
                }
                if default_branch && target.kind == IssueKind::Issue && scan.closes.contains(&target.number) {
                    self.port.close_commit(target.id, commit.id, actor).await?;
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct MergeIssueClosures<'a, P> {
    port: &'a P,
}
impl<'a, P: MergeReferencePort> MergeIssueClosures<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn close(
        &self,
        pull: IssueId,
        number: IssueNumber,
        actor: Option<AccountId>,
        scan: impl FnOnce(&str) -> Scan,
    ) -> Result<(), P::Error> {
        if !self.port.merged_into_default(pull).await? {
            return Ok(());
        }
        let Some(pull) = self.port.pull_issue(pull).await? else {
            return Ok(());
        };
        let scan = scan(pull.body.as_str());
        if scan.closes.is_empty() {
            return Ok(());
        }
        for target in self.port.closing_targets(pull.repo_id, &scan.closes).await? {
            if target.kind == IssueKind::Issue {
                self.port.close_merge(target.id, number, actor).await?;
            }
        }
        Ok(())
    }
}
