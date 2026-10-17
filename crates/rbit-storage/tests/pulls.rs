//! Pull requests, reviews and fork networks against a real, migrated SQLite database.

use rbit_core::{
    IssueKind, IssueState, IssueText, IssueTitle, RepoId, ReviewId, StateReason, TimelineEvent, Timestamp, User, Visibility,
    issue::{MergeStrategy, ReviewState},
    pull::{Mergeability, ReviewSide},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        issues::NewIssue,
        pulls::{CreatedPull, MergeRecord, NewPull, PullFilter, PullSort, PullStateFilter},
        reviews::{NewReview, NewReviewComment},
    },
};

async fn user(database: &Database, name: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}

async fn repo(database: &Database, owner: &User, name: &str, visibility: Visibility) -> RepoId {
    database
        .repositories()
        .create(NewRepository {
            owner_id: owner.id(),
            name: name.parse().unwrap(),
            description: None,
            visibility,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id
}

fn new_pull(repo: RepoId, head_repo: RepoId, author: &User, head: &str, at: i64) -> NewPull {
    NewPull {
        issue: NewIssue {
            repo,
            kind: IssueKind::Pull,
            title: IssueTitle::parse(&format!("Merge {head}")).unwrap(),
            body: IssueText::body("").unwrap(),
            author: author.id(),
            at: Timestamp(at),
        },
        head_repo,
        head_branch: head.to_owned(),
        base_branch: "main".to_owned(),
        head_sha: "a".repeat(40),
        is_draft: false,
    }
}

async fn created(database: &Database, new: &NewPull) -> (rbit_core::Issue, rbit_core::pull::PullRequest) {
    match database.pulls().create(new).await.unwrap() {
        CreatedPull::Created(pair) => *pair,
        CreatedPull::Duplicate(number) => panic!("duplicate of {number}"),
    }
}

#[tokio::test]
async fn pull_requests_share_numbers_and_refuse_duplicates_while_open() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let alice = user(&database, "alice").await;
    let demo = repo(&database, &alice, "demo", Visibility::Public).await;
    let issue = database
        .issues()
        .create(&NewIssue {
            repo: demo,
            kind: IssueKind::Issue,
            title: IssueTitle::parse("Bug").unwrap(),
            body: IssueText::body("").unwrap(),
            author: alice.id(),
            at: Timestamp(1),
        })
        .await
        .unwrap();
    assert_eq!(issue.number.get(), 1);

    let (thread, pull) = created(&database, &new_pull(demo, demo, &alice, "feature", 10)).await;
    assert_eq!((thread.number.get(), thread.kind), (2, IssueKind::Pull));
    assert_eq!((pull.head_branch.as_str(), pull.mergeable, pull.is_cross_repository()), ("feature", Mergeability::Unknown, false));
    assert_eq!(database.pulls().find(thread.id).await.unwrap().unwrap(), pull);
    assert!(database.pulls().find(issue.id).await.unwrap().is_none());

    // A second open pull request for the same branches is refused and uses up no number.
    let CreatedPull::Duplicate(existing) = database.pulls().create(&new_pull(demo, demo, &alice, "feature", 11)).await.unwrap() else {
        panic!("expected a duplicate");
    };
    assert_eq!(existing, thread.number);
    let (other, _) = created(&database, &new_pull(demo, demo, &alice, "other", 12)).await;
    assert_eq!(other.number.get(), 3, "the refused creation rolled back its number");

    // Once the first is closed, the same branches may be proposed again.
    let closed = TimelineEvent::Closed { reason: StateReason::Completed, commit: None, pull: None };
    database
        .issues()
        .set_state(thread.id, IssueState::Closed, Some(StateReason::Completed), Some(alice.id()), &closed, Timestamp(20))
        .await
        .unwrap();
    let (again, again_pull) = created(&database, &new_pull(demo, demo, &alice, "feature", 21)).await;
    assert_eq!(again.number.get(), 4);
    // Reopening the closed one would now duplicate the new one.
    assert_eq!(database.pulls().open_duplicate(&pull).await.unwrap(), Some(again.number));
    assert_eq!(database.pulls().open_duplicate(&again_pull).await.unwrap(), None);
    database.close().await;
}

#[tokio::test]
async fn heads_mergeability_drafts_and_merges() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let alice = user(&database, "alice").await;
    let bob = user(&database, "bob").await;
    let demo = repo(&database, &alice, "demo", Visibility::Public).await;
    let fork = repo(&database, &bob, "demo", Visibility::Public).await;
    let store = database.pulls();
    let (one, _) = created(&database, &new_pull(demo, fork, &bob, "feature", 10)).await;
    let (two, _) = created(&database, &new_pull(demo, demo, &alice, "fix", 11)).await;

    assert_eq!(store.open_with_head(fork, "feature").await.unwrap().len(), 1);
    assert_eq!(store.open_with_head(demo, "feature").await.unwrap().len(), 0);
    assert_eq!(store.open_with_base(demo, "main").await.unwrap().len(), 2);
    assert_eq!(store.unknown_mergeability(10).await.unwrap().len(), 2);

    // A result for an old head is not stored.
    assert!(!store.set_mergeability(one.id, Mergeability::Clean, &"b".repeat(40), &"c".repeat(40), &[]).await.unwrap());
    let paths = vec!["a.txt".to_owned(), "b.txt".to_owned()];
    assert!(store.set_mergeability(one.id, Mergeability::Conflict, &"b".repeat(40), &"a".repeat(40), &paths).await.unwrap());
    let stored = store.find(one.id).await.unwrap().unwrap();
    assert_eq!((stored.mergeable, stored.conflict_paths.clone()), (Mergeability::Conflict, paths));
    assert_eq!(store.unknown_mergeability(10).await.unwrap().len(), 1);

    // A new head forgets the result.
    assert!(store.set_head(one.id, &"d".repeat(40), Timestamp(30)).await.unwrap());
    assert!(!store.set_head(one.id, &"d".repeat(40), Timestamp(31)).await.unwrap());
    let moved = store.find(one.id).await.unwrap().unwrap();
    assert_eq!((moved.mergeable, moved.head_sha.as_str(), moved.conflict_paths.len()), (Mergeability::Unknown, "d".repeat(40).as_str(), 0));

    // Drafts are not checked.
    assert!(store.set_draft(two.id, true, Timestamp(40)).await.unwrap());
    assert!(!store.set_draft(two.id, true, Timestamp(41)).await.unwrap());
    assert_eq!(store.unknown_mergeability(10).await.unwrap().len(), 1);
    assert!(store.set_base(two.id, "release", Timestamp(42)).await.unwrap());
    assert_eq!(store.find(two.id).await.unwrap().unwrap().base_branch, "release");

    let record = MergeRecord {
        merged_by: Some(alice.id()),
        strategy: Some(MergeStrategy::Squash),
        commit: "e",
        base: "f",
        head: "g",
        at: Timestamp(50),
    };
    assert!(store.record_merge(one.id, &record).await.unwrap());
    assert!(!store.record_merge(one.id, &record).await.unwrap(), "merged once");
    let merged = store.find(one.id).await.unwrap().unwrap();
    assert!(merged.is_merged());
    assert_eq!(
        (merged.merge_strategy, merged.merge_commit_sha.as_deref(), merged.merged_by),
        (Some(MergeStrategy::Squash), Some("e"), Some(alice.id()))
    );
    assert!(!store.set_draft(one.id, true, Timestamp(51)).await.unwrap(), "a merged pull request stays as it is");

    // Lists and counts.
    let open = PullFilter::default();
    assert_eq!(store.count(demo, &open).await.unwrap(), 2);
    let merged_filter = PullFilter { state: PullStateFilter::Merged, ..PullFilter::default() };
    assert_eq!(store.count(demo, &merged_filter).await.unwrap(), 1);
    let by_bob = PullFilter { state: PullStateFilter::All, author: Some(bob.id()), ..PullFilter::default() };
    let listed = store.list(demo, &by_bob, 10, 0).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].0.number, listed[0].1.head_repo_id), (one.number, Some(fork)));
    let ascending = PullFilter { state: PullStateFilter::All, sort: PullSort::Created, ascending: true, ..PullFilter::default() };
    let numbers: Vec<u32> = store.list(demo, &ascending, 10, 0).await.unwrap().iter().map(|(i, _)| i.number.get()).collect();
    assert_eq!(numbers, [1, 2]);
    assert_eq!(store.list(demo, &ascending, 1, 1).await.unwrap()[0].0.number.get(), 2);
    let head_filter =
        PullFilter { state: PullStateFilter::All, head_repo: Some(fork), head_branch: Some("feature".to_owned()), ..PullFilter::default() };
    assert_eq!(store.count(demo, &head_filter).await.unwrap(), 1);

    // Deleting the fork keeps the pull request, without a head repository.
    database.repositories().delete(fork).await.unwrap();
    assert_eq!(store.find(one.id).await.unwrap().unwrap().head_repo_id, None);
    database.close().await;
}

