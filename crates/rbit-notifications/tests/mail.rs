use rbit_collaboration::threads::{IssueId, IssueKind, IssueNumber};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_notifications::{
    NOTIFICATION_BATCH_SIZE, NotificationReason as Reason,
    mail::{
        activity::MailActivity,
        delivery::{MailScope, deliver},
        digest::{DigestDefaults, DigestRun, DigestSection, send_digests},
        policy::{Cadence, MailPreference},
        port::{DigestOutbox, MailOutbox},
        records::{Claim, ClaimedDigest, DigestItem, DigestState, MailRecipient, NewDigestItem, PendingDigest},
    },
};
use std::{
    collections::HashSet,
    future::{Future, Ready, ready},
    sync::Mutex,
};

/// Every port of both outboxes; `trace` records calls in order.
#[derive(Default)]
struct World {
    trace: Mutex<Vec<String>>,
    /// `(account, preference, cadence)` rows that still qualify.
    users: Vec<(i64, Option<MailPreference>, Cadence)>,
    unreadable: Vec<i64>,
    already_claimed: Vec<i64>,
    repo_gone_after: Option<usize>,
    fail_render: bool,
    items: Vec<DigestItem>,
    refuse: bool,
}
type Ready2<T> = Ready<Result<T, &'static str>>;
impl World {
    fn call<T>(&self, entry: String, value: T) -> Ready2<T> {
        self.trace.lock().unwrap().push(entry);
        ready(Ok(value))
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
    fn recipient(&self, account: i64) -> Option<MailRecipient<()>> {
        let &(id, preference, cadence) = self.users.iter().find(|(id, _, _)| *id == account)?;
        Some(MailRecipient { account: AccountId(id), preference, cadence, user: () })
    }
}
fn activity() -> MailActivity {
    MailActivity {
        key: "comment-9".into(),
        headline: "bob commented.".into(),
        excerpt: "Same\n\nhere".into(),
        anchor: None,
        opening: false,
    }
}
fn scope() -> MailScope {
    MailScope { repo: RepoId(3), issue: IssueId(4), actor: Some(AccountId(1)), now: Timestamp(50), default: MailPreference::Participating }
}

impl MailOutbox for World {
    type Error = &'static str;
    type Repository = ();
    type User = ();
    type Mail = i64;
    fn repository(&self, repo: RepoId) -> impl Future<Output = Result<Option<()>, &'static str>> + Send {
        let batches = self.trace().iter().filter(|entry| entry.starts_with("repository")).count();
        self.call(format!("repository:{}", repo.0), (self.repo_gone_after != Some(batches)).then_some(()))
    }
    fn recipients(&self, accounts: &[AccountId]) -> impl Future<Output = Result<Vec<MailRecipient<()>>, &'static str>> + Send {
        self.call(format!("recipients:{}", accounts.len()), accounts.iter().filter_map(|id| self.recipient(id.0)).collect())
    }
    fn readers(&self, (): &(), users: &[&MailRecipient<()>]) -> impl Future<Output = Result<HashSet<AccountId>, &'static str>> + Send {
        let readers = users.iter().map(|user| user.account).filter(|id| !self.unreadable.contains(&id.0)).collect();
        self.call(format!("readers:{}", users.len()), readers)
    }
    fn render(
        &self,
        (): &(),
        user: &MailRecipient<()>,
        _: &MailActivity,
        reason: Reason,
    ) -> impl Future<Output = Result<i64, &'static str>> + Send {
        self.trace.lock().unwrap().push(format!("render:{}:{}", user.account.0, reason.as_str()));
        ready(if self.fail_render { Err("render") } else { Ok(user.account.0) })
    }
    fn claim(
        &self,
        accounts: &[AccountId],
        issue: IssueId,
        key: &str,
        now: Timestamp,
    ) -> impl Future<Output = Result<HashSet<AccountId>, &'static str>> + Send {
        let ids: Vec<_> = accounts.iter().map(|id| id.0).collect();
        let won = accounts.iter().copied().filter(|id| !self.already_claimed.contains(&id.0)).collect();
        self.call(format!("claim:{ids:?}:{}:{key}:{}", issue.0, now.0), won)
    }
    fn queue_digest(&self, items: &[NewDigestItem]) -> impl Future<Output = Result<(), &'static str>> + Send {
        let rendered: Vec<_> = items.iter().map(|item| format!("{}:{}:{}", item.account.0, item.reason.as_str(), item.excerpt)).collect();
        self.call(format!("queue:{rendered:?}"), ())
    }
    fn send(&self, user: &MailRecipient<()>, mail: i64) {
        self.trace.lock().unwrap().push(format!("send:{}:{mail}", user.account.0));
    }
}

