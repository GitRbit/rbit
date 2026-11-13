//! What the trigger decided for each pushed commit, against a real, migrated `SQLite`
//! database: every reason the trigger records is stored, and a later evaluation of the same
//! commit keeps the one that started more runs.

use rbit_core::{RepoId, Timestamp, Visibility};
use rbit_storage::{Database, NewRepository, NewUser, sqlite::actions_runs::CommitDecision};

#[tokio::test]
async fn every_trigger_reason_is_recorded_and_a_start_is_never_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let user = NewUser {
        name: "alice".parse().unwrap(),
        email: "alice@example.com".parse().unwrap(),
        display_name: None,
        password_hash: "not-a-real-hash".to_owned(),
        is_admin: false,
    };
    let alice = db.accounts().create_user(user).await.unwrap().id();
    let new = NewRepository {
        owner_id: alice,
        name: "rocket".parse().unwrap(),
        description: None,
        visibility: Visibility::Public,
        default_branch: "main".to_owned(),
    };
    let repo: RepoId = db.repositories().create(new).await.unwrap().id;
    let store = db.actions_runs();
    let decided = |started: i64, reason: &str| CommitDecision { workflows: 1, started, reason: reason.to_owned() };

    for (index, reason) in ["started", "skip_marker", "no_workflows", "filtered", "too_many_refs", "no_jobs"].into_iter().enumerate() {
        let sha = format!("{index:040}");
        store.record_decision(repo, &sha, &decided(i64::from(reason == "started"), reason), Timestamp(1)).await.unwrap();
        assert_eq!(store.decision(repo, &sha).await.unwrap().map(|found| found.reason), Some(reason.to_owned()));
    }

    // The same commit left out of another, larger push: its runs still came.
    let first = format!("{:040}", 0);
    store.record_decision(repo, &first, &decided(0, "too_many_refs"), Timestamp(2)).await.unwrap();
    assert_eq!(store.decision(repo, &first).await.unwrap().map(|found| found.reason), Some("started".to_owned()));
}
