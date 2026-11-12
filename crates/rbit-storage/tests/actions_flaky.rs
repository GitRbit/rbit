//! Flaky tests against a real, migrated `SQLite` database: a failed test is flaky when the
//! same test passed at the same commit in another attempt or run of a job of the same name,
//! even when that job failed for another test.

use std::collections::BTreeSet;

use rbit_core::{
    RepoId, Timestamp, Visibility,
    actions::{RunEvent, RunnerCapabilities, RunnerId, RunnerLabel, RunnerScope, TaskId},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        actions_runners::{NewRegistrationToken, NewRunner},
        actions_runs::{ClaimRequest, NewJob, NewRun, RunStart, StoredReport, StoredTest},
    },
};

const NOW: Timestamp = Timestamp(1_000_000);
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn repository(db: &Database) -> RepoId {
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
    let repo = db.repositories().create(new).await.unwrap().id;
    db.actions_settings().set_enabled(repo, true, NOW).await.unwrap();
    repo
}

/// A run of job `tests` at [`SHA`], claimed by `runner`: its task.
async fn tested(db: &Database, repo: RepoId, runner: RunnerId, labels: &[RunnerLabel], seed: u8) -> TaskId {
    let (none, runs_on) = (Vec::new(), vec!["linux".to_owned()]);
    let jobs = [NewJob {
        key: "tests",
        name: "tests",
        matrix: None,
        needs: &none,
        runs_on: &runs_on,
        permissions: None,
        payload: b"jobs: {}\n",
        condition: false,
        max_parallel: None,
        group: None,
        environment: None,
    }];
    let run = NewRun {
        repo,
        workflow_path: ".rbit/workflows/ci.yml",
        workflow_name: "CI",
        title: "Try",
        event: RunEvent::Push,
        reference: "refs/heads/main",
        sha: SHA,
        actor: None,
        trigger_actor: None,
        pull_number: None,
        is_fork: false,
        trusted: true,
        concurrency_group: None,
        cancel_in_progress: false,
        event_payload: "{}",
        dispatch_inputs: None,
        start: RunStart::Runnable,
        jobs: &jobs,
        error: None,
        now: NOW,
    };
    let store = db.actions_runs();
    store.create_run(&run).await.unwrap();
    let digest = [seed; 32];
    let claim = ClaimRequest {
        runner,
        scope: RunnerScope::Repository(repo),
        labels,
        host_labels: &[],
        ephemeral: false,
        instance_runners_for_all: false,
        token_digest: &digest,
        idempotency_key: None,
        now: Timestamp(NOW.0 + 10),
    };
    store.claim(&claim).await.unwrap().unwrap().task.id
}

/// A report whose `failed` tests failed and whose `passed` tests passed.
fn report(failed: &[&str], passed: &[&str]) -> StoredReport {
    let items = failed.iter().map(|id| StoredTest { id: (*id).to_owned(), path: None, line: None, message: "boom".to_owned() }).collect();
    let passed: Vec<String> = passed.iter().map(|id| (*id).to_owned()).collect();
    let total = u64::try_from(failed.len() + passed.len()).unwrap();
    StoredReport { total, failed: u64::try_from(failed.len()).unwrap(), items, passed }
}

/// A runner with label `linux`, registered with `registration`.
async fn runner(db: &Database, registration: &[u8; 32], seed: u8) -> (RunnerId, Vec<RunnerLabel>) {
    let labels: Vec<RunnerLabel> = vec!["linux".parse().unwrap()];
    let uuid = format!("00000000-0000-0000-0000-0000000000{seed:02x}");
    let new = NewRunner {
        registration_digest: registration,
        uuid: &uuid,
        token_digest: &[seed.wrapping_add(100); 32],
        name: "runner",
        version: "v12.0.0",
        labels: &labels,
        ephemeral: false,
        capabilities: &RunnerCapabilities::default(),
        protocol: 1,
        now: NOW,
    };
    (db.actions_runners().register(&new).await.unwrap().unwrap().id, labels)
}

#[tokio::test]
async fn a_test_that_passed_in_another_failed_attempt_is_flaky() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let repo = repository(&db).await;
    let registration = [7; 32];
    let token = NewRegistrationToken {
        digest: &registration,
        scope: RunnerScope::Repository(repo),
        labels: None,
        uses: Some(2),
        created_by: None,
        created_at: NOW,
        expires_at: Timestamp(NOW.0 + 3600),
    };
    db.actions_runners().create_registration_token(&token).await.unwrap();
    let (one, labels) = runner(&db, &registration, 1).await;
    let (two, _) = runner(&db, &registration, 2).await;
    let store = db.actions_runs();

    // Two runs of `tests` at one commit, both failing: `a` fails in both, `b` only in the
    // second, after passing in the first.
    let first = tested(&db, repo, one, &labels, 1).await;
    store.record_test_report(first, &report(&["a"], &["b", "c"])).await.unwrap();
    let second = tested(&db, repo, two, &labels, 2).await;
    store.record_test_report(second, &report(&["a", "b"], &["c"])).await.unwrap();
    let asked = vec!["a".to_owned(), "b".to_owned()];
    let flaky = store.flaky_tests(repo, SHA, &asked).await.unwrap();
    assert_eq!(flaky.into_iter().collect::<Vec<_>>(), ["b"]);
    assert_eq!(store.flaky_tests(repo, "f".repeat(40).as_str(), &asked).await.unwrap(), BTreeSet::new(), "another commit");

    // A newer report of the same attempt replaces the passed tests too.
    store.record_test_report(first, &report(&["a", "b"], &["c"])).await.unwrap();
    assert_eq!(store.flaky_tests(repo, SHA, &asked).await.unwrap(), BTreeSet::new());
}
