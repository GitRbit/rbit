//! Background jobs and operational checks.
//!
//! Two kinds of work run in the background, both started by [`JobService::spawn_background`]:
//!
//! * **Cache warming.** On [`DomainEvent::RepositoryPushed`], the language breakdown and the
//!   root directory's last-commit map of the default branch are computed and cached, so
//!   the first visitor after a push gets a fast page. Pushes to a repository whose warm-up
//!   is already queued are coalesced.
//! * **Maintenance**, every `[operations] maintenance_interval_minutes` (± 10 % jitter so
//!   several instances or restarts do not synchronise): `git gc --auto` for every
//!   repository on the first run after start-up and afterwards only for repositories pushed
//!   to since the previous run, then pruning of the insights cache by age and size. An
//!   idle instance therefore does nothing but wake up on schedule and prune.
//!
//! All jobs share one semaphore (`[operations] job_concurrency`), so a burst of pushes
//! or a large instance never runs more than that many Git computations at once. Jobs are
//! system work: they load repositories with [`RepoContext::load_for_system`], which grants
//! read access only.
//!
//! [`JobService::shutdown`] stops accepting work, interrupts queued and running jobs and
//! waits (bounded) for them to finish. Interrupting `git gc` is safe: Git writes new packs
//! under temporary names and only then removes the old ones.

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use rbit_core::RepoId;
use rbit_git::Maintenance;
use rbit_storage::sqlite::admin::ListFilter;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast::error::RecvError};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{DomainEvent, InsightsService, RepoContext, ServiceContext, ServiceError};

/// How long [`JobService::shutdown`] waits for jobs to wind down.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Repositories fetched per page during a full maintenance sweep.
const SWEEP_PAGE_SIZE: i64 = 200;
/// Upper bound for one health probe; a store that takes longer is reported unhealthy.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

/// The kinds of job, as used in log fields and metric labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    WarmCaches,
    GitGc,
    PruneCache,
}

impl Job {
    fn label(self) -> &'static str {
        match self {
            Self::WarmCaches => "warm_caches",
            Self::GitGc => "git_gc",
            Self::PruneCache => "prune_insights_cache",
        }
    }
}

/// What one maintenance run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaintenanceSummary {
    /// Repositories `git gc --auto` ran for successfully.
    pub collected: u64,
    /// Repositories where it failed (logged individually).
    pub failed: u64,
    /// Insights cache entries removed.
    pub pruned: u64,
}

/// Result of the health probes behind `/healthz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthReport {
    /// SQLite answered a trivial query.
    pub database: bool,
    /// redb answered a trivial read.
    pub kv: bool,
}

impl HealthReport {
    pub fn is_healthy(self) -> bool {
        self.database && self.kv
    }
}

#[derive(Debug, Clone)]
pub struct JobService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    insights: InsightsService,
    maintenance: Maintenance,
    permits: Arc<Semaphore>,
    shutdown: CancellationToken,
    tasks: TaskTracker,
    started: AtomicBool,
    /// Repositories with a cache warm-up queued but not yet started.
    warm_queue: Mutex<HashSet<RepoId>>,
    /// Repositories pushed to since the last maintenance run; `None` until the first run
    /// has swept every repository.
    pushed: Mutex<Option<HashSet<RepoId>>>,
}

impl JobService {
    pub(crate) fn new(ctx: ServiceContext) -> Self {
        let operations = &ctx.config.operations;
        Self {
            inner: Arc::new(Inner {
                insights: InsightsService::new(ctx.clone()),
                maintenance: Maintenance::new(ctx.config.repository.git_binary.clone()),
                permits: Arc::new(Semaphore::new(operations.job_concurrency())),
                shutdown: CancellationToken::new(),
                tasks: TaskTracker::new(),
                started: AtomicBool::new(false),
                warm_queue: Mutex::new(HashSet::new()),
                pushed: Mutex::new(None),
                ctx,
            }),
        }
    }

    /// Starts the push subscriber and the maintenance schedule. Called once at server
    /// start-up; later calls do nothing.
    pub fn spawn_background(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        // Subscribe before returning so no push published after start-up is missed.
        let events = self.inner.ctx.events.subscribe();
        let service = self.clone();
        self.inner.tasks.spawn(async move { service.listen(events).await });

        if let Some(interval) = self.inner.ctx.config.operations.maintenance_interval() {
            let service = self.clone();
            self.inner.tasks.spawn(async move { service.maintain_periodically(interval).await });
            tracing::info!(interval_minutes = interval.as_secs() / 60, "repository maintenance scheduled");
        } else {
            tracing::info!("periodic repository maintenance is disabled");
        }
    }

