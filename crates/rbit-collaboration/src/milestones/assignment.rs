//! Thread milestone assignment, retaining pre-read no-ops and individual effect commits.

use super::{IssueMilestone, Milestone, MilestoneId};
use std::future::Future;

#[derive(Debug, Clone, Copy)]
pub struct MilestoneAttachmentAccess {
    pub can_triage: bool,
    pub archived: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneAttachmentKind {
    Added,
    Removed,
}

pub trait MilestoneAttachmentPort: Sync {
    type Error;
    fn find(&self, id: MilestoneId) -> impl Future<Output = Result<Option<Milestone>, Self::Error>> + Send;
    fn current(&self) -> impl Future<Output = Result<Vec<IssueMilestone>, Self::Error>> + Send;
    /// Preserve the original transaction and ignore its previous-row result, as before.
    fn write(&self, id: Option<MilestoneId>) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn record(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: &str) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn publish(&self, kind: MilestoneAttachmentKind, id: MilestoneId, title: String);
}
#[derive(Debug)]
pub enum MilestoneAttachmentError<E, T = E> {
    Port(E),
    Throttle(T),
    Forbidden,
    Archived,
    Invalid(String),
}

#[derive(Debug, Clone, Copy)]
pub struct MilestoneAssignment {
    access: MilestoneAttachmentAccess,
}
impl MilestoneAssignment {
    pub fn new(access: MilestoneAttachmentAccess) -> Self {
        Self { access }
    }
    pub async fn set<P: MilestoneAttachmentPort, T>(
        &self,
        id: Option<MilestoneId>,
        factory: impl FnOnce() -> P,
        spend: impl FnOnce() -> Result<(), T>,
    ) -> Result<(), MilestoneAttachmentError<P::Error, T>> {
        if !self.access.can_triage {
            return Err(MilestoneAttachmentError::Forbidden);
        }
        if self.access.archived {
            return Err(MilestoneAttachmentError::Archived);
        }
        let port = factory();
        let new = match id {
            Some(id) => Some(
                port.find(id)
                    .await
                    .map_err(MilestoneAttachmentError::Port)?
                    .ok_or_else(|| MilestoneAttachmentError::Invalid("unknown milestone".into()))?,
            ),
            None => None,
        };
        let current = port.current().await.map_err(MilestoneAttachmentError::Port)?.pop();
        if current.as_ref().map(|old| old.id) == new.as_ref().map(|new| new.id) {
            return Ok(());
        }
        spend().map_err(MilestoneAttachmentError::Throttle)?;
        port.write(new.as_ref().map(|new| new.id)).await.map_err(MilestoneAttachmentError::Port)?;
        // The association commits before separate history/events. The pre-read can
        // be stale; retain existing race/partial-effect behavior, not an outbox claim.
        if let Some(old) = current {
            port.record(MilestoneAttachmentKind::Removed, old.id, &old.title).await.map_err(MilestoneAttachmentError::Port)?;
            port.publish(MilestoneAttachmentKind::Removed, old.id, old.title);
        }
        if let Some(new) = new {
            port.record(MilestoneAttachmentKind::Added, new.id, &new.title).await.map_err(MilestoneAttachmentError::Port)?;
            port.publish(MilestoneAttachmentKind::Added, new.id, new.title);
        }
        Ok(())
    }
}
