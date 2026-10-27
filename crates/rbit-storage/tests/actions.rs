//! The Actions stores against a real, migrated `SQLite` database: settings, runners and
//! registration, the run state machine (create, claim, finish, cancel, approve, re-run),
//! secrets and variables, and schedules.

use rbit_core::{
    AccountId, RepoId, Timestamp, Visibility,
    actions::{
        DefaultTokenPermission, JobId, JobStatus, RunEvent, RunnerBackend, RunnerCapabilities, RunnerId, RunnerLabel, RunnerScope,
        SealedSecret, SecretName, SecretScope, SecretVisibility, Status, VariableName,
    },
};
use rbit_storage::{
    Database, NewRepository, NewUser, StorageError,
    sqlite::{
        actions_runners::{Declaration, NewRegistrationToken, NewRunner, RegistrationRefusal},
        actions_runs::{ClaimRequest, Decision, NewJob, NewRun, Rerun, RunFilter, RunStart, StepUpdate},
        actions_schedules::NewSchedule,
    },
};

const NOW: Timestamp = Timestamp(1_000_000);
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

struct Fixture {
    dir: tempfile::TempDir,
    db: Database,
    alice: AccountId,
    bob: AccountId,
    /// alice/rocket, with Actions on.
    rocket: RepoId,
    /// alice/probe, with Actions on.
    probe: RepoId,
    /// bob/other, with Actions on.
    other: RepoId,
}

async fn user(db: &Database, name: &str) -> AccountId {
    db.accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: "not-a-real-hash".to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
        .id()
}

async fn repo(db: &Database, owner: AccountId, name: &str) -> RepoId {
    let repo = db
        .repositories()
        .create(NewRepository {
            owner_id: owner,
            name: name.parse().unwrap(),
            description: None,
            visibility: Visibility::Public,
            default_branch: "main".to_owned(),
        })
        .await
        .unwrap()
        .id;
    db.actions_settings().set_enabled(repo, true, NOW).await.unwrap();
    repo
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let alice = user(&db, "alice").await;
        let bob = user(&db, "bob").await;
        let rocket = repo(&db, alice, "rocket").await;
        let probe = repo(&db, alice, "probe").await;
        let other = repo(&db, bob, "other").await;
        Self { dir, db, alice, bob, rocket, probe, other }
    }

    async fn count(&self, table: &str) -> i64 {
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(self.dir.path().join("rbit.db")).read_only(true),
        )
        .await
        .unwrap();
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"))).fetch_one(&pool).await.unwrap()
    }

    /// A runner registered in `scope` with `labels`, and its id.
    async fn runner(&self, scope: RunnerScope, labels: &[&str], seed: u8) -> (RunnerId, Vec<RunnerLabel>) {
        let labels: Vec<RunnerLabel> = labels.iter().map(|label| label.parse().unwrap()).collect();
        let store = self.db.actions_runners();
        let registration = [seed; 32];
        store
            .create_registration_token(&NewRegistrationToken {
                digest: &registration,
                scope,
                labels: None,
                uses: Some(1),
                created_by: Some(self.alice),
                created_at: NOW,
                expires_at: Timestamp(NOW.0 + 3600),
            })
            .await
            .unwrap();
        let uuid = format!("00000000-0000-0000-0000-0000000000{seed:02x}");
        let runner = store
            .register(&NewRunner {
                registration_digest: &registration,
                uuid: &uuid,
                token_digest: &[seed.wrapping_add(100); 32],
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
            .unwrap();
        (runner.id, labels)
    }
}

fn job<'a>(key: &'a str, needs: &'a [String], runs_on: &'a [String]) -> NewJob<'a> {
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
    }
}