    /// Stops background work: nothing new starts, queued and running jobs are interrupted,
    /// and this waits up to a few seconds for them to finish. Call during graceful
    /// shutdown, before closing the database.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.tasks.close();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.inner.tasks.wait()).await.is_err() {
            tracing::warn!("background jobs did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    /// Probes SQLite and redb with trivial reads.
    pub async fn health(&self) -> HealthReport {
        let ctx = &self.inner.ctx;
        let database = async { matches!(tokio::time::timeout(HEALTH_TIMEOUT, ctx.database.admin().sqlite_version()).await, Ok(Ok(_))) };
        let kv = async { matches!(tokio::time::timeout(HEALTH_TIMEOUT, ctx.kv.insights().check()).await, Ok(Ok(()))) };
        let (database, kv) = tokio::join!(database, kv);
        HealthReport { database, kv }
    }

    async fn listen(self, mut events: tokio::sync::broadcast::Receiver<DomainEvent>) {
        let warm = self.inner.ctx.config.operations.warm_caches_on_push;
        loop {
            let event = tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                event = events.recv() => event,
            };
            match event {
                Ok(DomainEvent::RepositoryPushed { repo, .. }) => {
                    self.note_push(repo);
                    if warm {
                        self.queue_warm(repo);
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "job subscriber fell behind; some pushes were not processed"),
                Err(RecvError::Closed) => break,
            }
        }
    }

    /// Remembers a push so the next maintenance run collects the repository.
    fn note_push(&self, repo: RepoId) {
        if let Some(pushed) = lock(&self.inner.pushed).as_mut() {
            pushed.insert(repo);
        }
    }

    /// Queues a cache warm-up unless one is already waiting for this repository.
    fn queue_warm(&self, repo: RepoId) {
        if !lock(&self.inner.warm_queue).insert(repo) {
            return;
        }
        let service = self.clone();
        self.inner.tasks.spawn(async move {
            let inner = &service.inner;
            let Some(_permit) = acquire(inner).await else { return };
            // A push from now on queues a fresh warm-up: this one may read the old tip.
            lock(&inner.warm_queue).remove(&repo);
            let result = tokio::select! {
                () = inner.shutdown.cancelled() => return,
                result = timed(Job::WarmCaches, service.warm_caches(repo)) => result,
            };
            if let Err(error) = result {
                tracing::warn!(%error, repo = repo.0, "warming insights caches failed");
            }
        });
    }

    /// Computes (and so caches) the language breakdown and the root directory's last
    /// commits for the default branch. Empty repositories and a missing default branch
    /// are not errors: there is nothing to show yet.
    pub(crate) async fn warm_caches(&self, repo: RepoId) -> Result<(), ServiceError> {
        let Some(context) = RepoContext::load_for_system(&self.inner.ctx, repo).await? else { return Ok(()) };
        if context.reader.is_empty().await? {
            return Ok(());
        }
        let branch = context.repo.repo.default_branch.clone();
        let warmed = async {
            self.inner.insights.languages(&context, &branch).await?;
            let tree = context.reader.tree(branch.clone(), String::new()).await?;
            self.inner.insights.last_commits(&context, &branch, "", &tree.entries).await?;
            Ok(())
        };
        match warmed.await {
            Err(ServiceError::NotFound) => Ok(()),
            result => {
                if result.is_ok() {
                    tracing::debug!(repo = %context.repo.full_name(), branch, "insights caches warmed");
                }
                result
            }
        }
    }

    async fn maintain_periodically(self, interval: Duration) {
        loop {
            tokio::select! {
                () = self.inner.shutdown.cancelled() => break,
                () = tokio::time::sleep(jittered(interval)) => {}
            }
            let summary = self.run_maintenance().await;
            tracing::info!(collected = summary.collected, failed = summary.failed, pruned = summary.pruned, "maintenance finished");
        }
    }

    /// One maintenance run: `git gc --auto` where needed, then cache pruning.
    pub async fn run_maintenance(&self) -> MaintenanceSummary {
        let mut summary = MaintenanceSummary::default();
        let candidates = match self.gc_candidates().await {
            Ok(candidates) => candidates,
            Err(error) => {
                tracing::warn!(%error, "cannot list repositories for maintenance");
                Vec::new()
            }
        };

        let mut running = tokio::task::JoinSet::new();
        for repo in candidates {
            let Some(permit) = acquire(&self.inner).await else { break };
            let service = self.clone();
            running.spawn(async move {
                let _permit = permit;
                tokio::select! {
                    () = service.inner.shutdown.cancelled() => None,
                    result = timed(Job::GitGc, service.collect_garbage(repo)) => Some((repo, result)),
                }
            });
            // Reap finished jobs as we go so the set stays small on large instances.
            while let Some(done) = running.try_join_next() {
                tally(&mut summary, done);
            }
        }
        while let Some(done) = running.join_next().await {
            tally(&mut summary, done);
        }

        if !self.inner.shutdown.is_cancelled() {
            match timed(Job::PruneCache, self.prune_cache()).await {
                Ok(pruned) => summary.pruned = pruned,
                Err(error) => tracing::warn!(%error, "pruning the insights cache failed"),
            }
        }
        summary
    }

    /// Every repository on the first run, afterwards those pushed to since the last run.
    async fn gc_candidates(&self) -> Result<Vec<RepoId>, ServiceError> {
        let since_last_run = lock(&self.inner.pushed).replace(HashSet::new());
        if let Some(pushed) = since_last_run {
            return Ok(pushed.into_iter().collect());
        }
        let admin = self.inner.ctx.database.admin();
        let mut all = Vec::new();
        loop {
            let offset = i64::try_from(all.len()).unwrap_or(i64::MAX);
            let page = admin.list_repositories(ListFilter { search: "", limit: SWEEP_PAGE_SIZE, offset }).await?;
            let fetched = page.items.len();
            all.extend(page.items.into_iter().map(|repo| repo.repo.id));
            if fetched < usize::try_from(SWEEP_PAGE_SIZE).unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(all)
    }

    /// `git gc --auto` for `repo`; `false` when the repository no longer exists.
    async fn collect_garbage(&self, repo: RepoId) -> Result<bool, ServiceError> {
        let Some(context) = RepoContext::load_for_system(&self.inner.ctx, repo).await? else { return Ok(false) };
        // `git gc` has the repository's files open, so a transfer waits for it.
        let _lease = self.inner.ctx.repositories.lease(repo).await;
        self.inner.maintenance.gc_auto(&context.disk_path).await.map_err(ServiceError::internal)?;
        Ok(true)
    }

    async fn prune_cache(&self) -> Result<u64, ServiceError> {
        let operations = &self.inner.ctx.config.operations;
        let max_entries = (operations.insights_cache_max_entries > 0).then_some(operations.insights_cache_max_entries);
        let report = self.inner.ctx.kv.insights().prune(operations.insights_cache_max_age(), max_entries).await?;
        #[allow(clippy::cast_precision_loss)] // A gauge; precision beyond 2^53 entries is moot.
        metrics::gauge!("rbit_insights_cache_entries").set(report.remaining as f64);
        Ok(report.expired + report.evicted)
    }
}

/// Waits for a job slot, or `None` once shutdown has begun. The slot is returned when
/// the permit is dropped.
async fn acquire(inner: &Inner) -> Option<OwnedSemaphorePermit> {
    tokio::select! {
        () = inner.shutdown.cancelled() => None,
        permit = Arc::clone(&inner.permits).acquire_owned() => permit.ok(),
    }
}

/// Runs `job`, recording its outcome and duration as metrics.
async fn timed<T>(job: Job, work: impl Future<Output = Result<T, ServiceError>>) -> Result<T, ServiceError> {
    let started = Instant::now();
    let result = work.await;
    let outcome = if result.is_ok() { "ok" } else { "error" };
    metrics::counter!("rbit_jobs_total", "job" => job.label(), "outcome" => outcome).increment(1);
    metrics::histogram!("rbit_job_duration_seconds", "job" => job.label()).record(started.elapsed().as_secs_f64());
    result
}

/// The outcome of one `git gc` task: `None` if interrupted by shutdown.
type GcOutcome = Option<(RepoId, Result<bool, ServiceError>)>;

fn tally(summary: &mut MaintenanceSummary, done: Result<GcOutcome, tokio::task::JoinError>) {
    match done {
        Ok(Some((_, Ok(true)))) => summary.collected += 1,
        Ok(Some((_, Ok(false))) | None) => {}
        Ok(Some((repo, Err(error)))) => {
            summary.failed += 1;
            tracing::warn!(%error, repo = repo.0, "git gc failed");
        }
        Err(error) => {
            summary.failed += 1;
            tracing::warn!(%error, "git gc task failed");
        }
    }
}

/// `interval` ± 10 %, so restarts and several instances do not run in lockstep.
fn jittered(interval: Duration) -> Duration {
    let random = getrandom::u32().unwrap_or(u32::MAX / 2);
    // Maps the random value onto [0.9, 1.1).
    let factor = 0.9 + 0.2 * (f64::from(random) / (f64::from(u32::MAX) + 1.0));
    interval.mul_f64(factor)
}

/// Locks a mutex, recovering the data if a previous holder panicked (the sets stay valid).
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command, sync::Arc};

