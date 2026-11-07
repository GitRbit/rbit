use rbit_collaboration::{
    IssueState,
    threads::{IssueId, IssueKind, IssueNumber},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_notifications::{
    Notification, NotificationReason, Subscription,
    port::{InboxAccess, InboxRepository},
};
use std::{
    collections::HashMap,
    future::{Future, Ready, ready},
    sync::Mutex,
};
pub fn row(id: i64, repo: i64) -> Notification {
    Notification {
        id,
        user: AccountId(7),
        issue: IssueId(id),
        repo: RepoId(repo),
        reason: NotificationReason::Mention,
        unread: true,
        last_actor: None,
        updated_at: Timestamp(1),
        number: IssueNumber::new(1).unwrap(),
        kind: IssueKind::Issue,
        state: IssueState::Open,
        state_reason: None,
        title: "<unsafe>".into(),
        owner: "owner".into(),
        repo_name: "repo".into(),
    }
}
#[derive(Default)]
pub struct Fake {
    pub calls: Mutex<Vec<String>>,
    pub rows: Mutex<Vec<Notification>>,
    pub readable: Vec<i64>,
    pub total: u64,
    pub denied: bool,
    pub missing: bool,
    pub missing_repository: bool,
    pub fail: &'static str,
}
impl Fake {
    fn result<T>(&self, name: &str, value: T) -> Ready<Result<T, &'static str>> {
        self.calls.lock().unwrap().push(name.into());
        ready(if name.starts_with(self.fail) && !self.fail.is_empty() { Err("fixture") } else { Ok(value) })
    }
    pub fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl InboxRepository for Fake {
    type Error = &'static str;
    fn has_unread(&self, user: AccountId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.result(&format!("badge:{}", user.0), true)
    }
    fn mark_issue_read(&self, user: AccountId, issue: IssueId) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.result(&format!("thread-read:{}:{}", user.0, issue.0), true)
    }
    fn thread_state(
        &self,
        issue: IssueId,
        user: AccountId,
    ) -> impl Future<Output = Result<(Option<Subscription>, bool), Self::Error>> + Send {
        self.result(
            &format!("state:{}:{}", issue.0, user.0),
            (Some(Subscription { reason: NotificationReason::Author, subscribed: false }), true),
        )
    }
    fn subscription(&self, _: IssueId, _: AccountId) -> impl Future<Output = Result<Option<Subscription>, Self::Error>> + Send {
        self.result("subscription", None)
    }
    fn set_subscribed(
        &self,
        issue: IssueId,
        user: AccountId,
        subscribed: bool,
        now: Timestamp,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.result(&format!("subscribe:{}:{}:{subscribed}:{}", issue.0, user.0, now.0), ())
    }
    fn list(
        &self,
        user: AccountId,
        unread: bool,
        limit: u32,
        offset: u64,
    ) -> impl Future<Output = Result<Vec<Notification>, Self::Error>> + Send {
        let rows = std::mem::take(&mut *self.rows.lock().unwrap());
        self.result(&format!("list:{}:{unread}:{limit}:{offset}", user.0), rows)
    }
    fn count(&self, user: AccountId, unread: bool) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.result(&format!("count:{}:{unread}", user.0), self.total)
    }
    fn find(&self, user: AccountId, id: i64) -> impl Future<Output = Result<Option<Notification>, Self::Error>> + Send {
        self.result(&format!("find:{}:{id}", user.0), (!self.missing).then(|| row(id, 4)))
    }
    fn delete(&self, user: AccountId, ids: &[i64]) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.result(&format!("delete:{}:{ids:?}", user.0), u64::try_from(ids.len()).unwrap())
    }
    fn mark_read(&self, user: AccountId, id: i64) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.result(&format!("read:{}:{id}", user.0), !self.missing)
    }
    fn mark_all_read(&self, user: AccountId, now: Timestamp) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.result(&format!("all:{}:{}", user.0, now.0), self.total)
    }
}
impl InboxAccess for Fake {
    type Error = &'static str;
    type Repository = ();
    fn readable_among(&self, repos: &[RepoId]) -> impl Future<Output = Result<HashMap<RepoId, ()>, Self::Error>> + Send {
        self.result(&format!("acl:{repos:?}"), self.readable.iter().map(|id| (RepoId(*id), ())).collect())
    }
    fn find_repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<()>, Self::Error>> + Send {
        self.result(&format!("repository:{}", repo.0), (!self.missing_repository).then_some(()))
    }
    fn can_read(&self, (): &()) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        self.result("can-read", !self.denied)
    }
}