fn new_run<'a>(repo: RepoId, jobs: &'a [NewJob<'a>], start: RunStart) -> NewRun<'a> {
    NewRun {
        repo,
        workflow_path: ".forgejo/workflows/ci.yml",
        workflow_name: "CI",
        title: "Fix the build",
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
        start,
        jobs,
        error: None,
        now: NOW,
    }
}

fn claim<'a>(runner: RunnerId, scope: RunnerScope, labels: &'a [RunnerLabel], digest: &'a [u8; 32]) -> ClaimRequest<'a> {
    ClaimRequest {
        runner,
        scope,
        labels,
        ephemeral: false,
        instance_runners_for_all: false,
        token_digest: digest,
        idempotency_key: None,
        now: Timestamp(NOW.0 + 10),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn statuses(jobs: &[rbit_core::actions::Job]) -> Vec<(&str, JobStatus)> {
    jobs.iter().map(|job| (job.key.as_str(), job.status)).collect()
}

#[tokio::test]
async fn repository_settings_owner_policies_and_trusted_users() {
    let fx = Fixture::new().await;
    let settings = fx.db.actions_settings();
    let fresh = repo(&fx.db, fx.alice, "fresh").await;
    settings.set_enabled(fresh, false, NOW).await.unwrap();
    assert!(!settings.is_enabled(fresh).await.unwrap());
    let stored = settings.get(fresh).await.unwrap().unwrap();
    assert_eq!(stored.token_permission, DefaultTokenPermission::Read, "read by default");
    settings.set_token_permission(fresh, DefaultTokenPermission::Write, Timestamp(NOW.0 + 1)).await.unwrap();
    settings.set_enabled(fresh, true, Timestamp(NOW.0 + 2)).await.unwrap();
    let stored = settings.get(fresh).await.unwrap().unwrap();
    assert!(stored.enabled && stored.token_permission == DefaultTokenPermission::Write && stored.updated_at == Timestamp(NOW.0 + 2));

    assert!(!settings.instance_runners_allowed(fx.bob).await.unwrap());
    settings.set_instance_runners_allowed(fx.bob, true, fx.alice, NOW).await.unwrap();
    assert!(settings.instance_runners_allowed(fx.bob).await.unwrap());
    assert_eq!(settings.owners_with_instance_runners().await.unwrap(), [fx.bob]);

    assert!(settings.trust(fx.rocket, fx.bob, fx.alice, NOW).await.unwrap());
    assert!(!settings.trust(fx.rocket, fx.bob, fx.alice, NOW).await.unwrap(), "already trusted");
    assert!(settings.is_trusted(fx.rocket, fx.bob).await.unwrap());
    assert!(!settings.is_trusted(fx.probe, fx.bob).await.unwrap(), "trust is per repository");
    assert_eq!(settings.trusted_users(fx.rocket).await.unwrap()[0].approved_by, Some(fx.alice));
    assert!(settings.untrust(fx.rocket, fx.bob).await.unwrap());
    assert!(!settings.is_trusted(fx.rocket, fx.bob).await.unwrap());
}

#[tokio::test]
async fn registration_tokens_are_single_use_expire_and_pin_labels() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runners();
    let scope = RunnerScope::Repository(fx.rocket);
    let linux: Vec<RunnerLabel> = vec!["linux".parse().unwrap()];
    let token = store
        .create_registration_token(&NewRegistrationToken {
            digest: &[1; 32],
            scope,
            labels: Some(&linux),
            uses: Some(1),
            created_by: Some(fx.alice),
            created_at: NOW,
            expires_at: Timestamp(NOW.0 + 3600),
        })
        .await
        .unwrap();
    assert_eq!((token.uses_remaining, token.labels.as_deref()), (Some(1), Some(&linux[..])));
    assert!(matches!(
        store
            .create_registration_token(&NewRegistrationToken {
                digest: &[1; 32],
                scope,
                labels: None,
                uses: None,
                created_by: None,
                created_at: NOW,
                expires_at: Timestamp(NOW.0 + 1),
            })
            .await,
        Err(StorageError::Conflict(_))
    ));
    let register = |registration: [u8; 32], uuid: &'static str, digest: [u8; 32], labels: Vec<RunnerLabel>, now: Timestamp| {
        let store = store.clone();
        async move {
            store
                .register(&NewRunner {
                    registration_digest: &registration,
                    uuid,
                    token_digest: &digest,
                    name: "builder",
                    version: "v12",
                    labels: &labels,
                    ephemeral: false,
                    capabilities: &RunnerCapabilities::default(),
                    protocol: 1,
                    now,
                })
                .await
                .unwrap()
        }
    };
    let windows: Vec<RunnerLabel> = vec!["windows".parse().unwrap()];
    let uuid_a = "aaaaaaaa-0000-0000-0000-000000000001";
    let uuid_b = "aaaaaaaa-0000-0000-0000-000000000002";
    assert_eq!(register([1; 32], uuid_a, [9; 32], windows, NOW).await, Err(RegistrationRefusal::LabelsNotAllowed));
    let runner = register([1; 32], uuid_a, [9; 32], linux.clone(), NOW).await.unwrap();
    assert_eq!((runner.scope, runner.pinned_labels.as_deref()), (scope, Some(&linux[..])));
    assert_eq!(register([1; 32], uuid_b, [8; 32], linux.clone(), NOW).await, Err(RegistrationRefusal::InvalidToken), "single use");
    assert_eq!(register([7; 32], uuid_b, [8; 32], linux.clone(), NOW).await, Err(RegistrationRefusal::InvalidToken), "unknown");

    // Unlimited, but expiring and revocable.
    let unlimited = store
        .create_registration_token(&NewRegistrationToken {
            digest: &[2; 32],
            scope: RunnerScope::Owner(fx.alice),
            labels: None,
            uses: None,
            created_by: Some(fx.alice),
            created_at: NOW,
            expires_at: Timestamp(NOW.0 + 100),
        })
        .await
        .unwrap();
    assert!(register([2; 32], uuid_b, [8; 32], linux.clone(), NOW).await.is_ok());
    assert!(register([2; 32], "aaaaaaaa-0000-0000-0000-000000000003", [6; 32], linux.clone(), NOW).await.is_ok());
    assert_eq!(
        register([2; 32], "aaaaaaaa-0000-0000-0000-000000000004", [5; 32], linux.clone(), Timestamp(NOW.0 + 100)).await,
        Err(RegistrationRefusal::InvalidToken),
        "expired"
    );
    assert!(!store.revoke_registration_token(RunnerScope::Instance, unlimited.id, NOW).await.unwrap(), "scope must match");
    assert!(store.revoke_registration_token(RunnerScope::Owner(fx.alice), unlimited.id, NOW).await.unwrap());
    assert_eq!(
        register([2; 32], "aaaaaaaa-0000-0000-0000-000000000004", [5; 32], linux, NOW).await,
        Err(RegistrationRefusal::InvalidToken),
        "revoked"
    );
    assert_eq!(store.registration_tokens(RunnerScope::Owner(fx.alice)).await.unwrap().len(), 1);
    assert_eq!(store.delete_stale_registration_tokens(Timestamp(NOW.0 + 101)).await.unwrap(), 1);
    assert_eq!(store.registration_tokens(RunnerScope::Owner(fx.alice)).await.unwrap().len(), 0);
}

#[tokio::test]
async fn runners_are_found_by_token_and_managed_within_their_scope() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runners();
    let (repo_runner, _) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 1).await;
    let (owner_runner, _) = fx.runner(RunnerScope::Owner(fx.alice), &["linux"], 2).await;
    let (instance_runner, _) = fx.runner(RunnerScope::Instance, &["linux"], 3).await;
    let (_bob_runner, _) = fx.runner(RunnerScope::Owner(fx.bob), &["linux"], 4).await;
    assert_eq!(store.find_by_token(&[101; 32]).await.unwrap().map(|runner| runner.id), Some(repo_runner));
    assert_eq!(store.find_by_token(&[1; 32]).await.unwrap(), None, "the registration digest is not a runner token");
    let visible: Vec<_> = store.list_for_repository(fx.rocket, fx.alice).await.unwrap().into_iter().map(|runner| runner.id).collect();
    assert_eq!(visible, [repo_runner, owner_runner, instance_runner]);
    assert_eq!(store.list(RunnerScope::Instance).await.unwrap().len(), 1);
    assert_eq!(store.all().await.unwrap().len(), 4);

    let labels: Vec<RunnerLabel> = vec!["linux".parse().unwrap(), "x64".parse().unwrap()];
    let capabilities = RunnerCapabilities {
        backends: vec![RunnerBackend::Container, RunnerBackend::Host],
        os: "linux".into(),
        arch: "x86_64".into(),
        capacity: 2,
        host_labels: vec![labels[1].clone()],
    };
    let declaration = Declaration {
        name: "builder-2",
        labels: &labels,
        version: "v12.1",
        ephemeral: true,
        capabilities: &capabilities,
        protocol: 1,
        now: Timestamp(NOW.0 + 5),
    };
    let declared = store.declare(repo_runner, &declaration).await.unwrap().unwrap();
    assert_eq!((declared.name.as_str(), declared.ephemeral, &declared.capabilities), ("builder-2", true, &capabilities));
    assert!(declared.capabilities.runs_on_host());
    assert_eq!(store.declare(RunnerId(9_999), &declaration).await.unwrap(), None, "a runner that is gone");
    store.touch(&[repo_runner, owner_runner], Timestamp(NOW.0 + 7)).await.unwrap();
    let declared = store.find(repo_runner).await.unwrap().unwrap();
    assert_eq!((declared.labels, declared.version.as_str(), declared.last_online), (labels, "v12.1", Some(Timestamp(NOW.0 + 7))));
    assert_eq!(store.all_with_digests().await.unwrap()[0].1, [101; 32], "the cache is loaded with credential digests");

    assert!(!store.set_disabled(RunnerScope::Repository(fx.probe), repo_runner, true).await.unwrap(), "another repository's runner");
    assert!(store.set_disabled(RunnerScope::Repository(fx.rocket), repo_runner, true).await.unwrap());
    assert!(store.find(repo_runner).await.unwrap().unwrap().disabled);
    assert!(!store.delete(RunnerScope::Owner(fx.bob), owner_runner).await.unwrap());
    assert!(!store.delete_ephemeral(owner_runner).await.unwrap(), "not ephemeral");
    assert!(store.delete(RunnerScope::Owner(fx.alice), owner_runner).await.unwrap());
    assert_eq!(store.find(owner_runner).await.unwrap(), None);
}