    use rbit_core::{Config, Visibility};
    use rbit_storage::{Database, KvStore};

    use super::*;
    use crate::{CreateRepository, Services, SignUp};

    struct Fixture {
        _dir: tempfile::TempDir,
        services: Services,
        kv: KvStore,
        repo: RepoId,
        disk_path: std::path::PathBuf,
    }

    async fn fixture(configure: impl FnOnce(&mut Config)) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().join("data");
        configure(&mut config);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();

        let signup = SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
        let user = services.accounts.create_user(signup, Some(true)).await.unwrap();
        let input = CreateRepository { name: "demo".into(), description: String::new(), visibility: Visibility::Public };
        let created = services.repositories.create(&user, input).await.unwrap();
        let disk_path = services.repositories.open(Some(&user), "ada", "demo").await.unwrap().disk_path;
        Fixture { _dir: dir, services, kv, repo: created.repo.id, disk_path }
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// Pushes one commit with a Rust file to `main` of the bare repository.
    fn push_commit(bare: &Path) {
        let work = tempfile::tempdir().unwrap();
        git(work.path(), &["init", "-q"]);
        std::fs::write(work.path().join("main.rs"), "fn main() {}\n").unwrap();
        git(work.path(), &["add", "."]);
        git(work.path(), &["commit", "-q", "-m", "Initial commit"]);
        git(work.path(), &["push", "-q", bare.to_str().unwrap(), "HEAD:refs/heads/main"]);
    }

