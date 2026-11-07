//! Runtime composition of feature-owned inbox behavior; no background owner enters the port.
use super::{InboxPage, NOTIFICATIONS_PER_PAGE, Notification, NotificationService, Subscription};
use crate::{ServiceError, repositories::viewer_grants};
use rbit_auth::repo_access;
use rbit_core::{AccountId, IssueId, RepoId, Timestamp, User, repository::OwnedRepository};
use rbit_notifications::{
    application::{InboxApplication, InboxError},
    port::InboxAccess,
    signals,
};
use rbit_storage::{Database, NotificationInboxStore, StorageError};
use std::collections::HashMap;

struct InboxView<'a> {
    database: &'a Database,
    user: &'a User,
}
impl InboxAccess for InboxView<'_> {
    type Error = ServiceError;
    type Repository = OwnedRepository;
    async fn readable_among(&self, repos: &[RepoId]) -> Result<HashMap<RepoId, OwnedRepository>, ServiceError> {
        Ok(self.database.repositories().readable_among(repos, self.user.id(), self.user.is_admin).await?)
    }
    async fn find_repository(&self, repo: RepoId) -> Result<Option<OwnedRepository>, ServiceError> {
        Ok(self.database.admin().find_repository(repo).await?)
    }
    async fn can_read(&self, repo: &OwnedRepository) -> Result<bool, ServiceError> {
        let grants = viewer_grants(&self.database.repositories(), Some(self.user), repo).await?;
        Ok(repo_access(Some(self.user), repo, &grants).can_read())
    }
}
fn error(error: InboxError<StorageError, ServiceError>) -> ServiceError {
    match error {
        InboxError::Store(error) => error.into(),
        InboxError::Access(error) => error,
        InboxError::NotFound => ServiceError::NotFound,
    }
}
impl NotificationService {
    /// Disabled badges do not construct a store or query storage.
    pub async fn has_unread(&self, user: AccountId) -> Result<bool, ServiceError> {
        Ok(signals::has_unread(self.enabled(), user, || NotificationInboxStore::new(self.ctx.database.notifications())).await?)
    }
    /// Viewing a thread logs read-side failures and returns false, preserving its best-effort contract.
    pub async fn mark_thread_read(&self, user: AccountId, issue: IssueId) -> bool {
        signals::mark_thread_read(self.enabled(), user, issue, || NotificationInboxStore::new(self.ctx.database.notifications()))
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error,user=user.0,issue=issue.0,"cannot mark a notification read");
                false
            })
    }
    pub async fn subscription(&self, user: AccountId, issue: IssueId) -> Result<Option<Subscription>, ServiceError> {
        Ok(self.ctx.database.notifications().subscription(issue, user).await?)
    }
    pub async fn thread_state(&self, user: AccountId, issue: IssueId) -> Result<(Option<Subscription>, bool), ServiceError> {
        Ok(signals::thread_state(self.enabled(), user, issue, &NotificationInboxStore::new(self.ctx.database.notifications())).await?)
    }
    pub async fn set_subscribed(&self, user: AccountId, issue: IssueId, subscribed: bool) -> Result<(), ServiceError> {
        Ok(self.ctx.database.notifications().set_subscribed(issue, user, subscribed, Timestamp::now()).await?)
    }
    pub async fn inbox(&self, user: &User, unread_only: bool, page: u32) -> Result<InboxPage, ServiceError> {
        self.inbox_page(user, unread_only, page.max(1), NOTIFICATIONS_PER_PAGE).await
    }
    pub async fn inbox_page(&self, user: &User, unread_only: bool, page: u32, per_page: u32) -> Result<InboxPage, ServiceError> {
        let store = NotificationInboxStore::new(self.ctx.database.notifications());
        let access = InboxView { database: &self.ctx.database, user };
        InboxApplication::new(&store, &access, user.id()).page(unread_only, page, per_page).await.map_err(error)
    }
    pub async fn find(&self, user: &User, id: i64) -> Result<Notification, ServiceError> {
        let store = NotificationInboxStore::new(self.ctx.database.notifications());
        let access = InboxView { database: &self.ctx.database, user };
        InboxApplication::new(&store, &access, user.id()).find(id).await.map_err(error)
    }
    pub async fn mark_read(&self, user: &User, id: i64) -> Result<(), ServiceError> {
        let store = NotificationInboxStore::new(self.ctx.database.notifications());
        let access = InboxView { database: &self.ctx.database, user };
        InboxApplication::new(&store, &access, user.id()).mark_read(id).await.map_err(error)
    }
    pub async fn mark_all_read(&self, user: &User) -> Result<u64, ServiceError> {
        let store = NotificationInboxStore::new(self.ctx.database.notifications());
        let access = InboxView { database: &self.ctx.database, user };
        InboxApplication::new(&store, &access, user.id()).mark_all_read(Timestamp::now()).await.map_err(error)
    }
    pub async fn unsubscribe(&self, user: &User, id: i64) -> Result<(), ServiceError> {
        let store = NotificationInboxStore::new(self.ctx.database.notifications());
        let access = InboxView { database: &self.ctx.database, user };
        InboxApplication::new(&store, &access, user.id()).unsubscribe(id, Timestamp::now).await.map_err(error)
    }
}
