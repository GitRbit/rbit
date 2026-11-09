#![cfg(feature = "applications")]
//! Participant refusals and subscriber activity routing without concrete persistence.

#[path = "support/thread_fixtures.rs"]
mod thread_fixtures;

use std::sync::Mutex;

use rbit_collaboration::{
    comments::CommentId,
    issues::{
        AccountNamePort, IssueParticipants, ParticipantError, ParticipantPort, ReviewText, ThreadActivity, ThreadActivityPort, users_named,
    },
    labels::{Label, LabelId},
    threads::{Issue, IssueId, IssueNumber, IssueText},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};

#[derive(Default)]
struct Script {
    calls: Mutex<Vec<String>>,
    deleted: bool,
    pending: bool,
}

impl Script {
    fn call(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

fn account(id: i64, name: &str, kind: AccountKind) -> Account {
    Account { id: AccountId(id), kind, name: name.parse().unwrap(), display_name: None, bio: None, created_at: Timestamp(1) }
}

fn user(id: i64) -> User {
    User {
        account: account(id, &format!("u{id}"), AccountKind::User),
        email: format!("u{id}@example.com").parse().unwrap(),
        is_admin: false,
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl AccountNamePort for Script {
    type Error = &'static str;
    async fn accounts_named(&self, _: &[String]) -> Result<Vec<Account>, &'static str> {
        Ok(vec![account(1, "Ada", AccountKind::User), account(2, "acme", AccountKind::Organization)])
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ParticipantPort for Script {
    type Error = &'static str;
    async fn find_label(&self, repo: RepoId, id: LabelId) -> Result<Option<Label>, &'static str> {
        self.call(format!("label {} {}", repo.0, id.0));
        Ok((id.0 < 10).then(|| Label {
            id,
            repo_id: repo,
            name: format!("l{}", id.0),
            color: "#000000".into(),
            description: String::new(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        }))
    }
    async fn find_active_user(&self, id: AccountId) -> Result<Option<User>, &'static str> {
        self.call(format!("user {}", id.0));
        Ok((id.0 < 10).then(|| user(id.0)))
    }
    async fn can_push(&self, user: &User) -> Result<bool, &'static str> {
        self.call(format!("push {}", user.id().0));
        Ok(user.id().0 % 2 == 1)
    }
    async fn assignable(&self, _: RepoId, prefix: &str, limit: u32) -> Result<Vec<Account>, &'static str> {
        self.call(format!("assignable {prefix:?} {limit}"));
        Ok(Vec::new())
    }
    async fn comment_thread(&self, _: RepoId, id: CommentId) -> Result<Option<IssueNumber>, &'static str> {
        Ok((id.0 == 1).then(|| IssueNumber::new(5).unwrap()))
    }
}

fn invalid<T: std::fmt::Debug>(result: Result<T, ParticipantError<&'static str>>) -> String {
    match result {
        Err(ParticipantError::Invalid(message)) => message,
        other => panic!("expected an invalid request, got {other:?}"),
    }
}

#[tokio::test]
async fn participants_keep_the_legacy_refusals() {
    let port = Script::default();
    let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
    assert_eq!(users_named(&port, &names(&["ada"])).await.unwrap().len(), 1, "names match case-insensitively");
    assert_eq!(
        invalid(users_named(&port, &names(&["ada", "acme"])).await),
        "there is no user called “acme”",
        "organisations are not users"
    );

    let participants = IssueParticipants::new(&port, RepoId(3));
    assert_eq!(participants.labels(&[LabelId(1), LabelId(2)]).await.unwrap().len(), 2);
    assert_eq!(invalid(participants.labels(&[LabelId(11), LabelId(1)]).await), "unknown label");
    assert_eq!(invalid(participants.assignees(&[AccountId(12)]).await), "unknown user");
    assert_eq!(
        invalid(participants.assignees(&[AccountId(1), AccountId(2), AccountId(3)]).await),
        "u2 cannot be assigned: only people who can push to the repository can"
    );
    assert!(matches!(participants.assignable(false, "a").await, Err(ParticipantError::NotFound)));
    participants.assignable(true, "  b ").await.unwrap();
    assert!(matches!(participants.comment_thread(CommentId(2)).await, Err(ParticipantError::NotFound)));
    assert_eq!(participants.comment_thread(CommentId(1)).await.unwrap().get(), 5);
    assert_eq!(
        port.calls(),
        ["label 3 1", "label 3 2", "label 3 11", "user 12", "user 1", "push 1", "user 2", "push 2", "assignable \"b\" 100"],
        "refusals stop at the first failing id; unreadable repositories list nobody"
    );
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl ThreadActivityPort for Script {
    type Error = &'static str;
    type Review = i64;
    async fn thread(&self, issue: IssueId) -> Result<Option<Issue>, &'static str> {
        self.call(format!("thread {}", issue.0));
        let mut thread = thread_fixtures::issue();
        thread.body = IssueText::body("body @ada").unwrap();
        Ok((!self.deleted).then_some(thread))
    }
    async fn comment_body(&self, _: IssueId, comment: CommentId) -> Result<Option<String>, &'static str> {
        self.call(format!("comment {}", comment.0));
        Ok(Some("comment #2".into()))
    }
    async fn review(&self, _: IssueId, review: i64) -> Result<Option<ReviewText>, &'static str> {
        self.call(format!("review {review}"));
        Ok(Some(ReviewText { submitted: !self.pending, body: "review".into() }))
    }
    async fn review_comments(&self, _: IssueId, _: i64) -> Result<Vec<String>, &'static str> {
        self.call("review comments".into());
        Ok(vec!["line".into()])
    }
    async fn record_mentions(&self, issue: IssueId, text: &str) -> Result<(), &'static str> {
        self.call(format!("mentions {} {text}", issue.0));
        Ok(())
    }
    async fn record_cross_references(&self, source: &Issue, actor: Option<AccountId>, text: &str) -> Result<(), &'static str> {
        self.call(format!("references {} {:?} {text}", source.id.0, actor.map(|id| id.0)));
        Ok(())
    }
}

