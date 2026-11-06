//! Issue-only deletion preserves its existing commit, best-effort audit and event boundary.
use crate::threads::{Issue, IssueId, IssueKind};
use std::future::Future;
pub trait IssueDeletionPort: Sync {
    type Error;
    fn delete(&self, id: IssueId) -> impl Future<Output = Result<Option<Issue>, Self::Error>> + Send;
    /// Existing best-effort audit completes before publication; it does not undo the SQL delete.
    fn audit(&self, snapshot: &Issue) -> impl Future<Output = ()> + Send;
    fn publish(&self, snapshot: Issue);
}
#[derive(Debug)]
pub enum IssueDeletionError<E> {
    Forbidden,
    Archived,
    NotFound,
    Port(E),
}
#[derive(Debug, Clone, Copy)]
pub struct IssueDeletion {
    can_admin: bool,
    archived: bool,
}
impl IssueDeletion {
    pub fn new(can_admin: bool, archived: bool) -> Self {
        Self { can_admin, archived }
    }
    /// The context kind gates access; conditional SQL independently excludes pull requests.
    /// Cancellation after deletion can still prevent audit/event completion, as before.
    pub async fn delete<P: IssueDeletionPort>(&self, port: &P, context: &Issue) -> Result<(), IssueDeletionError<P::Error>> {
        if !self.can_admin || context.kind != IssueKind::Issue {
            return Err(IssueDeletionError::Forbidden);
        }
        if self.archived {
            return Err(IssueDeletionError::Archived);
        }
        let snapshot = port.delete(context.id).await.map_err(IssueDeletionError::Port)?.ok_or(IssueDeletionError::NotFound)?;
        port.audit(&snapshot).await;
        port.publish(snapshot);
        Ok(())
    }
}