#[tokio::test]
async fn only_wanted_readable_unclaimed_recipients_are_mailed_after_the_batch_renders() {
    let world = World {
        users: vec![
            (2, None, Cadence::Immediate),
            (3, Some(MailPreference::Off), Cadence::Immediate),
            (4, None, Cadence::Daily),
            (5, None, Cadence::Immediate),
            (6, None, Cadence::Immediate),
            (7, None, Cadence::Immediate),
        ],
        unreadable: vec![5],
        already_claimed: vec![6],
        ..World::default()
    };
    // 1 is the actor; 7 only subscribed by hand; 8 no longer qualifies (suspended or unverified).
    let recipients = [(1, Reason::Author), (2, Reason::Comment), (3, Reason::Mention), (4, Reason::Mention), (5, Reason::Comment)]
        .into_iter()
        .chain([(6, Reason::Comment), (7, Reason::Manual), (8, Reason::Comment), (2, Reason::Mention)])
        .map(|(id, reason)| (AccountId(id), reason))
        .collect::<Vec<_>>();
    deliver(&world, scope(), &activity(), &recipients).await.unwrap();
    assert_eq!(
        world.trace(),
        [
            "repository:3",
            "recipients:7",
            "readers:4",
            "render:2:mention",
            "render:6:comment",
            "claim:[2, 6, 4]:4:comment-9:50",
            "queue:[\"4:mention:Same here\"]",
            "send:2:2",
        ]
    );
}

#[tokio::test]
async fn a_failed_render_claims_nothing_and_a_vanished_repository_stops_delivery() {
    let world = World { users: vec![(2, None, Cadence::Immediate)], fail_render: true, ..World::default() };
    assert_eq!(deliver(&world, scope(), &activity(), &[(AccountId(2), Reason::Comment)]).await, Err("render"));
    assert!(!world.trace().iter().any(|entry| entry.starts_with("claim")));

    let recipients: Vec<_> =
        (10..10 + i64::try_from(NOTIFICATION_BATCH_SIZE + 3).unwrap()).map(|id| (AccountId(id), Reason::Comment)).collect();
    let world = World { repo_gone_after: Some(1), ..World::default() };
    deliver(&world, scope(), &activity(), &recipients).await.unwrap();
    assert_eq!(world.trace().last().unwrap(), "repository:3", "the second batch finds no repository");
    assert_eq!(world.trace().iter().filter(|entry| entry.starts_with("recipients:100")).count(), 1);
}

fn item(repo: i64, issue: i64, reason: Reason) -> DigestItem {
    DigestItem {
        repo: RepoId(repo),
        issue: IssueId(issue),
        reason,
        headline: format!("on {issue}"),
        excerpt: String::new(),
        anchor: (issue == 2).then(|| "issuecomment-5".into()),
        created_at: Timestamp(1),
        number: IssueNumber::new(u32::try_from(issue).unwrap()).unwrap(),
        kind: IssueKind::Issue,
        title: "t".into(),
        owner: "o".into(),
        repo_name: format!("r{repo}"),
    }
}