#[tokio::test]
async fn runs_are_numbered_and_found_only_in_their_repository() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let labels = strings(&["linux"]);
    let no_needs: Vec<String> = Vec::new();
    let build_needs = strings(&["build"]);
    let jobs = [job("build", &no_needs, &labels), job("test", &build_needs, &labels)];
    let first = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    assert_eq!((first.number, first.status, first.attempt), (1, Status::Waiting, 1));
    assert_eq!(statuses(&store.jobs(first.id).await.unwrap()), [("build", Status::Waiting), ("test", Status::Blocked)]);
    let built = &store.jobs(first.id).await.unwrap()[0];
    assert_eq!((built.runs_on.as_slice(), built.queued_at), (&labels[..], Some(NOW)));

    let error = NewRun { error: Some("line 3, column 1: jobs must be a mapping"), ..new_run(fx.rocket, &[], RunStart::Runnable) };
    let failed = store.create_run(&error).await.unwrap();
    assert_eq!((failed.number, failed.status, failed.finished_at), (2, Status::Failure, Some(NOW)));
    assert!(failed.is_workflow_error() && store.jobs(failed.id).await.unwrap().is_empty());
    assert!(store.create_run(&new_run(fx.rocket, &[], RunStart::Runnable)).await.is_err(), "no jobs needs an error");

    let elsewhere = store.create_run(&new_run(fx.other, &jobs, RunStart::Runnable)).await.unwrap();
    assert_eq!(elsewhere.number, 1, "numbers are per repository");
    assert_eq!(store.find_run(fx.rocket, elsewhere.id).await.unwrap(), None);
    assert_eq!(store.find_run_by_number(fx.rocket, 2).await.unwrap().map(|run| run.id), Some(failed.id));
    let other_job = store.jobs(elsewhere.id).await.unwrap()[0].id;
    assert_eq!(store.find_job(fx.rocket, other_job).await.unwrap(), None);
    assert!(store.find_job(fx.other, other_job).await.unwrap().is_some());

    // Deleting runs keeps the numbers monotonic.
    sqlx_delete_runs(&fx).await;
    let after = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    assert_eq!(after.number, 3);

    let page = store.list_runs(fx.rocket, &RunFilter { limit: 10, ..RunFilter::default() }).await.unwrap();
    assert_eq!(page.iter().map(|run| run.number).collect::<Vec<_>>(), [3]);
    let filtered = RunFilter { event: Some(RunEvent::Schedule), limit: 10, ..RunFilter::default() };
    assert_eq!(store.list_runs(fx.rocket, &filtered).await.unwrap(), []);
    let before = RunFilter { before: Some(after.id), limit: 10, ..RunFilter::default() };
    assert_eq!(store.list_runs(fx.rocket, &before).await.unwrap(), []);
    assert_eq!(store.event(after.id).await.unwrap(), Some(("{}".to_owned(), None)));
}

// Actions: integration (I).
#[tokio::test]
async fn runs_are_paged_by_offset_counted_and_filtered_by_several_statuses() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let (none, linux) = (Vec::new(), strings(&["linux"]));
    let jobs = [job("build", &none, &linux)];
    let mut runs = Vec::new();
    for _ in 0..4 {
        runs.push(store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap());
    }
    store.create_run(&new_run(fx.other, &jobs, RunStart::Runnable)).await.unwrap();
    store.cancel_run(fx.rocket, runs[0].id, NOW).await.unwrap();
    let failed = NewRun { error: Some("broken"), ..new_run(fx.rocket, &[], RunStart::Runnable) };
    store.create_run(&failed).await.unwrap();

    let numbers = |runs: Vec<rbit_core::actions::Run>| runs.iter().map(|run| run.number).collect::<Vec<_>>();
    let page = RunFilter { limit: 2, offset: 1, ..RunFilter::default() };
    assert_eq!(numbers(store.list_runs(fx.rocket, &page).await.unwrap()), [4, 3]);
    assert_eq!(store.count_runs(fx.rocket, &page).await.unwrap(), 5, "the count ignores the page");
    let past = RunFilter { limit: 2, offset: 5, ..RunFilter::default() };
    assert_eq!(store.list_runs(fx.rocket, &past).await.unwrap(), []);

    let done = [Status::Success, Status::Failure, Status::Cancelled, Status::Skipped];
    let finished = RunFilter { statuses: &done, limit: 10, ..RunFilter::default() };
    assert_eq!(numbers(store.list_runs(fx.rocket, &finished).await.unwrap()), [5, 1]);
    assert_eq!(store.count_runs(fx.rocket, &finished).await.unwrap(), 2);
    let both = RunFilter { status: Some(Status::Cancelled), ..finished.clone() };
    assert_eq!(numbers(store.list_runs(fx.rocket, &both).await.unwrap()), [1], "status and statuses apply together");
    let schedule = RunFilter { event: Some(RunEvent::Schedule), ..finished };
    assert_eq!(store.count_runs(fx.rocket, &schedule).await.unwrap(), 0);
    assert_eq!(store.count_runs(fx.other, &RunFilter::default()).await.unwrap(), 1);
}

async fn sqlx_delete_runs(fx: &Fixture) {
    assert!(fx.db.actions_runs().delete_finished_before(Timestamp(NOW.0 + 1)).await.unwrap() >= 1);
    assert_eq!(fx.db.actions_runs().find_run_by_number(fx.rocket, 2).await.unwrap(), None, "the finished workflow error went");
    let unfinished = fx.db.actions_runs().find_run_by_number(fx.rocket, 1).await.unwrap().unwrap();
    fx.db.actions_runs().cancel_run(fx.rocket, unfinished.id, NOW).await.unwrap();
    fx.db.actions_runs().delete_finished_before(Timestamp(NOW.0 + 1)).await.unwrap();
    assert_eq!(fx.db.actions_runs().list_runs(fx.rocket, &RunFilter { limit: 10, ..RunFilter::default() }).await.unwrap(), []);
}

