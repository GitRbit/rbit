//! Privileged close callers preserve atomic event provenance and no-op behavior.
use super::super::{IssueAction, IssueId, IssueKind, StateReason, TimelineEntry, TimelineEvent, tests::fixture};
use crate::DomainEvent;

#[tokio::test]
async fn system_closes_record_their_event_once() {
    let (_dir, services, alice, repo) = fixture().await;
    let timeline = &services.timeline;
    let issue = timeline.create(&alice, &repo, IssueKind::Issue, "Crash", "").await.unwrap();
    let pull = timeline.create(&alice, &repo, IssueKind::Pull, "Fix", "").await.unwrap();
    let mut events = services.events.subscribe();

    let by_commit = TimelineEvent::Closed { reason: StateReason::Completed, commit: Some("abc1234".to_owned()), pull: None };
    assert!(timeline.close_as_system(issue.id, None, StateReason::Completed, by_commit.clone()).await.unwrap());
    assert!(!timeline.close_as_system(issue.id, None, StateReason::Completed, by_commit.clone()).await.unwrap(), "already closed");
    let merged = TimelineEvent::Merged { commit: "def5678".to_owned(), strategy: Some(rbit_core::issue::MergeStrategy::Squash) };
    assert!(timeline.close_as_system(pull.id, Some(alice.id()), StateReason::Merged, merged).await.unwrap());
    assert!(!timeline.close_as_system(IssueId(999), None, StateReason::Completed, by_commit.clone()).await.unwrap());
    let event_id = timeline.record(issue.id, None, TimelineEvent::Locked).await.unwrap();
    assert!(event_id.0 > 0);

    let published: Vec<_> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert_eq!(published.len(), 2);
    assert!(matches!(
        &published[0],
        DomainEvent::Issue { actor: None, kind: IssueKind::Issue, action: IssueAction::Closed { reason: StateReason::Completed }, .. }
    ));
    assert!(matches!(&published[1], DomainEvent::Issue { action: IssueAction::Closed { reason: StateReason::Merged }, .. }));

    let ctx = timeline.open(Some(&alice), repo.clone(), issue.number).await.unwrap();
    assert_eq!((ctx.issue.state_reason, ctx.issue.closed_by), (Some(StateReason::Completed), None));
    let entries = timeline.timeline(Some(&alice), &ctx, None, 10).await.unwrap().entries;
    let recorded: Vec<_> = entries
        .iter()
        .filter_map(|entry| if let TimelineEntry::Event { event, .. } = entry { Some(event.clone()) } else { None })
        .collect();
    assert_eq!(recorded, [by_commit, TimelineEvent::Locked]);
    let merged_pull = timeline.open(Some(&alice), repo, pull.number).await.unwrap();
    assert_eq!(merged_pull.issue.state_reason, Some(StateReason::Merged));
}

#[tokio::test]
async fn system_winner_noop_missing_and_concurrent_closures_keep_work_and_publication() {
    use rbit_storage::count_work;
    let (_directory, services, alice, repo) = fixture().await;
    let timeline = &services.timeline;
    let issue = timeline.create(&alice, &repo, IssueKind::Issue, "Closure work", "").await.unwrap();
    let event = TimelineEvent::Closed { reason: StateReason::Completed, commit: Some("abc1234".into()), pull: None };
    let mut events = services.events.subscribe();
    let (changed, work) = count_work(timeline.close_as_system(issue.id, None, StateReason::Completed, event.clone())).await;
    assert!(changed.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 3));
    let (same, work) = count_work(timeline.close_as_system(issue.id, None, StateReason::Completed, event.clone())).await;
    assert!(!same.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    let (missing, work) = count_work(timeline.close_as_system(IssueId(i64::MAX), None, StateReason::Completed, event.clone())).await;
    assert!(!missing.unwrap());
    assert_eq!((work.acquisitions, work.statements), (1, 1));
    assert_eq!(std::iter::from_fn(|| events.try_recv().ok()).count(), 1);
    let issue = timeline.create(&alice, &repo, IssueKind::Issue, "Race", "").await.unwrap();
    let (first, second) = tokio::join!(
        timeline.close_as_system(issue.id, None, StateReason::Completed, event.clone()),
        timeline.close_as_system(issue.id, None, StateReason::Completed, event)
    );
    assert_eq!(usize::from(first.unwrap()) + usize::from(second.unwrap()), 1);
    assert_eq!(std::iter::from_fn(|| events.try_recv().ok()).count(), 1);
}