#[tokio::test]
async fn activity_indexes_and_references_what_the_legacy_subscriber_did() {
    let port = Script::default();
    let activity = ThreadActivity::new(&port);
    activity.earlier_text(IssueId(1), "old @bob").await.unwrap();
    activity.thread_written(IssueId(1), Some(AccountId(4))).await.unwrap();
    activity.comment_written(IssueId(1), CommentId(8), Some(AccountId(4))).await.unwrap();
    activity.comment_written(IssueId(1), CommentId(9), None).await.unwrap();
    activity.review_changed(IssueId(1), 6, Some("before")).await.unwrap();
    assert_eq!(
        port.calls(),
        [
            "mentions 1 old @bob",
            "thread 1",
            "mentions 1 body @ada",
            "references 1 Some(4) body @ada",
            "thread 1",
            "comment 8",
            "mentions 1 comment #2",
            "references 1 Some(4) comment #2",
            "thread 1",
            "comment 9",
            "mentions 1 comment #2",
            "review 6",
            "mentions 1 before",
            "mentions 1 review",
            "review comments",
            "mentions 1 line",
        ],
        "rbit Actions comments reference nothing"
    );

    let pending = Script { pending: true, ..Script::default() };
    ThreadActivity::new(&pending).review_changed(IssueId(1), 6, Some("before")).await.unwrap();
    assert_eq!(pending.calls(), ["review 6"], "pending reviews stay private");
    let deleted = Script { deleted: true, ..Script::default() };
    ThreadActivity::new(&deleted).thread_written(IssueId(1), None).await.unwrap();
    ThreadActivity::new(&deleted).comment_written(IssueId(1), CommentId(8), None).await.unwrap();
    assert_eq!(deleted.calls(), ["thread 1", "thread 1", "comment 8"], "deleted threads are skipped");
}