    #[tokio::test]
    async fn a_push_warms_the_default_branch_caches() {
        let fixture = fixture(|_| {}).await;
        push_commit(&fixture.disk_path);
        fixture.services.jobs.spawn_background();
        fixture.services.events.publish(DomainEvent::RepositoryPushed { repo: fixture.repo, pusher: None, updates: Vec::new() });

        let cache = fixture.kv.insights();
        let deadline = Instant::now() + Duration::from_secs(20);
        // Languages and the root last-commit map: two entries.
        while cache.entry_count().await.unwrap() < 2 {
            assert!(Instant::now() < deadline, "caches were not warmed");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        fixture.services.jobs.shutdown().await;
    }

    #[tokio::test]
    async fn warming_an_empty_or_missing_repository_is_a_no_op() {
        let fixture = fixture(|_| {}).await;
        fixture.services.jobs.warm_caches(fixture.repo).await.unwrap();
        fixture.services.jobs.warm_caches(RepoId(999)).await.unwrap();
        assert_eq!(fixture.kv.insights().entry_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn maintenance_sweeps_everything_first_then_only_pushed_repositories() {
        let fixture = fixture(|config| config.operations.insights_cache_max_entries = 1).await;
        push_commit(&fixture.disk_path);
        fixture.services.jobs.warm_caches(fixture.repo).await.unwrap();
        let jobs = &fixture.services.jobs;

        let first = jobs.run_maintenance().await;
        assert_eq!(first, MaintenanceSummary { collected: 1, failed: 0, pruned: 1 });

        let idle = jobs.run_maintenance().await;
        assert_eq!(idle, MaintenanceSummary::default(), "nothing pushed, nothing to do");

        jobs.note_push(fixture.repo);
        jobs.note_push(RepoId(999));
        let after_push = jobs.run_maintenance().await;
        assert_eq!(after_push, MaintenanceSummary { collected: 1, failed: 0, pruned: 0 }, "deleted repositories are skipped quietly");
    }

    #[tokio::test]
    async fn shutdown_stops_background_work_promptly() {
        let fixture = fixture(|config| config.operations.maintenance_interval_minutes = 1).await;
        fixture.services.jobs.spawn_background();
        fixture.services.jobs.spawn_background();
        tokio::time::timeout(Duration::from_secs(5), fixture.services.jobs.shutdown()).await.unwrap();
    }

    #[tokio::test]
    async fn health_reports_both_stores() {
        let fixture = fixture(|_| {}).await;
        let report = fixture.services.jobs.health().await;
        assert!(report.is_healthy(), "{report:?}");
    }

    #[test]
    fn jitter_stays_within_ten_percent() {
        for _ in 0..100 {
            let delay = jittered(Duration::from_secs(1000));
            assert!((900..1100).contains(&delay.as_secs()), "{delay:?}");
        }
    }
}