#[tokio::test]
async fn reviews_pending_submitted_dismissed_and_requests() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let alice = user(&database, "alice").await;
    let bob = user(&database, "bob").await;
    let demo = repo(&database, &alice, "demo", Visibility::Public).await;
    let (pull, _) = created(&database, &new_pull(demo, demo, &alice, "feature", 10)).await;
    let store = database.reviews();
    let head = "a".repeat(40);

    assert!(store.request(pull.id, bob.id(), alice.id(), Timestamp(11)).await.unwrap());
    assert!(!store.request(pull.id, bob.id(), alice.id(), Timestamp(12)).await.unwrap());
    assert_eq!(store.requested(pull.id).await.unwrap(), [bob.id()]);

    let comment = |line: u32| NewReviewComment {
        path: "src/lib.rs".to_owned(),
        side: ReviewSide::Right,
        line,
        commit_sha: head.clone(),
        diff_hunk: "@@ -1 +1 @@\n+x".to_owned(),
        in_reply_to: None,
        body: format!("Line {line}"),
    };
    let (pending, first) = store.add_pending_comment(pull.id, bob.id(), &head, &comment(1), Timestamp(13)).await.unwrap();
    let (same, second) = store.add_pending_comment(pull.id, bob.id(), &head, &comment(2), Timestamp(14)).await.unwrap();
    assert_eq!((pending.state, pending.id), (ReviewState::Pending, same.id), "one pending review per user");
    assert_eq!((first.review_id, second.line), (pending.id, 2));
    assert_eq!(store.submitted_comment_count(pull.id).await.unwrap(), 0, "pending comments are not counted");

    let submitted =
        store.submit_pending(pull.id, bob.id(), ReviewState::ChangesRequested, "Please fix", &head, Timestamp(15)).await.unwrap().unwrap();
    assert_eq!((submitted.state, submitted.submitted_at), (ReviewState::ChangesRequested, Some(Timestamp(15))));
    assert!(store.requested(pull.id).await.unwrap().is_empty(), "submitting clears the request");
    assert!(store.submit_pending(pull.id, bob.id(), ReviewState::Approved, "", &head, Timestamp(16)).await.unwrap().is_none());
    assert_eq!(store.submitted_comment_count(pull.id).await.unwrap(), 2);

    let new =
        NewReview { pull: pull.id, reviewer: bob.id(), state: ReviewState::Commented, body: "", commit_sha: &head, at: Timestamp(17) };
    let (single, comments) = store.create_submitted(&new, &[NewReviewComment { in_reply_to: Some(first.id), ..comment(1) }]).await.unwrap();
    assert_eq!((single.state, comments[0].in_reply_to), (ReviewState::Commented, Some(first.id)));

    assert!(store.dismiss(pull.id, single.id, alice.id(), "n/a", Timestamp(18)).await.unwrap().is_none(), "comments cannot be dismissed");
    let dismissed = store.dismiss(pull.id, submitted.id, alice.id(), "Fixed", Timestamp(19)).await.unwrap().unwrap();
    assert_eq!(
        (dismissed.state, dismissed.dismissed_by, dismissed.dismissal_message.as_deref()),
        (ReviewState::Dismissed, Some(alice.id()), Some("Fixed"))
    );
    assert_eq!(store.edit_body(pull.id, single.id, "Edited").await.unwrap().unwrap().body, "Edited");
    assert!(store.find(pull.id, ReviewId(999)).await.unwrap().is_none());

    store.mark_outdated(&[first.id]).await.unwrap();
    let all = store.comments(pull.id).await.unwrap();
    assert_eq!(all.iter().filter(|c| c.outdated).count(), 1);
    assert_eq!(store.find_comment(pull.id, second.id).await.unwrap().unwrap().body, "Line 2");

    let (_, _) = store.add_pending_comment(pull.id, alice.id(), &head, &comment(3), Timestamp(20)).await.unwrap();
    assert!(store.discard_pending(pull.id, alice.id()).await.unwrap());
    assert!(store.pending(pull.id, alice.id()).await.unwrap().is_none());
    let reviews = store.reviews(pull.id).await.unwrap();
    assert_eq!(reviews.len(), 2);
    let summaries = store.submitted_among(&[pull.id]).await.unwrap();
    assert_eq!(summaries[&pull.id].len(), 2);

    database.close().await;
}