#[tokio::test]
async fn claims_match_labels_scope_and_the_repository_switch() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux_x64 = strings(&["linux", "x64"]);
    let jobs = [job("build", &none, &linux_x64)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();

    let (linux_only, linux_labels) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 1).await;
    assert_eq!(store.claim(&claim(linux_only, RunnerScope::Repository(fx.rocket), &linux_labels, &[50; 32])).await.unwrap(), None);
    let (probe_runner, probe_labels) = fx.runner(RunnerScope::Repository(fx.probe), &["linux", "x64"], 2).await;
    assert_eq!(store.claim(&claim(probe_runner, RunnerScope::Repository(fx.probe), &probe_labels, &[50; 32])).await.unwrap(), None);
    let (bob_runner, bob_labels) = fx.runner(RunnerScope::Owner(fx.bob), &["linux", "x64"], 3).await;
    assert_eq!(store.claim(&claim(bob_runner, RunnerScope::Owner(fx.bob), &bob_labels, &[50; 32])).await.unwrap(), None);
    let (instance, instance_labels) = fx.runner(RunnerScope::Instance, &["linux", "x64", "big"], 4).await;
    assert_eq!(
        store.claim(&claim(instance, RunnerScope::Instance, &instance_labels, &[50; 32])).await.unwrap(),
        None,
        "instance runners serve allowed owners only"
    );
    fx.db.actions_settings().set_enabled(fx.rocket, false, NOW).await.unwrap();
    let everyone = ClaimRequest { instance_runners_for_all: true, ..claim(instance, RunnerScope::Instance, &instance_labels, &[50; 32]) };
    assert_eq!(store.claim(&everyone).await.unwrap(), None, "Actions turned off for the repository");
    fx.db.actions_settings().set_enabled(fx.rocket, true, NOW).await.unwrap();
    fx.db.actions_settings().set_instance_runners_allowed(fx.alice, true, fx.alice, NOW).await.unwrap();
    let claimed = store.claim(&claim(instance, RunnerScope::Instance, &instance_labels, &[50; 32])).await.unwrap().unwrap();
    assert_eq!((claimed.job.status, claimed.job.runner_id, claimed.job.task_id), (Status::Running, Some(instance), Some(claimed.task.id)));
    assert_eq!((claimed.task.status, claimed.task.attempt, claimed.task.runner_id), (Status::Running, 1, Some(instance)));
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Running);
    assert_eq!(store.run(run.id).await.unwrap().unwrap().started_at, Some(Timestamp(NOW.0 + 10)));
    assert_eq!(store.claim(&claim(instance, RunnerScope::Instance, &instance_labels, &[51; 32])).await.unwrap(), None, "taken");

    // Owner runners take every repository of their owner; the oldest waiting job first.
    let first = store.create_run(&new_run(fx.probe, &jobs, RunStart::Runnable)).await.unwrap();
    let later = NewRun { now: Timestamp(NOW.0 + 1), ..new_run(fx.rocket, &jobs, RunStart::Runnable) };
    store.create_run(&later).await.unwrap();
    let (owner_runner, owner_labels) = fx.runner(RunnerScope::Owner(fx.alice), &["linux", "x64"], 5).await;
    let next = store.claim(&claim(owner_runner, RunnerScope::Owner(fx.alice), &owner_labels, &[52; 32])).await.unwrap().unwrap();
    assert_eq!(next.job.run_id, first.id);
}

#[tokio::test]
async fn jobs_without_labels_are_never_claimed() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    store.create_run(&new_run(fx.rocket, &[job("bare", &none, &none)], RunStart::Runnable)).await.unwrap();
    let (runner, labels) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 1).await;
    assert_eq!(store.claim(&claim(runner, RunnerScope::Repository(fx.rocket), &labels, &[50; 32])).await.unwrap(), None);
}

#[tokio::test]
async fn two_runners_claiming_one_job_get_it_once() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let jobs = [job("build", &none, &linux)];
    store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let (a, a_labels) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 1).await;
    let (b, b_labels) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 2).await;
    let (first, second) = (
        claim(a, RunnerScope::Repository(fx.rocket), &a_labels, &[60; 32]),
        claim(b, RunnerScope::Repository(fx.rocket), &b_labels, &[61; 32]),
    );
    let mut claimed = 0;
    for _ in 0..3 {
        let (one, two) = tokio::join!(store.claim(&first), store.claim(&second));
        let (one, two) = (one.unwrap(), two.unwrap());
        assert!(one.is_none() || two.is_none(), "never both: {one:?} {two:?}");
        claimed += usize::from(one.is_some()) + usize::from(two.is_some());
    }
    assert_eq!(claimed, 1);
    assert_eq!(fx.count("actions_tasks").await, 1);
}

