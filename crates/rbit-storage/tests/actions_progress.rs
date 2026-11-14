//! How far each unfinished run is, counted against a real, migrated `SQLite` database: the
//! runs list draws it as a ring.

use rbit_core::{
    RepoId, Timestamp, Visibility,
    actions::{RunEvent, RunId, RunnerCapabilities, RunnerId, RunnerLabel, RunnerScope, Status},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        actions_runners::{NewRegistrationToken, NewRunner},
        actions_runs::{ActionsRunStore, ClaimRequest, Claimed, NewJob, NewRun, RunProgress, RunStart},
    },
};

const NOW: Timestamp = Timestamp(1_000_000);
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

struct Queue {
    _dir: tempfile::TempDir,
    db: Database,
    repo: RepoId,
    runner: RunnerId,
    labels: Vec<RunnerLabel>,
    /// Seconds since [`NOW`]: every step of a test happens a second after the last.
    clock: i64,
    claims: u8,
}

impl Queue {
    async fn new() -> Self {
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
        let repo = db.repositories().create(new).await.unwrap().id;
        db.actions_settings().set_enabled(repo, true, NOW).await.unwrap();
        let labels: Vec<RunnerLabel> = vec!["linux".parse().unwrap()];
        let scope = RunnerScope::Repository(repo);
        let registration = [7; 32];
        let runners = db.actions_runners();
        runners
            .create_registration_token(&NewRegistrationToken {
                digest: &registration,
                scope,
                labels: None,
                uses: Some(1),
                created_by: Some(alice),
                created_at: NOW,
                expires_at: Timestamp(NOW.0 + 3600),
            })
            .await
            .unwrap();
        let runner = runners
            .register(&NewRunner {
                registration_digest: &registration,
                uuid: "00000000-0000-0000-0000-000000000007",
                token_digest: &[107; 32],
                name: "runner",
                version: "v12.0.0",
                labels: &labels,
                ephemeral: false,
                capabilities: &RunnerCapabilities::default(),
                protocol: 1,
                now: NOW,
            })
            .await
            .unwrap()
            .unwrap()
            .id;
        Self { _dir: dir, db, repo, runner, labels, clock: 0, claims: 0 }
    }

    fn store(&self) -> ActionsRunStore {
        self.db.actions_runs()
    }

    fn tick(&mut self) -> Timestamp {
        self.clock += 1;
        Timestamp(NOW.0 + self.clock)
    }

    /// A run of `build`, then `test` (which needs it), then `ship` (which needs `test`).
    async fn pipeline(&mut self) -> RunId {
        let (none, linux) = (Vec::new(), vec!["linux".to_owned()]);
        let (build, test) = (vec!["build".to_owned()], vec!["test".to_owned()]);
        let jobs = [new_job("build", &none, &linux), new_job("test", &build, &linux), new_job("ship", &test, &linux)];
        self.run(&jobs).await
    }

    /// A run of `jobs`.
    async fn run(&mut self, jobs: &[NewJob<'_>]) -> RunId {
        let now = self.tick();
        let run = NewRun {
            repo: self.repo,
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
            jobs,
            error: None,
            now,
        };
        self.store().create_run(&run).await.unwrap().id
    }

    /// The runner asks for work.
    async fn claim(&mut self) -> Option<Claimed> {
        self.claims += 1;
        let (now, digest) = (self.tick(), [self.claims; 32]);
        let request = ClaimRequest {
            runner: self.runner,
            scope: RunnerScope::Repository(self.repo),
            labels: &self.labels,
            host_labels: &[],
            ephemeral: false,
            instance_runners_for_all: false,
            token_digest: &digest,
            idempotency_key: None,
            now,
        };
        self.store().claim(&request).await.unwrap()
    }

    /// The runner takes its next job and finishes it with `result`: the run and the job's key.
    async fn work(&mut self, result: Status) -> (RunId, String) {
        let claimed = self.claim().await.expect("a job waits");
        let now = self.tick();
        self.store().finish_task(claimed.task.id, result, now).await.unwrap();
        (claimed.job.run_id, claimed.job.key)
    }
}

fn new_job<'a>(key: &'a str, needs: &'a [String], runs_on: &'a [String]) -> NewJob<'a> {
    NewJob {
        key,
        name: key,
        matrix: None,
        needs,
        runs_on,
        permissions: None,
        payload: b"jobs: {}\n",
        condition: false,
        max_parallel: None,
        group: None,
        environment: None,
    }
}

#[tokio::test]
async fn progress_counts_the_jobs_of_each_run_by_state() {
    let mut queue = Queue::new().await;
    let first = queue.pipeline().await;
    queue.work(Status::Success).await;
    let running = queue.claim().await.unwrap();
    assert_eq!((running.job.run_id, running.job.key.as_str()), (first, "test"));
    let second = queue.pipeline().await;

    let progress = queue.store().progress(&[first, second]).await.unwrap();
    assert_eq!(progress[&first], RunProgress { total: 3, done: 1, running: 1, waiting: 0 });
    assert_eq!(progress[&second], RunProgress { total: 3, done: 0, running: 0, waiting: 1 });
    assert!(queue.store().progress(&[]).await.unwrap().is_empty());
}
