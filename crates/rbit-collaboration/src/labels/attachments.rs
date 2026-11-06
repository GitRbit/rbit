//! Label assignment policy, including the original duplicate and partial-effect behavior.

use super::{LabelAttachmentKind, LabelAttachmentMutation, LabelAttachmentPort, LabelId};
use crate::issues::MAX_LABELS_PER_ISSUE;
use rbit_contracts::Timestamp;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct LabelAttachmentAccess {
    pub can_triage: bool,
    pub archived: bool,
}
impl LabelAttachmentAccess {
    fn require<E, P>(self) -> Result<(), LabelAttachmentError<E, P>> {
        if !self.can_triage {
            return Err(LabelAttachmentError::Forbidden);
        }
        if self.archived {
            return Err(LabelAttachmentError::Archived);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum LabelAttachmentError<E, P = E> {
    Port(E),
    Throttle(P),
    Forbidden,
    Archived,
    Invalid(String),
}

#[derive(Debug)]
pub struct LabelAttachments<'a, P> {
    port: &'a P,
    access: LabelAttachmentAccess,
}
impl<'a, P: LabelAttachmentPort> LabelAttachments<'a, P> {
    pub fn new(port: &'a P, access: LabelAttachmentAccess) -> Self {
        Self { port, access }
    }

    pub async fn change<T>(
        &self,
        add: &[LabelId],
        remove: &[LabelId],
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<(), LabelAttachmentError<P::Error, T>> {
        self.access.require()?;
        let add = self.port.resolve(&distinct(add)).await.map_err(LabelAttachmentError::Port)?;
        let remove: Vec<LabelId> = distinct(remove).into_iter().filter(|id| !add.iter().any(|label| label.id == *id)).collect();
        let mutation = self.port.mutation();
        let current = mutation.current().await.map_err(LabelAttachmentError::Port)?;
        let kept = current.iter().filter(|(_, label)| !remove.contains(&label.id)).count();
        let added = add.iter().filter(|label| !current.iter().any(|(_, old)| old.id == label.id)).count();
        if kept + added > MAX_LABELS_PER_ISSUE {
            return Err(LabelAttachmentError::Invalid(format!("an issue can have at most {MAX_LABELS_PER_ISSUE} labels")));
        }
        spend().map_err(LabelAttachmentError::Throttle)?;
        let at = now();
        // Each association mutation commits before its separate timeline record.
        // Preserve this existing partial-commit boundary; no outbox or rollback promise.
        for (_, label) in current.iter().filter(|(_, label)| remove.contains(&label.id)) {
            if mutation.remove(label.id).await.map_err(LabelAttachmentError::Port)? {
                mutation.record(LabelAttachmentKind::Unlabeled, label).await.map_err(LabelAttachmentError::Port)?;
                mutation.publish(LabelAttachmentKind::Unlabeled, label.clone());
            }
        }
        for label in &add {
            if mutation.add(label.id, at).await.map_err(LabelAttachmentError::Port)? {
                let snapshot = label.snapshot();
                mutation.record(LabelAttachmentKind::Labeled, &snapshot).await.map_err(LabelAttachmentError::Port)?;
                mutation.publish(LabelAttachmentKind::Labeled, snapshot);
            }
        }
        Ok(())
    }

    /// Retains the initial read and the delegated change's separate second read.
    pub async fn set<T>(
        &self,
        labels: &[LabelId],
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<(), LabelAttachmentError<P::Error, T>> {
        self.access.require()?;
        let current = self.port.mutation().current().await.map_err(LabelAttachmentError::Port)?;
        let remove: Vec<LabelId> = current.iter().map(|(_, label)| label.id).filter(|id| !labels.contains(id)).collect();
        self.change(labels, &remove, spend, now).await
    }
}
fn distinct<T: Ord + Copy>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items.iter().copied().filter(|item| seen.insert(*item)).collect()
}
