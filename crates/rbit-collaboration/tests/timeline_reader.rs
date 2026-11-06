#![cfg(feature = "applications")]
//! Selected-page batching, privacy filtering and cursor continuation through a static port.
use rbit_collaboration::{
    IssueState,
    comments::{Comment, CommentId, CommentSource},
    threads::{IssueId, IssueKind, IssueNumber, IssueText},
    timeline::{
        CommentAccess, EventId, IssueRef, IssueSource, StoredTimelineItem, TimelineCursor, TimelineEntry, TimelineItemId, TimelinePage,
        TimelineReadPort, TimelineReader,
    },
};
use rbit_contracts::{AccountId, Timestamp};
use rbit_identity_models::{Account, AccountKind};
use std::sync::Mutex;
#[derive(Debug, Clone)]
enum Event {
    Subject(AccountId),
    Cross(IssueId),
}
fn account(id: i64) -> Account {
    Account {
        id: AccountId(id),
        kind: AccountKind::User,
        name: format!("u{id}").parse().unwrap(),
        display_name: None,
        bio: None,
        created_at: Timestamp(1),
    }
}
fn comment(author: Option<AccountId>) -> Comment {
    Comment {
        id: CommentId(1),
        issue_id: IssueId(1),
        author,
        body: IssueText::comment("body").unwrap(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        edited_at: None,
        source: CommentSource::User,
    }
}
fn event(id: i64, event: Event) -> StoredTimelineItem<Event> {
    StoredTimelineItem::Event { id: EventId(id), actor: None, actor_id: None, event, at: Timestamp(id) }
}
struct Port {
    page: Mutex<Option<TimelinePage<Event>>>,
    calls: Mutex<Vec<&'static str>>,
    ids: Mutex<(Vec<AccountId>, Vec<IssueId>)>,
    limits: Mutex<Vec<u32>>,
    comment: Option<Comment>,
    can_edit: bool,
}
impl Port {
    fn new(items: Vec<StoredTimelineItem<Event>>) -> Self {
        Self {
            page: Mutex::new(Some(TimelinePage {
                items,
                next: Some(TimelineCursor { at: Timestamp(8), item: TimelineItemId::Event(EventId(8)) }),
            })),
            calls: Mutex::new(Vec::new()),
            ids: Mutex::new((Vec::new(), Vec::new())),
            limits: Mutex::new(Vec::new()),
            comment: None,
            can_edit: false,
        }
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl TimelineReadPort for Port {
    type Error = &'static str;
    type Event = Event;
    type Permissions = bool;
    async fn page(&self, _: IssueId, _: Option<TimelineCursor>, limit: u32) -> Result<TimelinePage<Event>, Self::Error> {
        self.calls.lock().unwrap().push("page");
        self.limits.lock().unwrap().push(limit);
        Ok(self.page.lock().unwrap().take().unwrap())
    }
    async fn subjects(&self, accounts: &[AccountId], sources: &[IssueId]) -> Result<(Vec<Account>, Vec<IssueSource>), Self::Error> {
        self.calls.lock().unwrap().push("subjects");
        *self.ids.lock().unwrap() = (accounts.to_vec(), sources.to_vec());
        Ok((
            vec![account(3)],
            vec![IssueSource {
                issue: IssueRef {
                    id: IssueId(7),
                    number: IssueNumber::new(2).unwrap(),
                    kind: IssueKind::Issue,
                    state: IssueState::Open,
                    title: "Readable".into(),
                },
                full_name: "u3/repo".into(),
            }],
        ))
    }
    async fn comment(&self, _: IssueId, _: CommentId) -> Result<Option<Comment>, Self::Error> {
        self.calls.lock().unwrap().push("comment");
        Ok(self.comment.clone())
    }
    async fn accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, Self::Error> {
        assert_eq!(ids, &[AccountId(3)]);
        self.calls.lock().unwrap().push("author");
        Ok(vec![account(3)])
    }
    fn subject_account(event: &Event) -> Option<AccountId> {
        if let Event::Subject(id) = event { Some(*id) } else { None }
    }
    fn cross_reference(event: &Event) -> Option<IssueId> {
        if let Event::Cross(id) = event { Some(*id) } else { None }
    }
    fn permissions(&self) -> bool {
        self.calls.lock().unwrap().push("permissions");
        self.can_edit
    }
    fn comment_access(&self, permission: bool, _: &Comment) -> CommentAccess {
        self.calls.lock().unwrap().push("comment_access");
        CommentAccess { can_edit: permission, can_delete: permission }
    }
}
#[tokio::test]
async fn selected_page_deduplicates_batches_and_filters_unreadable_sources_without_rewriting_next() {
    let port = Port::new(vec![
        event(1, Event::Subject(AccountId(3))),
        event(2, Event::Subject(AccountId(3))),
        event(3, Event::Cross(IssueId(7))),
        event(4, Event::Cross(IssueId(9))),
        StoredTimelineItem::Comment { comment: comment(None), author: None },
    ]);
    let page = TimelineReader::new(&port).read(IssueId(1), None, 999).await.unwrap();
    assert_eq!(page.entries.len(), 4);
    assert_eq!(page.next.unwrap().at, Timestamp(8));
    let mut ids = port.ids.lock().unwrap();
    ids.0.sort();
    ids.1.sort();
    assert_eq!(ids.0, [AccountId(3)]);
    assert_eq!(ids.1, [IssueId(7), IssueId(9)]);
    assert_eq!(*port.limits.lock().unwrap(), [200]);
    assert_eq!(*port.calls.lock().unwrap(), ["page", "subjects", "permissions", "comment_access"]);
    assert!(matches!(&page.entries[2],TimelineEntry::Event {subjects,..} if subjects.source.as_ref().unwrap().title=="Readable"));
    assert!(matches!(&page.entries[3], TimelineEntry::Comment { can_edit: false, can_delete: false, .. }));
}
#[tokio::test]
async fn comment_lookup_reads_author_only_when_present_and_missing_short_circuits() {
    for author in [None, Some(AccountId(3))] {
        let mut port = Port::new(Vec::new());
        port.comment = Some(comment(author));
        let result = TimelineReader::new(&port).find_comment(IssueId(1), CommentId(1)).await.unwrap().unwrap();
        assert_eq!(result.1.is_some(), author.is_some());
        assert_eq!(*port.calls.lock().unwrap(), if author.is_some() { vec!["comment", "author"] } else { vec!["comment"] });
    }
    let port = Port::new(Vec::new());
    assert!(TimelineReader::new(&port).find_comment(IssueId(1), CommentId(1)).await.unwrap().is_none());
    assert_eq!(*port.calls.lock().unwrap(), ["comment"]);
}
#[tokio::test]
async fn minimum_page_and_unpolled_requests_keep_bounds_and_no_eager_work() {
    let port = Port::new(Vec::new());
    let reader = TimelineReader::new(&port);
    drop(reader.read(IssueId(1), None, 0));
    assert_eq!(*port.calls.lock().unwrap(), Vec::<&str>::new());
    reader.read(IssueId(1), None, 0).await.unwrap();
    assert_eq!(*port.limits.lock().unwrap(), [1]);
}