#[tokio::test]
async fn finishing_tasks_unblocks_dependents_and_settles_the_run() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let build = strings(&["build"]);
    let jobs = [job("build", &none, &linux), job("lint", &none, &linux), job("test", &build, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let (runner, labels) = fx.runner(RunnerScope::Repository(fx.rocket), &["linux"], 1).await;
    let scope = RunnerScope::Repository(fx.rocket);

    let first =
        store.claim(&ClaimRequest { idempotency_key: Some("k1"), ..claim(runner, scope, &labels, &[70; 32]) }).await.unwrap().unwrap();
    assert_eq!(first.job.key, "build");
    assert_eq!(store.task_by_idempotency_key(runner, "k1").await.unwrap().map(|task| task.id), Some(first.task.id));
    assert!(store.replace_task_token(first.task.id, &[71; 32]).await.unwrap());
    assert_eq!(store.running_task_by_token(&[70; 32]).await.unwrap(), None, "the old token is replaced");
    let (task, job) = store.running_task_by_token(&[71; 32]).await.unwrap().unwrap();
    assert_eq!((task.id, job.id), (first.task.id, first.job.id));

    assert!(store.heartbeat(first.task.id, Timestamp(NOW.0 + 20)).await.unwrap());
    store
        .put_steps(
            first.task.id,
            &[StepUpdate {
                number: 0,
                name: "Checkout",
                status: Status::Success,
                started_at: Some(NOW),
                stopped_at: Some(NOW),
                log_index: 0,
                log_length: 3,
            }],
        )
        .await
        .unwrap();
    store
        .put_steps(
            first.task.id,
            &[StepUpdate {
                number: 0,
                name: "Checkout",
                status: Status::Success,
                started_at: Some(NOW),
                stopped_at: Some(NOW),
                log_index: 0,
                log_length: 4,
            }],
        )
        .await
        .unwrap();
    assert_eq!(store.steps(first.task.id).await.unwrap()[0].log_length, 4);
    // Actions: integration (I).
    assert_eq!(
        store.steps_of_tasks(&[first.task.id, rbit_core::actions::TaskId(999)]).await.unwrap(),
        store.steps(first.task.id).await.unwrap()
    );
    assert_eq!(store.steps_of_tasks(&[]).await.unwrap(), []);
    store.set_log_progress(first.task.id, 4, 120, false).await.unwrap();
    assert_eq!(store.add_outputs(first.job.id, &[("version", "1.2"), ("empty", "")], 1024).await.unwrap(), ["empty", "version"]);
    assert_eq!(
        store.add_outputs(first.job.id, &[("version", "9.9"), ("big", &"x".repeat(2000))], 1024).await.unwrap(),
        ["empty", "version"]
    );
    assert_eq!(store.outputs(first.job.id).await.unwrap(), [("empty".to_owned(), String::new()), ("version".to_owned(), "1.2".to_owned())]);

    let settled = store.finish_task(first.task.id, Status::Failure, Timestamp(NOW.0 + 30)).await.unwrap().unwrap();
    assert_eq!(settled.run_status, Status::Waiting, "lint still waits");
    let test_job = store.jobs(run.id).await.unwrap().into_iter().find(|job| job.key == "test").unwrap();
    assert!(settled.now_waiting.is_empty() && settled.to_decide.is_empty());
    assert_eq!(test_job.status, Status::Skipped, "without its own if:, a job whose need failed is skipped (success())");
    assert_eq!(store.finish_task(first.task.id, Status::Success, NOW).await.unwrap(), None, "already finished");
    assert_eq!(store.running_task_by_token(&[71; 32]).await.unwrap(), None, "the token died with the task");
    assert_eq!(store.task(first.task.id).await.unwrap().unwrap().stopped_at, Some(Timestamp(NOW.0 + 30)));

    let next = store.claim(&claim(runner, scope, &labels, &[72; 32])).await.unwrap().unwrap();
    assert_eq!(next.job.key, "lint");
    store.finish_task(next.task.id, Status::Success, Timestamp(NOW.0 + 40)).await.unwrap().unwrap();
    assert!(store.claim(&claim(runner, scope, &labels, &[73; 32])).await.unwrap().is_none(), "nothing is left to run");
    let finished = store.run(run.id).await.unwrap().unwrap();
    assert_eq!((finished.status, finished.finished_at), (Status::Failure, Some(Timestamp(NOW.0 + 40))));
    assert!(store.finish_task(first.task.id, Status::Running, NOW).await.is_err(), "a result must be final");
}

// Actions: join (J).
#[tokio::test]
async fn jobs_with_their_own_if_wait_for_the_servers_decision() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let (none, needs_build, linux) = (Vec::<String>::new(), strings(&["build"]), strings(&["linux"]));
    let jobs = [
        job("build", &none, &linux),
        NewJob { condition: true, ..job("report", &needs_build, &linux) },
        NewJob { condition: true, ..job("notify", &none, &linux) },
        job("deploy", &needs_build, &linux),
    ];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let key = |key: &str| key.to_owned();
    let status_of = |jobs: &[rbit_core::actions::Job], wanted: &str| jobs.iter().find(|job| job.key == wanted).unwrap().status;

    // A condition with no needs is decided at once; until then no runner may take it.
    let undecided = store.undecided(10).await.unwrap();
    let all = store.jobs(run.id).await.unwrap();
    let notify = all.iter().find(|job| job.key == "notify").unwrap().id;
    assert_eq!(undecided, [notify]);
    let first = store.claim(&claim(runner, scope, &labels, &[90; 32])).await.unwrap().unwrap();
    assert_eq!(first.job.key, key("build"), "the undecided job is never claimed");
    assert!(store.claim(&claim(runner, scope, &labels, &[91; 32])).await.unwrap().is_none());
    let settled = store.decide(notify, Decision::Skip, NOW).await.unwrap().unwrap();
    assert!(settled.now_waiting.is_empty(), "a skipped job waits for nothing");
    assert_eq!(store.decide(notify, Decision::Run, NOW).await.unwrap(), None, "decided once");

    // `build` fails: `deploy` follows success() and is skipped; `report` is the server's call.
    let settled = store.finish_task(first.task.id, Status::Failure, NOW).await.unwrap().unwrap();
    let report = store.jobs(run.id).await.unwrap().into_iter().find(|job| job.key == "report").unwrap().id;
    assert_eq!(settled.to_decide, [report]);
    let jobs_now = store.jobs(run.id).await.unwrap();
    assert_eq!((status_of(&jobs_now, "deploy"), status_of(&jobs_now, "report")), (Status::Skipped, Status::Blocked));
    let settled = store.decide(report, Decision::Run, NOW).await.unwrap().unwrap();
    assert_eq!(settled.now_waiting, [report], "always(): it runs after a failure");
    let next = store.claim(&claim(runner, scope, &labels, &[92; 32])).await.unwrap().unwrap();
    assert_eq!(next.job.key, key("report"));
    assert!(store.undecided(10).await.unwrap().is_empty(), "every condition is decided");
}

#[tokio::test]
async fn max_parallel_and_job_groups_gate_claims_and_fail_fast_cancels() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let (none, linux) = (Vec::<String>::new(), strings(&["linux"]));
    let jobs = [
        NewJob { max_parallel: Some(1), ..job("matrix", &none, &linux) },
        NewJob { max_parallel: Some(1), ..job("matrix", &none, &linux) },
        NewJob { group: Some("deploy"), ..job("one", &none, &linux) },
        NewJob { group: Some("deploy"), ..job("two", &none, &linux) },
    ];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let mut claimed = Vec::new();
    for seed in 100..110u8 {
        match store.claim(&claim(runner, scope, &labels, &[seed; 32])).await.unwrap() {
            Some(next) => claimed.push(next),
            None => break,
        }
    }
    let keys: Vec<&str> = claimed.iter().map(|next| next.job.key.as_str()).collect();
    assert_eq!(keys, ["matrix", "one"], "one matrix job at a time, one job of the group at a time");

    let siblings: Vec<JobId> = store.jobs(run.id).await.unwrap().into_iter().filter(|job| job.key == "matrix").map(|job| job.id).collect();
    store.finish_task(claimed[0].task.id, Status::Failure, NOW).await.unwrap().unwrap();
    let settled = store.cancel_jobs(run.id, &siblings, NOW).await.unwrap().unwrap();
    assert!(settled.now_waiting.is_empty(), "cancelling makes nothing wait");
    let statuses: Vec<Status> =
        store.jobs(run.id).await.unwrap().into_iter().filter(|job| job.key == "matrix").map(|job| job.status).collect();
    assert_eq!(statuses, [Status::Failure, Status::Cancelled], "only the unfinished sibling is cancelled");
    assert_eq!(store.cancel_jobs(run.id, &siblings, NOW).await.unwrap(), None, "nothing left to cancel");

    store.finish_task(claimed[1].task.id, Status::Success, NOW).await.unwrap().unwrap();
    let next = store.claim(&claim(runner, scope, &labels, &[120; 32])).await.unwrap().unwrap();
    assert_eq!(next.job.key, "two", "the group's next job runs once the first finished");
}

