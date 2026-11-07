//! Shared title/body editing, preserving the original transaction and stale-context semantics.

use std::future::Future;

use rbit_contracts::{AccountId, Timestamp};

use super::{Issue, IssueId, IssueText, IssueTextError, IssueTitle};

/// The adapter recomputes permission for the acting user, not the context's old viewer.
#[derive(Debug, Clone, Copy)]
pub struct ThreadEditAccess {
    pub can_edit: bool,
    pub archived: bool,
}

/// Persistence retains atomic title rename events, monotonic timestamps and no-op reads.
pub trait ThreadTextRepository: Sync {
    type Error;
    fn update_text(
        &self,
        id: IssueId,
        title: Option<&IssueTitle>,
        body: Option<&IssueText>,
        actor: AccountId,
        at: Timestamp,
    ) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
}

#[derive(Debug)]
pub enum ThreadEditError<E, P = E> {
    Store(E),
    Throttle(P),
    Text(IssueTextError),
    Forbidden,
    Archived,
    NotFound,
}

/// Previous values from the supplied context, matching the existing public event intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadTextChange {
    pub previous_title: Option<String>,
    pub previous_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadEditResult {
    pub issue: Issue,
    /// `None` means no event is published; it does not imply no throttle/store work.
    pub change: Option<ThreadTextChange>,
}

#[derive(Debug)]
pub struct ThreadEditor<'a, R> {
    repository: &'a R,
    actor: AccountId,
    access: ThreadEditAccess,
}

impl<'a, R: ThreadTextRepository> ThreadEditor<'a, R> {
    pub fn new(repository: &'a R, actor: AccountId, access: ThreadEditAccess) -> Self {
        Self { repository, actor, access }
    }

    pub async fn edit<P>(
        &self,
        issue: &Issue,
        title: Option<&str>,
        body: Option<&str>,
        spend: impl FnOnce() -> Result<(), P>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<ThreadEditResult, ThreadEditError<R::Error, P>> {
        if !self.access.can_edit {
            return Err(ThreadEditError::Forbidden);
        }
        if self.access.archived {
            return Err(ThreadEditError::Archived);
        }
        let title = title.map(IssueTitle::parse).transpose().map_err(ThreadEditError::Text)?;
        let body = body.map(IssueText::body).transpose().map_err(ThreadEditError::Text)?;
        spend().map_err(ThreadEditError::Throttle)?;
        let updated = self
            .repository
            .update_text(issue.id, title.as_ref(), body.as_ref(), self.actor, now())
            .await
            .map_err(ThreadEditError::Store)?
            .ok_or(ThreadEditError::NotFound)?;
        // Compare with the caller's context, not a freshly read pre-update snapshot.
        let previous_title = (updated.title != issue.title).then(|| issue.title.to_string());
        let previous_body = (updated.body != issue.body).then(|| issue.body.to_string());
        let change = if previous_title.is_some() || previous_body.is_some() {
            Some(ThreadTextChange { previous_title, previous_body })
        } else {
            None
        };
        Ok(ThreadEditResult { issue: updated, change })
    }
}