impl DigestOutbox for World {
    type Error = &'static str;
    type User = ();
    type Mail = Vec<DigestSection>;
    fn candidates(&self, after: AccountId, limit: u32) -> impl Future<Output = Result<Vec<PendingDigest>, &'static str>> + Send {
        let page = if after.0 == 0 {
            self.users.iter().map(|&(id, _, cadence)| PendingDigest { account: AccountId(id), cadence, digest_hour: Some(8) }).collect()
        } else {
            Vec::new()
        };
        self.call(format!("candidates:{}:{limit}", after.0), page)
    }
    fn claim(&self, account: AccountId, period: Timestamp, now: Timestamp) -> impl Future<Output = Result<Claim, &'static str>> + Send {
        let digest = ClaimedDigest { id: account.0 * 10, account, period, items: 1 };
        self.call(format!("claim:{}:{}:{}", account.0, period.0, now.0), Claim::Claimed { digest, skipped_read: 1 })
    }
    fn finish(&self, digest: i64, state: DigestState, now: Timestamp) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.call(format!("finish:{digest}:{}:{}", state.as_str(), now.0), ())
    }
    fn recipient(&self, account: AccountId) -> impl Future<Output = Result<Option<MailRecipient<()>>, &'static str>> + Send {
        self.call(format!("recipient:{}", account.0), self.recipient(account.0))
    }
    fn items(&self, digest: i64) -> impl Future<Output = Result<Vec<DigestItem>, &'static str>> + Send {
        self.call(format!("items:{digest}"), self.items.clone())
    }
    fn can_read(&self, _: &MailRecipient<()>, repo: RepoId) -> impl Future<Output = Result<bool, &'static str>> + Send {
        self.call(format!("can-read:{}", repo.0), !self.unreadable.contains(&repo.0))
    }
    fn render(
        &self,
        user: &MailRecipient<()>,
        _: &ClaimedDigest,
        weekly: bool,
        sections: Vec<DigestSection>,
    ) -> impl Future<Output = Result<Vec<DigestSection>, &'static str>> + Send {
        self.trace.lock().unwrap().push(format!("render:{}:{weekly}", user.account.0));
        ready(if self.fail_render { Err("render") } else { Ok(sections) })
    }
    fn deliver(&self, user: &MailRecipient<()>, mail: Vec<DigestSection>, notifications: usize) -> impl Future<Output = bool> + Send {
        let entries: usize = mail.iter().flat_map(|section| &section.threads).map(|thread| thread.entries.len()).sum();
        self.trace.lock().unwrap().push(format!("deliver:{}:{}:{entries}:{notifications}", user.account.0, mail.len()));
        ready(!self.refuse)
    }
    fn build_failed(&self, account: AccountId, error: &&'static str) {
        self.trace.lock().unwrap().push(format!("build-failed:{}:{error}", account.0));
    }
    fn record(&self, sent: bool) {
        self.trace.lock().unwrap().push(format!("record:{sent}"));
    }
}

const DEFAULTS: DigestDefaults<'static> = DigestDefaults { preference: MailPreference::Participating, hour: 3, base_url: "https://x" };

#[tokio::test]
async fn digests_recheck_preference_and_access_then_group_and_count() {
    let world = World {
        users: vec![(2, None, Cadence::Weekly)],
        unreadable: vec![9],
        items: vec![item(1, 2, Reason::Comment), item(1, 2, Reason::Mention), item(1, 3, Reason::Manual), item(9, 4, Reason::Mention)],
        ..World::default()
    };
    let ticks = Mutex::new(100);
    let clock = || {
        let mut tick = ticks.lock().unwrap();
        *tick += 1;
        Timestamp(*tick)
    };
    let run = send_digests(&world, DEFAULTS, clock).await.unwrap();
    assert_eq!(run, DigestRun { sent: 1, skipped_read: 1, ..DigestRun::default() });
    assert_eq!(
        world.trace(),
        [
            "candidates:0:200",
            "claim:2:-230400:101",
            "recipient:2",
            "items:20",
            "can-read:1",
            "can-read:9",
            "render:2:true",
            "deliver:2:1:2:2",
            "record:true",
            "finish:20:sent:102",
        ]
    );
}

#[tokio::test]
async fn refused_unbuildable_and_empty_digests_are_finished_once() {
    for (world, state, recorded) in [
        (
            World { users: vec![(2, None, Cadence::Daily)], items: vec![item(1, 2, Reason::Comment)], refuse: true, ..World::default() },
            "failed",
            true,
        ),
        (
            World {
                users: vec![(2, None, Cadence::Daily)],
                items: vec![item(1, 2, Reason::Comment)],
                fail_render: true,
                ..World::default()
            },
            "failed",
            true,
        ),
        (
            World {
                users: vec![(2, Some(MailPreference::Off), Cadence::Daily)],
                items: vec![item(1, 2, Reason::Comment)],
                ..World::default()
            },
            "empty",
            false,
        ),
        (World { users: vec![(2, None, Cadence::Daily)], items: vec![item(1, 2, Reason::Manual)], ..World::default() }, "empty", false),
    ] {
        send_digests(&world, DEFAULTS, || Timestamp(100)).await.unwrap();
        let trace = world.trace();
        assert_eq!(trace.iter().filter(|entry| entry.starts_with("finish")).collect::<Vec<_>>(), [&format!("finish:20:{state}:100")]);
        assert_eq!(trace.iter().any(|entry| entry.starts_with("record")), recorded, "{trace:?}");
        assert_eq!(trace.iter().any(|entry| entry.starts_with("build-failed")), world.fail_render);
    }
}
