//! The order of the Actions queue against a real, migrated `SQLite` database: a run a runner
//! has started is finished before a run pushed after it starts, and a job run again waits
//! its turn.

use rbit_core::{
    RepoId, Timestamp, Visibility,
    actions::{RunEvent, RunId, RunnerCapabilities, RunnerId, RunnerLabel, RunnerScope, Status},
};
use rbit_storage::{
    Database, NewRepository, NewUser,
    sqlite::{
        actions_runners::{NewRegistrationToken, NewRunner},
        actions_runs::{ActionsRunStore, ClaimRequest, Claimed, Decision, NewJob, NewRun, Rerun, RunStart},
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
async fn a_started_run_is_finished_before_a_newer_run_starts() {
    let mut queue = Queue::new().await;
    let first = queue.pipeline().await;
    let second = queue.pipeline().await;
    let third = queue.pipeline().await;

    // One runner: by queue time alone `test` of the first run would wait behind `build`
    // of the two runs pushed after it, and `ship` behind their `test`.
    let mut order = Vec::new();
    for _ in 0..9 {
        order.push(queue.work(Status::Success).await);
    }
    let expected: Vec<(RunId, String)> =
        [first, second, third].into_iter().flat_map(|run| ["build", "test", "ship"].map(|key| (run, key.to_owned()))).collect();
    assert_eq!(order, expected);
    assert!(queue.claim().await.is_none());
}

#[tokio::test]
async fn runs_in_progress_go_in_the_order_they_started_and_a_rerun_waits_its_turn() {
    let mut queue = Queue::new().await;
    let first = queue.pipeline().await;
    let second = queue.pipeline().await;

    // Two runners' worth of claims start both runs; the first run's next job still goes first.
    let (one, two) = (queue.claim().await.unwrap(), queue.claim().await.unwrap());
    assert_eq!((one.job.run_id, two.job.run_id), (first, second));
    for claimed in [two, one] {
        let now = queue.tick();
        queue.store().finish_task(claimed.task.id, Status::Success, now).await.unwrap();
    }
    assert_eq!(queue.work(Status::Success).await, (first, "test".to_owned()));
    assert_eq!(queue.work(Status::Success).await, (first, "ship".to_owned()));

    // The first run is run again while a runner works on the second. Its new attempt has
    // not started, so its job, queued first, still waits for the run in progress.
    let testing = queue.claim().await.unwrap();
    assert_eq!((testing.job.run_id, testing.job.key.as_str()), (second, "test"));
    let now = queue.tick();
    queue.store().rerun(queue.repo, first, Rerun::All, RunStart::Runnable, now).await.unwrap().unwrap();
    let now = queue.tick();
    queue.store().finish_task(testing.task.id, Status::Success, now).await.unwrap();
    assert_eq!(queue.work(Status::Success).await, (second, "ship".to_owned()));
    assert_eq!(queue.work(Status::Success).await, (first, "build".to_owned()));
    assert_eq!(queue.work(Status::Success).await, (first, "test".to_owned()));
}

#[tokio::test]
async fn a_cancelled_run_is_wound_up_before_a_run_that_has_not_started() {
    let mut queue = Queue::new().await;
    let first = queue.pipeline().await;
    let (none, linux, build) = (Vec::new(), vec!["linux".to_owned()], vec!["build".to_owned()]);
    let jobs = [new_job("build", &none, &linux), NewJob { condition: true, ..new_job("report", &build, &linux) }];
    let cancelled = queue.run(&jobs).await;

    // The second run is cancelled before a runner took any of it. Its `always()` job still
    // runs, and until it has the run is not finished: it does not wait behind the first run.
    let now = queue.tick();
    queue.store().cancel_run(queue.repo, cancelled, now).await.unwrap().unwrap();
    let report = queue.store().jobs(cancelled).await.unwrap().into_iter().find(|job| job.key == "report").unwrap().id;
    let now = queue.tick();
    queue.store().decide(report, Decision::Run, now).await.unwrap().unwrap();

    assert_eq!(queue.work(Status::Success).await, (cancelled, "report".to_owned()));
    assert_eq!(queue.work(Status::Success).await, (first, "build".to_owned()));
}
