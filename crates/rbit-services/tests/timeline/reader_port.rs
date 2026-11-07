//! Selected-page SQL batches, private references and explicit reader composition.
use super::Fixture;
use rbit_core::{IssueKind, IssueText, IssueTitle, TimelineEvent, Timestamp, Visibility};
use rbit_services::{
    CreateRepository,
    timeline::{TimelineCursor, TimelineEntry},
};
use rbit_storage::{count_work, sqlite::issues::NewIssue};
#[test]
fn cursor_event_and_page_paths_are_exact_owner_aliases() {
    use rbit_collaboration::timeline as owner;
    use std::any::TypeId;
    assert_eq!(TypeId::of::<rbit_services::TimelineService>(), TypeId::of::<rbit_services::ThreadServices>());
    assert_eq!(TypeId::of::<TimelineCursor>(), TypeId::of::<owner::TimelineCursor>());
    assert_eq!(TypeId::of::<rbit_core::EventId>(), TypeId::of::<owner::EventId>());
    assert_eq!(TypeId::of::<TimelineEntry>(), TypeId::of::<owner::TimelineEntry<TimelineEvent>>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issues::StoredTimelineItem>(), TypeId::of::<owner::StoredTimelineItem<TimelineEvent>>());
    assert_eq!(TypeId::of::<rbit_storage::sqlite::issues::TimelinePage>(), TypeId::of::<owner::TimelinePage<TimelineEvent>>());
}
#[tokio::test]
async fn selected_page_batches_subjects_and_filters_private_crossrefs_while_preserving_next() {
    let fixture = Fixture::new(|_| {}).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Public target").await;
    let ctx = fixture.open(&fixture.bob, number).await;
    fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            CreateRepository { name: "private-source".into(), description: String::new(), visibility: Visibility::Private },
        )
        .await
        .unwrap();
    let private = fixture.services.repositories.open(Some(&fixture.alice), "alice", "private-source").await.unwrap();
    let source = fixture
        .database
        .issues()
        .create(&NewIssue {
            repo: private.repo.repo.id,
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Private source").unwrap(),
            body: IssueText::body("").unwrap(),
            author: fixture.alice.id(),
            at: Timestamp(1),
        })
        .await
        .unwrap();
    let store = fixture.database.issues();
    store.record_event(ctx.issue.id, None, &TimelineEvent::Assigned { assignee: fixture.bob.id() }, Timestamp(1)).await.unwrap();
    let hidden = store.record_event(ctx.issue.id, None, &TimelineEvent::CrossReferenced { source: source.id }, Timestamp(2)).await.unwrap();
    store.record_event(ctx.issue.id, None, &TimelineEvent::CrossReferenced { source: ctx.issue.id }, Timestamp(3)).await.unwrap();
    let timeline = &fixture.services.timeline;
    let (page, work) = count_work(timeline.reader().timeline(Some(&fixture.bob), &ctx, None, 2)).await;
    let page = page.unwrap();
    assert_eq!((work.acquisitions, work.statements), (3, 3));
    assert_eq!(page.entries.len(), 1);
    assert!(matches!(&page.entries[0],TimelineEntry::Event {subjects,..} if subjects.account.as_ref().unwrap().id==fixture.bob.id()));
    let next = page.next.unwrap();
    assert_eq!(next.at, Timestamp(2));
    assert_eq!(next.item, rbit_services::timeline::TimelineItemId::Event(hidden));
    let (continuation, work) = count_work(timeline.reader().timeline(Some(&fixture.bob), &ctx, Some(next), 2)).await;
    let continuation = continuation.unwrap();
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    assert_eq!(continuation.entries.len(), 1);
    assert!(
        matches!(&continuation.entries[0],TimelineEntry::Event {subjects,..} if subjects.source.as_ref().unwrap().full_name=="alice/demo")
    );
    let owner = fixture.open(&fixture.alice, number).await;
    let (all, work) = count_work(timeline.reader().timeline(Some(&fixture.alice), &owner, None, 200)).await;
    assert_eq!(all.unwrap().entries.len(), 3);
    assert_eq!((work.acquisitions, work.statements), (3, 3));
}
#[tokio::test]
async fn explicit_roles_keep_comment_author_reads_and_joined_comment_page_without_extra_query() {
    let fixture = Fixture::new(|config| config.security.rate_limit = false).await;
    let number = fixture.seed(&fixture.alice, IssueKind::Issue, "Comments").await;
    let ctx = fixture.open(&fixture.bob, number).await;
    let timeline = &fixture.services.timeline;
    let comment = timeline.writer().comment(&fixture.bob, &ctx, "comment").await.unwrap();
    let (found, work) = count_work(timeline.reader().find_comment(&ctx, comment.id)).await;
    assert_eq!(found.unwrap().1.unwrap().id, fixture.bob.id());
    assert_eq!((work.acquisitions, work.statements), (2, 2));
    let (page, work) = count_work(timeline.reader().timeline(Some(&fixture.bob), &ctx, None, 200)).await;
    assert_eq!(page.unwrap().entries.len(), 1);
    assert_eq!((work.acquisitions, work.statements), (1, 1));
}

#[test]
fn narrow_thread_composition_omits_context_only_handles() {
    let old = std::mem::size_of::<(rbit_services::ServiceContext, rbit_services::throttle::ThrottleService)>();
    let new = std::mem::size_of::<rbit_services::ThreadServices>();
    assert!(new < old);
    eprintln!("thread composition: legacy context+throttle={old} bytes, explicit owner={new} bytes");
}