#[tokio::test]
async fn cancelling_a_running_sibling_kills_its_task_token() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let (none, linux) = (Vec::<String>::new(), strings(&["linux"]));
    let jobs = [job("matrix", &none, &linux), job("matrix", &none, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let first = store.claim(&claim(runner, scope, &labels, &[130; 32])).await.unwrap().unwrap();
    let second = store.claim(&claim(runner, scope, &labels, &[131; 32])).await.unwrap().unwrap();
    assert!(store.running_task_by_token(&[131; 32]).await.unwrap().is_some());
    store.finish_task(first.task.id, Status::Failure, NOW).await.unwrap().unwrap();
    store.cancel_jobs(run.id, &[second.job.id], NOW).await.unwrap().unwrap();
    assert_eq!(store.running_task_by_token(&[131; 32]).await.unwrap(), None, "the cancelled sibling's token is dead at once");
    assert_eq!(store.job(second.job.id).await.unwrap().unwrap().status, Status::Cancelled);
}

#[tokio::test]
async fn request_keys_are_unique_per_runner() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let jobs = [job("a", &none, &linux), job("b", &none, &linux), job("c", &none, &linux)];
    store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (one, labels) = fx.runner(scope, &["linux"], 1).await;
    let (two, _) = fx.runner(scope, &["linux"], 2).await;
    let keyed = |runner, digest| ClaimRequest { idempotency_key: Some("same"), ..claim(runner, scope, &labels, digest) };
    assert!(store.claim(&keyed(one, &[80; 32])).await.unwrap().is_some());
    assert!(store.claim(&keyed(two, &[81; 32])).await.unwrap().is_some(), "another runner may use the same key");
    assert!(matches!(store.claim(&keyed(one, &[82; 32])).await, Err(StorageError::Conflict(_))));
    assert_eq!(store.count_waiting(fx.rocket).await.unwrap(), 1, "the refused claim changed nothing");
}

#[tokio::test]
async fn cancelling_stops_tokens_at_once_and_keeps_the_job_cancelled() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let a = strings(&["a"]);
    let jobs = [job("a", &none, &linux), job("b", &a, &linux), job("c", &none, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let running = store.claim(&claim(runner, scope, &labels, &[90; 32])).await.unwrap().unwrap();
    assert_eq!(store.cancel_run(fx.probe, run.id, NOW).await.unwrap(), None, "another repository");
    let settled = store.cancel_run(fx.rocket, run.id, Timestamp(NOW.0 + 50)).await.unwrap().unwrap();
    assert_eq!(settled.run_status, Status::Cancelled);
    assert!(store.jobs(run.id).await.unwrap().iter().all(|job| job.status == Status::Cancelled));
    assert_eq!(store.running_task_by_token(&[90; 32]).await.unwrap(), None, "the token stops working before the runner knows");
    assert_eq!(store.task(running.task.id).await.unwrap().unwrap().status, Status::Running, "the runner learns on its next update");
    store.finish_task(running.task.id, Status::Success, Timestamp(NOW.0 + 60)).await.unwrap().unwrap();
    assert_eq!(store.job(running.job.id).await.unwrap().unwrap().status, Status::Cancelled);
    assert_eq!(store.task(running.task.id).await.unwrap().unwrap().status, Status::Success);
    assert_eq!(store.cancel_run(fx.rocket, run.id, NOW).await.unwrap(), None, "already finished");
}

#[tokio::test]
async fn a_task_left_behind_by_a_rerun_never_settles_the_new_attempt() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let run = store.create_run(&new_run(fx.rocket, &[job("a", &none, &linux)], RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let old = store.claim(&claim(runner, scope, &labels, &[140; 32])).await.unwrap().unwrap();
    store.cancel_run(fx.rocket, run.id, NOW).await.unwrap().unwrap();
    // The cancelled run is finished while its task still runs; a rerun starts at once.
    store.rerun(fx.rocket, run.id, Rerun::All, RunStart::Runnable, NOW).await.unwrap().unwrap();
    let new = store.claim(&claim(runner, scope, &labels, &[141; 32])).await.unwrap().unwrap();
    assert_eq!(new.job.id, old.job.id);

    store.finish_task(old.task.id, Status::Failure, Timestamp(NOW.0 + 10)).await.unwrap().unwrap();
    assert_eq!(store.task(old.task.id).await.unwrap().unwrap().status, Status::Failure);
    let job = store.job(old.job.id).await.unwrap().unwrap();
    assert_eq!((job.status, job.finished_at, job.task_id), (Status::Running, None, Some(new.task.id)), "the new attempt still runs");
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Running);

    store.finish_task(new.task.id, Status::Success, Timestamp(NOW.0 + 20)).await.unwrap().unwrap();
    assert_eq!(store.job(old.job.id).await.unwrap().unwrap().status, Status::Success);
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Success);
}

#[tokio::test]
async fn awaiting_and_held_runs_start_only_when_released() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let a = strings(&["a"]);
    let jobs = [job("a", &none, &linux), job("b", &a, &linux)];
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;

    let awaiting = store.create_run(&new_run(fx.rocket, &jobs, RunStart::AwaitingApproval)).await.unwrap();
    assert_eq!(awaiting.status, Status::AwaitingApproval);
    assert_eq!(statuses(&store.jobs(awaiting.id).await.unwrap()), [("a", Status::AwaitingApproval), ("b", Status::AwaitingApproval)]);
    assert_eq!(store.claim(&claim(runner, scope, &labels, &[91; 32])).await.unwrap(), None);
    let held = store.approve_run(fx.rocket, awaiting.id, fx.alice, true, NOW).await.unwrap().unwrap();
    assert_eq!(held.run_status, Status::Blocked, "approved but its concurrency group is busy");
    assert_eq!(store.claim(&claim(runner, scope, &labels, &[91; 32])).await.unwrap(), None);
    assert_eq!(store.approve_run(fx.rocket, awaiting.id, fx.alice, false, NOW).await.unwrap(), None, "approved once");
    let released = store.release_run(fx.rocket, awaiting.id, NOW).await.unwrap().unwrap();
    assert_eq!(released.run_status, Status::Waiting);
    assert_eq!(statuses(&store.jobs(awaiting.id).await.unwrap()), [("a", Status::Waiting), ("b", Status::Blocked)]);
    let run = store.run(awaiting.id).await.unwrap().unwrap();
    assert_eq!((run.approved_by, run.approved_at), (Some(fx.alice), Some(NOW)));

    let concurrent = store.create_run(&new_run(fx.rocket, &jobs, RunStart::HeldForConcurrency)).await.unwrap();
    assert_eq!(concurrent.status, Status::Blocked);
    assert_eq!(store.approve_run(fx.rocket, concurrent.id, fx.alice, false, NOW).await.unwrap(), None, "nothing to approve");
    let group = NewRun { concurrency_group: Some("ci-main"), ..new_run(fx.rocket, &jobs, RunStart::Runnable) };
    let grouped = store.create_run(&group).await.unwrap();
    assert_eq!(store.unfinished_in_group(fx.rocket, "ci-main").await.unwrap().iter().map(|run| run.id).collect::<Vec<_>>(), [grouped.id]);
    let direct = store.approve_run(fx.rocket, awaiting.id, fx.alice, false, NOW).await.unwrap();
    assert_eq!(direct, None);
    let awaiting_again = store.create_run(&new_run(fx.rocket, &jobs, RunStart::AwaitingApproval)).await.unwrap();
    let started = store.approve_run(fx.rocket, awaiting_again.id, fx.bob, false, NOW).await.unwrap().unwrap();
    assert_eq!(started.run_status, Status::Waiting);
    assert_eq!(started.now_waiting.len(), 1);
}

