//! Queued merge authority is rechecked after credentials have stopped working.

use super::*;
use rbit_core::protection::RuleSettings;
use rbit_services::{protection::ProtectionInput, statuses::NewStatus};
use rbit_storage::sqlite::actions_runs::{NewEntry, QueueEntry};
use std::time::Duration;

async fn settled(fixture: &Fixture, issue: rbit_core::IssueId, ready: impl Fn(&QueueEntry) -> bool) -> QueueEntry {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let entry = fixture.database.actions_runs().queue_of_pull(issue).await.unwrap().unwrap();
            if ready(&entry) {
                break entry;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the queued entry settles")
}

async fn suspended_queuer_cannot_advance(build_first: bool) {
    let fixture = Fixture::new().await;
    let repo = fixture.repo(&fixture.alice, "alice").await;
    let before = repo.reader.commit_id("refs/heads/main".into()).await.unwrap();
    let rule = ProtectionInput {
        pattern: "main".into(),
        settings: RuleSettings { require_status_checks: true, ..RuleSettings::default() },
        contexts: vec!["ci/test".into()],
        ..ProtectionInput::default()
    };
    fixture.services.protection.create(&fixture.alice, &repo, &rule).await.unwrap();
    let number = fixture.create(&fixture.alice, "alice", "feature").await.unwrap();
    let pulls = &fixture.services.pulls;
    pulls.check_unknown().await.unwrap();
    let ctx = fixture.open_pull(&fixture.carol, number).await;
    let issue = ctx.issue().id;
    if build_first {
        pulls.spawn_background();
        pulls.enqueue(&fixture.carol, &ctx, MergeStrategy::Merge, Some(&ctx.pull.head_sha)).await.unwrap();
        let testing = settled(&fixture, issue, |entry| entry.state == "testing").await;
        let candidate = testing.candidate_sha.unwrap();
        fixture.database.admin().set_suspended(fixture.carol.id(), true).await.unwrap();
        let success = NewStatus { state: "success".into(), context: Some("ci/test".into()), ..NewStatus::default() };
        fixture.services.statuses.create(&fixture.alice, &repo, &candidate, &success).await.unwrap();
    } else {
        // A persisted waiting entry, as resumed after a server restart. Suspension
        // must reject it before any candidate or native merge work is created.
        let new = NewEntry {
            repo: repo.repo.repo.id,
            issue,
            number,
            base_branch: &ctx.pull.base_branch,
            head_sha: &ctx.pull.head_sha,
            strategy: "merge",
            queued_by: fixture.carol.id(),
        };
        fixture.database.actions_runs().queue_pull(&new, rbit_core::Timestamp::now()).await.unwrap().unwrap();
        fixture.database.admin().set_suspended(fixture.carol.id(), true).await.unwrap();
        pulls.spawn_background();
        let success = NewStatus { state: "success".into(), context: Some("ci/test".into()), ..NewStatus::default() };
        fixture.services.statuses.create(&fixture.alice, &repo, &ctx.pull.head_sha, &success).await.unwrap();
    }
    let ended = settled(&fixture, issue, |entry| entry.state != "waiting" && (!build_first || entry.state != "testing")).await;
    assert_eq!(ended.state, "failed", "a suspended queuer must never land: {ended:?}");
    assert!(ended.reason.as_deref().is_some_and(|reason| reason.contains("suspended")), "{ended:?}");
    if !build_first {
        assert!(ended.candidate_sha.is_none(), "suspension is checked before candidate construction");
        assert_eq!(ended.attempts, 0);
    }
    assert_eq!(repo.reader.commit_id("refs/heads/main".into()).await.unwrap(), before, "the base must not move");
    assert!(!fixture.open_pull(&fixture.alice, number).await.pull.is_merged());
    pulls.shutdown().await;
}

#[tokio::test]
async fn a_suspended_queuer_cannot_land_a_candidate_after_ci_passes() {
    Box::pin(suspended_queuer_cannot_advance(true)).await;
}

#[tokio::test]
async fn a_suspended_queuer_cannot_build_a_persisted_waiting_entry() {
    Box::pin(suspended_queuer_cannot_advance(false)).await;
}
