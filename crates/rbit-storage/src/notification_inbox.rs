//! Feature port forwarding; original SQL and row decoding remain unchanged.
use crate::{StorageError, sqlite::notifications::NotificationStore};
use rbit_core::{AccountId, IssueId, Timestamp};
use rbit_notifications::{Notification, Subscription, port::InboxRepository};
/// One owned store handle, borrowed by the feature application. Construction moves
/// the existing handle; it adds no query, heap allocation or underlying handle clone.
pub struct NotificationInboxStore {
    store: NotificationStore,
}
impl NotificationInboxStore {
    pub fn new(store: NotificationStore) -> Self {
        Self { store }
    }
}
impl InboxRepository for NotificationInboxStore {
    type Error = StorageError;
    fn has_unread(&self, user: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        NotificationStore::has_unread(&self.store, user)
    }
    fn mark_issue_read(&self, user: AccountId, issue: IssueId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        NotificationStore::mark_issue_read(&self.store, user, issue)
    }
    fn thread_state(
        &self,
        issue: IssueId,
        user: AccountId,
    ) -> impl Future<Output = Result<(Option<Subscription>, bool), Self::Error>> + Send {
        NotificationStore::thread_state(&self.store, issue, user)
    }
    fn subscription(&self, issue: IssueId, user: AccountId) -> impl Future<Output = Result<Option<Subscription>, Self::Error>> + Send {
        NotificationStore::subscription(&self.store, issue, user)
    }
    fn set_subscribed(
        &self,
        issue: IssueId,
        user: AccountId,
        subscribed: bool,
        now: Timestamp,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        NotificationStore::set_subscribed(&self.store, issue, user, subscribed, now)
    }
    fn list(
        &self,
        user: AccountId,
        unread_only: bool,
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = Result<Vec<Notification>, Self::Error>> + Send {
        NotificationStore::list(&self.store, user, unread_only, limit, offset)
    }
    fn count(&self, user: AccountId, unread_only: bool) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        NotificationStore::count(&self.store, user, unread_only)
    }
    fn find(&self, user: AccountId, id: i64) -> impl Future<Output = Result<Option<Notification>, Self::Error>> + Send {
        NotificationStore::find(&self.store, user, id)
    }
    fn delete(&self, user: AccountId, ids: &[i64]) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        NotificationStore::delete(&self.store, user, ids)
    }
    fn mark_read(&self, user: AccountId, id: i64) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        NotificationStore::mark_read(&self.store, user, id)
    }
    fn mark_all_read(&self, user: AccountId, now: Timestamp) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        NotificationStore::mark_all_read(&self.store, user, now)
    }
}

impl std::fmt::Debug for NotificationInboxStore {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("NotificationInboxStore").finish_non_exhaustive()
    }
}
