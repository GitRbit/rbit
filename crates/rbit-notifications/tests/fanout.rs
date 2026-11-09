#[allow(dead_code)]
mod support;
use rbit_collaboration::{
    comments::CommentId,
    threads::{IssueId, IssueText},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_notifications::{
    Activity, ActivityEvent, MentionedAccount, NOTIFICATION_BATCH_SIZE, NotificationReason as Reason, NotificationRecipient, ThreadText,
    fanout::FanoutApplication,
    port::{ActivitySource, DeliveryRepository, RecipientAccess},
    retention, subscriptions,
};
use std::{
    future::{Future, Ready, ready},
    sync::Mutex,
};

const ISSUE: IssueId = IssueId(5);
const REPO: RepoId = RepoId(9);
const NOW: Timestamp = Timestamp(1_000);

/// One fake for every port; `trace` records storage-visible calls in order.
#[derive(Default)]
struct World {
    trace: Mutex<Vec<String>>,
    recorded: Mutex<Vec<NotificationRecipient>>,
    thread: Option<(Option<i64>, &'static str)>,
    assignees: Vec<i64>,
    comment: Option<&'static str>,
    /// `(id, organization)` for any non-empty mentioning text.
    mentioned: Vec<(i64, bool)>,
    known: Vec<i64>,
    subscribers: Vec<(i64, Reason)>,
    unreadable: Vec<i64>,
    /// The repository disappears once this many batches were authorized.
    repo_gone_after: Option<usize>,
}
impl World {
    fn call<T>(&self, entry: String, value: T) -> Ready<Result<T, &'static str>> {
        self.trace.lock().unwrap().push(entry);
        ready(Ok(value))
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
    async fn deliver(&self, actor: Option<i64>, activity: Activity) -> Option<Vec<(AccountId, Reason)>> {
        let event = ActivityEvent { repo: REPO, issue: ISSUE, actor: actor.map(AccountId), activity };
        FanoutApplication::new(self, self, self).deliver(event, NOW).await.unwrap()
    }
}
fn ids(values: &[i64]) -> Vec<AccountId> {
    values.iter().copied().map(AccountId).collect()
}
impl DeliveryRepository for World {
    type Error = &'static str;
    fn participate(
        &self,
        issue: IssueId,
        user: AccountId,
        reason: Reason,
        at: Timestamp,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.call(format!("participate:{}:{}:{}:{}", issue.0, user.0, reason.as_str(), at.0), ())
    }
    fn subscribers(&self, _: IssueId) -> impl Future<Output = Result<Vec<(AccountId, Reason)>, Self::Error>> + Send {
        self.call("subscribers".into(), self.subscribers.iter().map(|(id, reason)| (AccountId(*id), *reason)).collect())
    }
    fn known_participants(&self, _: IssueId, users: &[AccountId]) -> impl Future<Output = Result<Vec<AccountId>, Self::Error>> + Send {
        let known = users.iter().copied().filter(|user| self.known.contains(&user.0)).collect();
        self.call(format!("known:{:?}", users.iter().map(|user| user.0).collect::<Vec<_>>()), known)
    }
    fn notify_many(
        &self,
        issue: IssueId,
        repo: RepoId,
        actor: Option<AccountId>,
        at: Timestamp,
        recipients: &[NotificationRecipient],
    ) -> impl Future<Output = Result<Vec<(AccountId, Reason)>, Self::Error>> + Send {
        self.recorded.lock().unwrap().extend_from_slice(recipients);
        let entry = format!("notify:{}:{}:{:?}:{}:{}", issue.0, repo.0, actor.map(|a| a.0), at.0, recipients.len());
        self.call(entry, recipients.iter().map(|recipient| (recipient.user, recipient.reason)).collect())
    }
    fn prune_read(&self, before: Timestamp) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        self.call(format!("prune:{}", before.0), 3)
    }
}
impl ActivitySource for World {
    type Error = &'static str;
    fn thread(&self, issue: IssueId) -> impl Future<Output = Result<Option<ThreadText>, Self::Error>> + Send {
        let thread = self.thread.map(|(author, body)| ThreadText { author: author.map(AccountId), body: IssueText::body(body).unwrap() });
        self.call(format!("thread:{}", issue.0), thread)
    }
    fn assignees(&self, _: IssueId) -> impl Future<Output = Result<Vec<AccountId>, Self::Error>> + Send {
        self.call("assignees".into(), ids(&self.assignees))
    }
    fn comment_body(&self, _: IssueId, comment: CommentId) -> impl Future<Output = Result<Option<IssueText>, Self::Error>> + Send {
        self.call(format!("comment:{}", comment.0), self.comment.map(|body| IssueText::comment(body).unwrap()))
    }
    fn mentioned(&self, text: &str) -> impl Future<Output = Result<Vec<MentionedAccount>, Self::Error>> + Send {
        let accounts = if text.contains('@') {
            self.mentioned.iter().map(|(id, organization)| MentionedAccount { id: AccountId(*id), organization: *organization }).collect()
        } else {
            Vec::new()
        };
        self.call("mentioned".into(), accounts)
    }
}
impl RecipientAccess for World {
    type Error = &'static str;
    fn readers(&self, repo: RepoId, users: &[AccountId]) -> impl Future<Output = Result<Option<Vec<AccountId>>, Self::Error>> + Send {
        let batches = self.trace().iter().filter(|entry| entry.starts_with("readers")).count();
        let readers = (self.repo_gone_after != Some(batches))
            .then(|| users.iter().copied().filter(|user| !self.unreadable.contains(&user.0)).collect());
        self.call(format!("readers:{}:{}", repo.0, users.len()), readers)
    }
}

#[tokio::test]
async fn opening_subscribes_the_author_and_notifies_assignees_and_new_people_mentioned() {
    let world = World {
        thread: Some((Some(1), "hello @someone")),
        assignees: vec![2],
        // 3 is new, 4 an organization, 5 already takes part, 2 is also assigned.
        mentioned: vec![(3, false), (4, true), (5, false), (2, false)],
        known: vec![5],
        ..World::default()
    };
    let notified = world.deliver(Some(1), Activity::Opened).await.unwrap();
    assert_eq!(notified, [(AccountId(2), Reason::Assigned), (AccountId(3), Reason::Mention)]);
    assert_eq!(
        world.trace(),
        [
            "thread:5",
            "participate:5:1:author:1000",
            "assignees",
            "mentioned",
            "known:[3, 5, 2]",
            "readers:9:2",
            "notify:5:9:Some(1):1000:2",
        ]
    );
    let participation: Vec<_> = world.recorded.lock().unwrap().iter().map(|r| (r.user.0, r.participation)).collect();
    assert_eq!(participation, [(2, Some(Reason::Assigned)), (3, Some(Reason::Mention))]);
}

#[tokio::test]
async fn a_missing_thread_or_an_unmentioning_edit_writes_nothing() {
    let world = World::default();
    assert_eq!(world.deliver(Some(1), Activity::Opened).await, None);
    assert_eq!(world.trace(), ["thread:5"]);
    let world = World { thread: Some((Some(1), "no mentions")), mentioned: vec![(3, false)], ..World::default() };
    assert_eq!(world.deliver(Some(1), Activity::BodyEdited).await, None);
    assert_eq!(world.trace(), ["thread:5", "mentioned"]);
}

#[tokio::test]
async fn the_actor_is_never_notified_but_still_takes_part_unless_only_mentioned() {
    let world = World::default();
    assert_eq!(world.deliver(Some(1), Activity::Assigned(AccountId(1))).await, Some(Vec::new()));
    assert_eq!(world.trace(), ["participate:5:1:assigned:1000"], "no batch is authorized for nobody");

    let world = World { comment: Some("@me"), mentioned: vec![(1, false)], ..World::default() };
    assert_eq!(world.deliver(Some(1), Activity::CommentEdited(CommentId(8))).await, Some(Vec::new()));
    assert_eq!(world.trace(), ["comment:8", "mentioned", "known:[1]"], "a self-mention neither subscribes nor notifies");
}

#[tokio::test]
async fn subscribers_hear_of_state_changes_with_their_strongest_reason_if_they_can_read() {
    let world = World {
        subscribers: vec![(1, Reason::Author), (2, Reason::Comment), (3, Reason::Manual), (4, Reason::Comment)],
        unreadable: vec![3],
        ..World::default()
    };
    let notified = world.deliver(Some(1), Activity::StateChanged).await.unwrap();
    assert_eq!(notified, [(AccountId(2), Reason::Comment), (AccountId(4), Reason::Comment)]);
    assert_eq!(world.trace(), ["subscribers", "readers:9:3", "notify:5:9:Some(1):1000:2"]);
    assert!(world.recorded.lock().unwrap().iter().all(|recipient| recipient.participation.is_none()));

    let world = World { subscribers: vec![(2, Reason::Comment)], comment: Some("@two"), mentioned: vec![(2, false)], ..World::default() };
    let notified = world.deliver(None, Activity::CommentCreated(CommentId(8))).await.unwrap();
    assert_eq!(notified, [(AccountId(2), Reason::Mention)], "an automated comment subscribes nobody");
    assert!(!world.trace().iter().any(|entry| entry.starts_with("participate")));
}

#[tokio::test]
async fn reviews_and_review_requests_record_their_reasons() {
    let world = World { subscribers: vec![(2, Reason::Author)], ..World::default() };
    assert_eq!(world.deliver(Some(3), Activity::ReviewSubmitted).await.unwrap(), [(AccountId(2), Reason::Author)]);
    assert_eq!(world.trace()[0], "participate:5:3:reviewed:1000");
    let world = World::default();
    assert_eq!(world.deliver(Some(1), Activity::ReviewRequested(AccountId(2))).await.unwrap(), [(AccountId(2), Reason::ReviewRequested)]);
}

#[tokio::test]
async fn every_batch_is_authorized_afresh_and_a_vanished_repository_stops_delivery() {
    let subscribers: Vec<_> = (10..10 + i64::try_from(NOTIFICATION_BATCH_SIZE * 2 + 7).unwrap()).map(|id| (id, Reason::Comment)).collect();
    let world = World { subscribers: subscribers.clone(), ..World::default() };
    let notified = world.deliver(Some(1), Activity::StateChanged).await.unwrap();
    assert_eq!(notified.len(), subscribers.len());
    let readers: Vec<_> = world.trace().into_iter().filter(|entry| entry.starts_with("readers")).collect();
    assert_eq!(readers, ["readers:9:100", "readers:9:100", "readers:9:7"]);

    let world = World { subscribers, repo_gone_after: Some(1), ..World::default() };
    let notified = world.deliver(Some(1), Activity::StateChanged).await.unwrap();
    assert_eq!(notified.len(), NOTIFICATION_BATCH_SIZE);
    assert_eq!(world.trace().last().unwrap(), "readers:9:100", "nothing is written after the repository is gone");
}

#[tokio::test]
async fn retention_saturates_and_prunes_once() {
    assert_eq!(retention::cutoff(Timestamp(10 * 86_400), 3), Timestamp(7 * 86_400));
    assert_eq!(retention::cutoff(Timestamp(i64::MIN + 1), u32::MAX), Timestamp(i64::MIN));
    let world = World::default();
    assert_eq!(retention::prune_read(&world, Timestamp(42)).await, Ok(3));
    assert_eq!(world.trace(), ["prune:42"]);
    assert_eq!(format!("{:?}", FanoutApplication::new(&world, &world, &world)), "FanoutApplication { .. }");
}

#[tokio::test]
async fn manual_subscriptions_reach_the_store_unchanged() {
    let fake = support::Fake::default();
    assert_eq!(subscriptions::subscription(&fake, AccountId(7), IssueId(3)).await, Ok(None));
    subscriptions::set_subscribed(&fake, AccountId(7), IssueId(3), false, Timestamp(4)).await.unwrap();
    assert_eq!(fake.trace(), ["subscription", "subscribe:3:7:false:4"]);
}
