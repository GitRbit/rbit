//! Ordered assignment policy, preserving no-op charging and non-snapshot replacement.

use super::{AssigneeAttachmentKind, AssigneeAttachmentPort};
use crate::issues::MAX_ASSIGNEES;
use rbit_contracts::{AccountId, Timestamp};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct AssigneeAccess {
    pub can_triage: bool,
    pub archived: bool,
}
impl AssigneeAccess {
    fn require<E, T>(self) -> Result<(), AssigneeError<E, T>> {
        if !self.can_triage {
            return Err(AssigneeError::Forbidden);
        }
        if self.archived {
            return Err(AssigneeError::Archived);
        }
        Ok(())
    }
}
#[derive(Debug)]
pub enum AssigneeError<E, T = E> {
    Port(E),
    Throttle(T),
    Forbidden,
    Archived,
    Invalid(String),
}
#[derive(Debug, Clone, Copy)]
pub struct AssigneeAssignments {
    access: AssigneeAccess,
}
impl AssigneeAssignments {
    pub fn new(access: AssigneeAccess) -> Self {
        Self { access }
    }
    pub async fn change<P: AssigneeAttachmentPort, T>(
        &self,
        add: &[AccountId],
        remove: &[AccountId],
        factory: impl FnOnce() -> P,
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<(), AssigneeError<P::Error, T>> {
        self.access.require()?;
        let add = distinct(add);
        let remove: Vec<AccountId> = distinct(remove).into_iter().filter(|id| !add.contains(id)).collect();
        let port = factory();
        let current: Vec<AccountId> =
            port.current().await.map_err(AssigneeError::Port)?.into_iter().map(|(_, account)| account.id).collect();
        let new: Vec<AccountId> = add.iter().copied().filter(|id| !current.contains(id)).collect();
        let users = port.resolve_new(&new).await.map_err(AssigneeError::Port)?;
        let kept = current.iter().filter(|id| !remove.contains(id)).count();
        if kept + users.len() > MAX_ASSIGNEES {
            return Err(AssigneeError::Invalid(format!("an issue can have at most {MAX_ASSIGNEES} assignees")));
        }
        spend().map_err(AssigneeError::Throttle)?;
        let at = now();
        // Association and history commits remain separate; errors/cancellation can
        // leave an assigned/unassigned row without its transient event, as before.
        for id in current.iter().filter(|id| remove.contains(id)) {
            if port.remove(*id).await.map_err(AssigneeError::Port)? {
                port.record(AssigneeAttachmentKind::Unassigned, *id).await.map_err(AssigneeError::Port)?;
                port.publish(AssigneeAttachmentKind::Unassigned, *id);
            }
        }
        for user in &users {
            if port.add(user.id(), at).await.map_err(AssigneeError::Port)? {
                port.record(AssigneeAttachmentKind::Assigned, user.id()).await.map_err(AssigneeError::Port)?;
                port.publish(AssigneeAttachmentKind::Assigned, user.id());
            }
        }
        Ok(())
    }
    pub async fn set<P: AssigneeAttachmentPort, T>(
        &self,
        ids: &[AccountId],
        factory: impl Fn() -> P,
        spend: impl FnOnce() -> Result<(), T>,
        now: impl FnOnce() -> Timestamp,
    ) -> Result<(), AssigneeError<P::Error, T>> {
        self.access.require()?;
        let current = factory().current().await.map_err(AssigneeError::Port)?;
        let remove: Vec<AccountId> = current.iter().map(|(_, account)| account.id).filter(|id| !ids.contains(id)).collect();
        self.change(ids, &remove, factory, spend, now).await
    }
}
fn distinct<T: Ord + Copy>(items: &[T]) -> Vec<T> {
    let mut seen = BTreeSet::new();
    items.iter().copied().filter(|item| seen.insert(*item)).collect()
}