#[tokio::test]
async fn reruns_restart_failed_jobs_and_their_dependents() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let a = strings(&["a"]);
    let b = strings(&["b"]);
    let jobs = [job("a", &none, &linux), job("b", &a, &linux), job("c", &b, &linux), job("d", &none, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    assert_eq!(store.rerun(fx.rocket, run.id, Rerun::All, RunStart::Runnable, NOW).await.unwrap(), None, "still running");
    // `c` needs the failing `b`, so it is skipped rather than claimed.
    let mut seed = 100u8;
    while let Some(claimed) = store.claim(&claim(runner, scope, &labels, &[seed; 32])).await.unwrap() {
        seed += 1;
        let result = if claimed.job.key == "b" { Status::Failure } else { Status::Success };
        store.add_outputs(claimed.job.id, &[("out", "1")], 1024).await.unwrap();
        store.finish_task(claimed.task.id, result, NOW).await.unwrap().unwrap();
    }
    assert_eq!(seed, 103, "a, b and d ran");
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Failure);
    let settled = store.rerun(fx.rocket, run.id, Rerun::Failed, RunStart::Runnable, Timestamp(NOW.0 + 100)).await.unwrap().unwrap();
    assert_eq!(settled.run_status, Status::Waiting);
    let after = store.jobs(run.id).await.unwrap();
    assert_eq!(statuses(&after), [("a", Status::Success), ("b", Status::Waiting), ("c", Status::Blocked), ("d", Status::Success)]);
    let attempts: Vec<_> = after.iter().map(|job| job.attempt).collect();
    assert_eq!(attempts, [1, 2, 2, 1]);
    assert!(after[1].task_id.is_none() && after[1].started_at.is_none());
    assert!(store.outputs(after[1].id).await.unwrap().is_empty() && !store.outputs(after[0].id).await.unwrap().is_empty());
    let run_after = store.run(run.id).await.unwrap().unwrap();
    assert_eq!((run_after.attempt, run_after.finished_at), (2, None));
    let next = store.claim(&claim(runner, scope, &labels, &[110; 32])).await.unwrap().unwrap();
    assert_eq!((next.job.key.as_str(), next.task.attempt), ("b", 2));
    store.finish_task(next.task.id, Status::Success, NOW).await.unwrap();
    let last = store.claim(&claim(runner, scope, &labels, &[111; 32])).await.unwrap().unwrap();
    store.finish_task(last.task.id, Status::Success, NOW).await.unwrap();
    assert_eq!(store.run(run.id).await.unwrap().unwrap().status, Status::Success);

    let one = store.rerun(fx.rocket, run.id, Rerun::Job(after[3].id), RunStart::AwaitingApproval, NOW).await.unwrap().unwrap();
    assert_eq!(one.run_status, Status::AwaitingApproval, "a re-run of an untrusted run waits again");
    assert_eq!(store.rerun(fx.probe, run.id, Rerun::All, RunStart::Runnable, NOW).await.unwrap(), None);
}

#[tokio::test]
async fn reruns_restart_only_the_chosen_matrix_instances() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let test = strings(&["test"]);
    // Two instances of one matrix job, and a job that needs them.
    let jobs = [job("test", &none, &linux), job("test", &none, &linux), job("report", &test, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let mut seed = 100u8;
    let mut failed_first = false;
    while let Some(claimed) = store.claim(&claim(runner, scope, &labels, &[seed; 32])).await.unwrap() {
        seed += 1;
        let result = if claimed.job.key == "test" && !failed_first { Status::Failure } else { Status::Success };
        failed_first |= result == Status::Failure;
        store.finish_task(claimed.task.id, result, NOW).await.unwrap();
    }
    let before = store.jobs(run.id).await.unwrap();
    assert_eq!(statuses(&before), [("test", Status::Failure), ("test", Status::Success), ("report", Status::Skipped)]);

    store.rerun(fx.rocket, run.id, Rerun::Failed, RunStart::Runnable, NOW).await.unwrap().unwrap();
    let after = store.jobs(run.id).await.unwrap();
    assert_eq!(statuses(&after), [("test", Status::Waiting), ("test", Status::Success), ("report", Status::Blocked)]);
    assert_eq!(after.iter().map(|job| job.attempt).collect::<Vec<_>>(), [2, 1, 2]);
    let next = store.claim(&claim(runner, scope, &labels, &[120; 32])).await.unwrap().unwrap();
    assert_eq!(next.job.id, after[0].id);
    store.finish_task(next.task.id, Status::Success, NOW).await.unwrap();
    let report = store.claim(&claim(runner, scope, &labels, &[121; 32])).await.unwrap().unwrap();
    assert_eq!(report.job.key, "report", "the job that needs them runs once every instance is done");
}

#[tokio::test]
async fn sweeps_find_silent_endless_and_abandoned_work() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_runs();
    let none: Vec<String> = Vec::new();
    let linux = strings(&["linux"]);
    let jobs = [job("a", &none, &linux), job("b", &none, &linux)];
    let run = store.create_run(&new_run(fx.rocket, &jobs, RunStart::Runnable)).await.unwrap();
    let scope = RunnerScope::Repository(fx.rocket);
    let (runner, labels) = fx.runner(scope, &["linux"], 1).await;
    let claimed = store.claim(&claim(runner, scope, &labels, &[120; 32])).await.unwrap().unwrap();
    assert_eq!(store.silent_tasks(Timestamp(NOW.0 + 10)).await.unwrap(), []);
    assert_eq!(store.silent_tasks(Timestamp(NOW.0 + 11)).await.unwrap(), [claimed.task.id]);
    assert_eq!(store.long_running_tasks(Timestamp(NOW.0 + 11)).await.unwrap(), [claimed.task.id]);
    let settled = store.cancel_abandoned(Timestamp(NOW.0 + 1), Timestamp(NOW.0 + 2)).await.unwrap();
    assert_eq!(settled.len(), 1);
    assert_eq!(statuses(&store.jobs(run.id).await.unwrap()), [("a", Status::Running), ("b", Status::Cancelled)]);
    assert_eq!(store.count_all_waiting().await.unwrap(), 0);
    store.finish_task(claimed.task.id, Status::Failure, Timestamp(NOW.0 + 3)).await.unwrap();
    let with_logs = store.runs_with_logs(Timestamp(NOW.0 + 4), 10).await.unwrap();
    assert_eq!(with_logs, [(run.id, vec![claimed.task.id])]);
    store.expire_logs(run.id, Timestamp(NOW.0 + 5)).await.unwrap();
    assert_eq!(store.run(run.id).await.unwrap().unwrap().logs_expired_at, Some(Timestamp(NOW.0 + 5)));
    assert_eq!(store.task(claimed.task.id).await.unwrap().unwrap().log_state, rbit_core::actions::LogState::Expired);
    assert_eq!(store.runs_with_logs(Timestamp(NOW.0 + 4), 10).await.unwrap(), []);
}

