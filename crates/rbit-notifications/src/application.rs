//! Inbox visibility filtering and mutation order; concrete stores retain SQL authority.
use crate::{
    InboxPage, Notification,
    port::{InboxAccess, InboxRepository},
};
use rbit_contracts::{AccountId, Timestamp};

#[derive(Debug, PartialEq, Eq)]
pub enum InboxError<S, A> {
    Store(S),
    Access(A),
    NotFound,
}

pub struct InboxApplication<'a, R, A> {
    repository: &'a R,
    access: &'a A,
    user: AccountId,
}
impl<'a, R: InboxRepository, A: InboxAccess> InboxApplication<'a, R, A> {
    pub fn new(repository: &'a R, access: &'a A, user: AccountId) -> Self {
        Self { repository, access, user }
    }
    pub async fn page(&self, unread_only: bool, page: u32, per_page: u32) -> Result<InboxPage, InboxError<R::Error, A::Error>> {
        let per_page = per_page.max(1);
        let page = page.max(1);
        let offset = u64::from(page - 1) * u64::from(per_page);
        let rows = self.repository.list(self.user, unread_only, per_page, offset).await.map_err(InboxError::Store)?;
        let mut repos: Vec<_> = rows.iter().map(|row| row.repo).collect();
        repos.sort_unstable_by_key(|id| id.0);
        repos.dedup();
        let readable = self.access.readable_among(&repos).await.map_err(InboxError::Access)?;
        let mut hidden = Vec::new();
        let mut notifications = Vec::with_capacity(rows.len());
        for row in rows {
            if readable.contains_key(&row.repo) {
                notifications.push(row);
            } else {
                hidden.push(row.id);
            }
        }
        if !hidden.is_empty() {
            self.repository.delete(self.user, &hidden).await.map_err(InboxError::Store)?;
        }
        let total = self.repository.count(self.user, unread_only).await.map_err(InboxError::Store)?;
        Ok(InboxPage { notifications, total, page, per_page })
    }
    pub async fn find(&self, id: i64) -> Result<Notification, InboxError<R::Error, A::Error>> {
        let row = self.repository.find(self.user, id).await.map_err(InboxError::Store)?.ok_or(InboxError::NotFound)?;
        let repo = self.access.find_repository(row.repo).await.map_err(InboxError::Access)?.ok_or(InboxError::NotFound)?;
        if !self.access.can_read(&repo).await.map_err(InboxError::Access)? {
            self.repository.delete(self.user, &[id]).await.map_err(InboxError::Store)?;
            return Err(InboxError::NotFound);
        }
        Ok(row)
    }
    pub async fn mark_read(&self, id: i64) -> Result<(), InboxError<R::Error, A::Error>> {
        if self.repository.mark_read(self.user, id).await.map_err(InboxError::Store)? { Ok(()) } else { Err(InboxError::NotFound) }
    }
    pub async fn mark_all_read(&self, now: Timestamp) -> Result<u64, InboxError<R::Error, A::Error>> {
        self.repository.mark_all_read(self.user, now).await.map_err(InboxError::Store)
    }
    /// The clock is called after visibility, matching the original unsubscribe order.
    pub async fn unsubscribe(&self, id: i64, now: impl FnOnce() -> Timestamp) -> Result<(), InboxError<R::Error, A::Error>> {
        let row = self.find(id).await?;
        self.repository.set_subscribed(row.issue, self.user, false, now()).await.map_err(InboxError::Store)?;
        self.repository.mark_read(self.user, id).await.map_err(InboxError::Store)?;
        Ok(())
    }
}

// Payload Debug bounds would unnecessarily expose stores, access owners or viewer IDs.
impl<R, A> std::fmt::Debug for InboxApplication<'_, R, A> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("InboxApplication").finish_non_exhaustive()
    }
}
#[cfg(test)]
#[path = "application_debug_tests.rs"]
mod debug_tests;