#[tokio::test]
async fn fork_networks_reach_the_root_and_respect_visibility() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let alice = user(&database, "alice").await;
    let bob = user(&database, "bob").await;
    let carol = user(&database, "carol").await;
    let root = repo(&database, &alice, "demo", Visibility::Public).await;
    let child = repo(&database, &bob, "demo", Visibility::Public).await;
    let grandchild = repo(&database, &carol, "demo", Visibility::Private).await;
    let sibling = repo(&database, &carol, "other", Visibility::Public).await;
    let unrelated = repo(&database, &bob, "unrelated", Visibility::Public).await;
    let forks = database.forks();
    forks.record(child, root).await.unwrap();
    forks.record(grandchild, child).await.unwrap();
    forks.record(sibling, root).await.unwrap();

    let names = |repos: Vec<rbit_core::OwnedRepository>| repos.iter().map(rbit_core::OwnedRepository::full_name).collect::<Vec<_>>();
    let from_child = names(forks.network_readable(child, Some(bob.id()), false).await.unwrap());
    assert_eq!(from_child, ["alice/demo", "bob/demo", "carol/other"], "the private grandchild is hidden from bob");
    let from_grandchild = names(forks.network_readable(grandchild, Some(carol.id()), false).await.unwrap());
    assert_eq!(from_grandchild, ["alice/demo", "bob/demo", "carol/demo", "carol/other"]);
    assert_eq!(names(forks.network_readable(unrelated, None, false).await.unwrap()), ["bob/unrelated"]);
    database.close().await;
}