fn seal(byte: u8) -> impl FnOnce(rbit_core::actions::SecretId) -> SealedSecret {
    move |id| SealedSecret { key_id: [byte; 8], nonce: [u8::try_from(id.0).unwrap(); 12], ciphertext: vec![byte; 20] }
}

#[tokio::test]
async fn secrets_are_sealed_with_their_id_and_scoped() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_secrets();
    let token: SecretName = "token".parse().unwrap();
    let repo_scope = SecretScope::Repository(fx.rocket);
    let owner_scope = SecretScope::Owner(fx.alice);
    assert!(!store.any_secret().await.unwrap());
    let created = store.put_secret(repo_scope, &token, SecretVisibility::Selected, fx.alice, NOW, seal(1)).await.unwrap();
    assert_eq!((created.name.as_str(), created.visibility, created.key_id), ("TOKEN", SecretVisibility::All, [1; 8]));
    let replaced = store.put_secret(repo_scope, &token, SecretVisibility::All, fx.bob, Timestamp(NOW.0 + 1), seal(2)).await.unwrap();
    assert_eq!((replaced.id, replaced.key_id, replaced.updated_by, replaced.created_at), (created.id, [2; 8], Some(fx.bob), NOW));
    store.put_secret(owner_scope, &token, SecretVisibility::All, fx.alice, NOW, seal(3)).await.unwrap();
    let only_selected: SecretName = "deploy".parse().unwrap();
    let deploy = store.put_secret(owner_scope, &only_selected, SecretVisibility::Selected, fx.alice, NOW, seal(4)).await.unwrap();
    assert!(store.set_secret_repositories(fx.alice, &only_selected, &[fx.probe, fx.other]).await.unwrap());
    assert_eq!(store.secret_repositories(deploy.id).await.unwrap(), [fx.probe], "another owner's repository is ignored");
    assert!(!store.set_secret_repositories(fx.bob, &only_selected, &[]).await.unwrap());

    let for_rocket = store.secrets_for_task(fx.rocket, fx.alice).await.unwrap();
    let names: Vec<_> = for_rocket.iter().map(|secret| (secret.info.scope, secret.info.name.as_str())).collect();
    assert_eq!(names, [(repo_scope, "TOKEN"), (owner_scope, "TOKEN")], "repository secrets first; deploy is not given to rocket");
    assert_eq!(for_rocket[0].sealed.ciphertext, vec![2; 20]);
    assert_eq!(for_rocket[0].sealed.nonce, [u8::try_from(created.id.0).unwrap(); 12], "sealed for its own id");
    let for_probe: Vec<_> =
        store.secrets_for_task(fx.probe, fx.alice).await.unwrap().into_iter().map(|s| s.info.name.to_string()).collect();
    assert_eq!(for_probe, ["DEPLOY", "TOKEN"]);
    assert_eq!(store.secrets_for_task(fx.other, fx.bob).await.unwrap(), []);
    assert_eq!(store.secrets(repo_scope).await.unwrap().len(), 1);
    assert_eq!(store.count_secrets(owner_scope).await.unwrap(), 2);
    assert!(store.delete_secret(repo_scope, &token).await.unwrap());
    assert!(!store.delete_secret(SecretScope::Repository(fx.probe), &only_selected).await.unwrap());
    assert!(store.any_secret().await.unwrap());
}

#[tokio::test]
async fn variables_are_scoped_and_repository_values_come_first() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_secrets();
    let name: VariableName = "region".parse().unwrap();
    let repo_scope = SecretScope::Repository(fx.rocket);
    store.put_variable(SecretScope::Owner(fx.alice), &name, "eu", fx.alice, NOW).await.unwrap();
    let created = store.put_variable(repo_scope, &name, "us", fx.alice, NOW).await.unwrap();
    let updated = store.put_variable(repo_scope, &name, "ap", fx.bob, Timestamp(NOW.0 + 1)).await.unwrap();
    assert_eq!((updated.id, updated.value.as_str(), updated.created_at, updated.updated_by), (created.id, "ap", NOW, Some(fx.bob)));
    let for_task: Vec<_> = store.variables_for_task(fx.rocket, fx.alice).await.unwrap().into_iter().map(|v| v.value).collect();
    assert_eq!(for_task, ["ap", "eu"]);
    assert_eq!(store.count_variables(repo_scope).await.unwrap(), 1);
    assert_eq!(store.variables(SecretScope::Owner(fx.alice)).await.unwrap()[0].value, "eu");
    assert!(store.delete_variable(repo_scope, &name).await.unwrap());
    assert_eq!(store.variables_for_task(fx.other, fx.bob).await.unwrap(), []);
}

#[tokio::test]
async fn schedules_are_replaced_ticked_and_paused() {
    let fx = Fixture::new().await;
    let store = fx.db.actions_schedules();
    let entries = [
        NewSchedule { workflow_path: "a.yml", cron: "0 * * * *", next_at: Timestamp(100) },
        NewSchedule { workflow_path: "b.yml", cron: "@daily", next_at: Timestamp(200) },
    ];
    store.replace(fx.rocket, &entries).await.unwrap();
    assert_eq!(store.next_due().await.unwrap(), Some(Timestamp(100)));
    assert_eq!(store.due(Timestamp(150), 10).await.unwrap().len(), 1);
    for _ in 0..2 {
        store.record_tick(fx.rocket, "a.yml", "0 * * * *", Timestamp(300), true, 3).await.unwrap();
    }
    assert_eq!(store.list(fx.rocket).await.unwrap()[0].failures, 2);
    // Replacing keeps an entry's failures; dropped entries go.
    store.replace(fx.rocket, &entries[..1]).await.unwrap();
    let kept = store.list(fx.rocket).await.unwrap();
    assert_eq!((kept.len(), kept[0].failures, kept[0].next_at), (1, 2, Timestamp(100)));
    store.record_tick(fx.rocket, "a.yml", "0 * * * *", Timestamp(400), true, 3).await.unwrap();
    assert!(store.list(fx.rocket).await.unwrap()[0].paused, "three failures in a row pause it");
    assert_eq!(store.next_due().await.unwrap(), None);
    store.set_paused(fx.rocket, false).await.unwrap();
    let resumed = &store.list(fx.rocket).await.unwrap()[0];
    assert!(!resumed.paused && resumed.failures == 0);
    store.record_tick(fx.rocket, "a.yml", "0 * * * *", Timestamp(500), false, 3).await.unwrap();
    assert_eq!(store.due(Timestamp(500), 10).await.unwrap()[0].next_at, Timestamp(500));
}
