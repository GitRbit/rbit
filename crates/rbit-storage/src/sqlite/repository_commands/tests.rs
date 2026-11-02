//! Real SQLite contract tests; no backend/driver-stage or crash proof is fabricated.
use super::super::{orgs::NewOrganization, rows::RepositoryRow};
use super::*;
use crate::{Database, NewUser};
use rbit_core::{OrgRole, Permission, Visibility};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(2);
struct Fixture {
    directory: tempfile::TempDir,
    db: Database,
    alice: AccountId,
    bob: AccountId,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_commit_fault(false).await
    }
    async fn with_commit_fault(fault: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("db.sqlite")).await.unwrap();
        if fault {
            // Fixed fault schema BEFORE any store fixture activity; no late DDL/drop trigger.
            for statement in [
                "CREATE TABLE fixture_gate (enabled INTEGER NOT NULL)",
                "INSERT INTO fixture_gate VALUES (0)",
                "CREATE TABLE fixture_deferred (repo_id INTEGER REFERENCES repositories(id) ON DELETE CASCADE, missing INTEGER REFERENCES accounts(id) DEFERRABLE INITIALLY DEFERRED)",
                "CREATE TRIGGER fixture_commit AFTER INSERT ON repositories WHEN (SELECT enabled FROM fixture_gate) = 1 BEGIN INSERT INTO fixture_deferred VALUES (NEW.id, -999_999); END",
                "CREATE TRIGGER fixture_delete BEFORE DELETE ON repositories WHEN (SELECT enabled FROM fixture_gate) = 2 AND OLD.name = 'keep-row' BEGIN SELECT RAISE(ABORT, 'injected compensation statement refusal'); END",
                "CREATE TRIGGER fixture_rollback BEFORE UPDATE OF description ON repositories WHEN (SELECT enabled FROM fixture_gate) = 3 BEGIN SELECT RAISE(ROLLBACK, 'injected outer rollback'); END",
                "CREATE TRIGGER fixture_creator BEFORE INSERT ON collaborators WHEN (SELECT enabled FROM fixture_gate) = 4 BEGIN SELECT RAISE(ABORT, 'injected creator grant refusal'); END",
            ] {
                sqlx::query(statement).execute(&db.pool).await.unwrap();
            }
        }
        let mut users = Vec::new();
        for name in ["alice", "bob"] {
            users.push(
                db.accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "fixture".into(),
                        is_admin: true,
                    })
                    .await
                    .unwrap()
                    .id(),
            );
        }
        Self { directory, db, alice: users[0], bob: users[1] }
    }
    fn new_repo(&self, name: &str) -> NewRepository {
        NewRepository {
            owner_id: self.alice,
            name: name.parse().unwrap(),
            description: Some("original".into()),
            visibility: Visibility::Private,
            default_branch: "main".into(),
        }
    }
    async fn admit(&self, plan: RepositoryPlan) -> AdmittedRepositoryCommand {
        self.db.repository_commands().reserve(plan).unwrap().acquire().await.unwrap()
    }
    async fn create(&self, name: &str) -> RepoId {
        let AdmittedRepositoryCommand { mut command, owner } = self.admit(RepositoryPlan::Create(self.new_repo(name))).await;
        let report = settled(command.execute_next().await);
        let RepositoryOutput::Created(repo) = report.result.unwrap() else { panic!("missing created output") };
        assert_eq!(report.repository_id, Some(repo.id));
        assert_eq!(command.repository_id(), Some(repo.id));
        assert_eq!(
            command.finish_after_successful_backend(owner).await.unwrap().logical,
            RepositoryLogicalSettlement::EnrollmentAcknowledged
        );
        repo.id
    }
}
fn settled(result: RepositoryPhaseResult) -> RepositoryPhaseReport {
    match result {
        RepositoryPhaseResult::Settled(report) => report,
        other @ (RepositoryPhaseResult::Unconfirmed { .. } | RepositoryPhaseResult::RejectedName(_)) => {
            panic!("unexpected phase: {other:?}")
        }
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}
fn settings() -> RepoGeneralSettings {
    RepoGeneralSettings { description: Some("changed".into()), visibility: Visibility::Public, default_branch: "next".into() }
}

#[tokio::test]
async fn settings_sequence_and_both_delete_aliases_preserve_rows_redirects_and_cascades() {
    for via in [RepositoryDeleteSource::Repositories, RepositoryDeleteSource::Settings] {
        let f = Fixture::new().await;
        let id = f.create("original").await;
        let AdmittedRepositoryCommand { mut command, owner } = f
            .admit(RepositoryPlan::Settings(RepositorySettingsPlan {
                repo: id,
                general: Some(settings()),
                rename: Some("renamed".parse().unwrap()),
                archival: Some(RepositoryArchiveChange::Archive { by: f.alice, at: Timestamp(100) }),
            }))
            .await;
        for _ in 0..3 {
            assert_eq!(settled(command.execute_next().await).boundary, RepositorySqlBoundary::Committed);
        }
        let completion = command.finish_after_successful_backend(owner).await.unwrap();
        assert_eq!(completion.progress.committed_forward_steps, 3);
        assert_eq!(completion.logical, RepositoryLogicalSettlement::EnrollmentAcknowledged);
        let row = f.db.repositories().find_by_path("alice", "renamed").await.unwrap().unwrap().repo;
        assert_eq!(row.id, id);
        assert_eq!(row.visibility, Visibility::Public);
        assert_eq!(row.default_branch, "next");
        assert_eq!(row.description.as_deref(), Some("changed"));
        assert_eq!(row.archived_at, Some(Timestamp(100)));
        assert_eq!(f.db.repositories().find_by_old_path("alice", "original").await.unwrap().unwrap().repo.id, id);
        for _ in 0..2 {
            let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Delete { repo: id, via }).await;
            assert!(matches!(settled(command.execute_next().await).result, Ok(RepositoryOutput::Unit)));
            command.finish_after_successful_backend(owner).await.unwrap();
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM repo_redirects WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
        assert_eq!(count, 0);
        assert!(f.db.repositories().find_by_path("alice", "renamed").await.unwrap().is_none());
    }
}

#[tokio::test]
async fn standalone_general_rename_archive_and_unarchive_keep_original_noops_and_actor_time() {
    let f = Fixture::new().await;
    let id = f.create("repo").await;
    for plan in
        [RepositoryPlan::General { repo: id, settings: settings() }, RepositoryPlan::Rename { repo: id, name: "REPO".parse().unwrap() }]
    {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
        settled(command.execute_next().await).result.unwrap();
        command.finish_after_successful_backend(owner).await.unwrap();
    }
    for (by, at, expected) in [(f.alice, Timestamp(100), true), (f.bob, Timestamp(200), false)] {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Archive { repo: id, by, at }).await;
        assert!(matches!(settled(command.execute_next().await).result, Ok(RepositoryOutput::Changed(changed)) if changed == expected));
        command.finish_after_successful_backend(owner).await.unwrap();
    }
    let attribution: (i64, i64) = sqlx::query_as("SELECT archived_by, archived_at FROM repo_archives WHERE repo_id = ?")
        .bind(id.0)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(attribution, (f.alice.0, 100));
    for expected in [true, false] {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Unarchive { repo: id }).await;
        assert!(matches!(settled(command.execute_next().await).result, Ok(RepositoryOutput::Changed(changed)) if changed == expected));
        command.finish_after_successful_backend(owner).await.unwrap();
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repo_redirects WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0, "case-only rename does not create a redirect");
}

#[tokio::test]
async fn transfer_keeps_identity_and_unrelated_grant_while_removing_old_team_and_new_owner_grant() {
    let f = Fixture::new().await;
    let org =
        f.db.orgs()
            .create(NewOrganization { name: "acme".parse().unwrap(), display_name: None, description: None, creator: f.alice })
            .await
            .unwrap()
            .account
            .id;
    f.db.orgs().add_member(org, f.bob, OrgRole::Member).await.unwrap();
    let team = f.db.teams().create(org, &"core".parse().unwrap(), None).await.unwrap().id;
    let mut input = f.new_repo("team-repo");
    input.owner_id = org;
    let id = f.db.repositories().create(input).await.unwrap().id;
    f.db.teams().set_repository(team, id, Permission::Write).await.unwrap();
    for user in [f.alice, f.bob] {
        f.db.collaborators().add(id, user, Permission::Admin, f.alice).await.unwrap();
    }
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Transfer { repo: id, new_owner: f.alice }).await;
    settled(command.execute_next().await).result.unwrap();
    command.finish_after_successful_backend(owner).await.unwrap();
    let row = f.db.repositories().find_by_path("alice", "team-repo").await.unwrap().unwrap();
    assert_eq!(row.repo.id, id);
    assert_eq!(f.db.repositories().find_by_old_path("acme", "team-repo").await.unwrap().unwrap().repo.id, id);
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM team_repositories WHERE repo_id = ?1), (SELECT COUNT(*) FROM collaborators WHERE repo_id = ?1 AND user_id = ?2), (SELECT COUNT(*) FROM collaborators WHERE repo_id = ?1 AND user_id = ?3)")
        .bind(id.0).bind(f.alice.0).bind(f.bob.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(counts, (0, 0, 1));
}

#[tokio::test]
async fn three_duplicate_and_fk_failures_acknowledge_rollback_reuse_slots_and_stay_logical_failure() {
    let f = Fixture::new().await;
    f.create("taken").await;
    for missing_owner in [false, true, false, true, false, true] {
        let mut input = f.new_repo(if missing_owner { "missing-owner" } else { "taken" });
        if missing_owner {
            input.owner_id = AccountId(-999_999);
        }
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(input)).await;
        let report = settled(command.execute_next().await);
        assert_eq!(report.boundary, RepositorySqlBoundary::RolledBack);
        if missing_owner {
            assert!(matches!(report.result, Err(StorageError::Sqlite(sqlx::Error::Database(_)))));
        } else {
            assert!(matches!(report.result, Err(StorageError::Conflict("repository name"))));
        }
        assert_eq!(command.finish_after_successful_backend(owner).await.unwrap().logical, RepositoryLogicalSettlement::FailureRecorded);
    }
    assert!(f.db.authority_domain.snapshot().uncertain);
    f.create("still-usable").await;
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::FailureRecorded));
}

#[tokio::test]
async fn one_shot_compensation_uses_authoritative_create_id_and_records_original_failure() {
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("compensated"))).await;
    let report = settled(command.execute_next().await);
    let id = report.repository_id.unwrap();
    report.result.unwrap();
    assert!(matches!(settled(command.compensate_created().await).result, Ok(RepositoryOutput::Unit)));
    let completion = command.finish_failed_backend(owner).await.unwrap();
    assert_eq!(completion.progress.committed_forward_steps, 1);
    assert_eq!(completion.progress.create_compensation, Some(RepositorySqlBoundary::Committed));
    assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("no-retry"))).await;
    settled(command.execute_next().await).result.unwrap();
    settled(command.compensate_created().await).result.unwrap();
    assert!(matches!(command.compensate_created().await, RepositoryPhaseResult::Unconfirmed { original: StorageError::Invariant(_), .. }));
    assert!(command.finish_failed_backend(owner).await.is_err());
}

#[tokio::test]
async fn failed_backend_terminal_before_sql_reuses_credits_without_asserting_no_effects() {
    let f = Fixture::new().await;
    let path = f.directory.path().join("ordinary-file");
    std::fs::write(&path, "kept").unwrap();
    for _ in 0..3 {
        let AdmittedRepositoryCommand { command, owner } =
            f.admit(RepositoryPlan::Delete { repo: RepoId(-1), via: RepositoryDeleteSource::Settings }).await;
        // Actual synchronous ordinary filesystem error ended. This is not a Git lease/worker integration proof.
        assert_eq!(std::fs::remove_dir_all(&path).unwrap_err().kind(), std::io::ErrorKind::NotADirectory);
        let completion = command.finish_failed_backend(owner).await.unwrap();
        assert_eq!(completion.progress, RepositorySqlProgress::default());
        assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), "kept");
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn successful_backend_terminal_cannot_skip_initial_sql_or_successful_prefix() {
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("not-run"))).await;
    assert!(command.finish_after_successful_backend(owner).await.is_err());
    let id = f.create("prefix").await;
    let AdmittedRepositoryCommand { mut command, owner } = f
        .admit(RepositoryPlan::Settings(RepositorySettingsPlan {
            repo: id,
            general: Some(settings()),
            rename: Some("later".parse().unwrap()),
            archival: None,
        }))
        .await;
    settled(command.execute_next().await).result.unwrap();
    assert!(command.finish_after_successful_backend(owner).await.is_err());
    assert!(f.db.repositories().find_by_path("alice", "prefix").await.unwrap().is_some());
}

#[tokio::test]
async fn committed_prefix_then_rename_conflict_is_known_partial_not_reconciliation() {
    let f = Fixture::new().await;
    let id = f.create("prefix").await;
    f.create("taken").await;
    let AdmittedRepositoryCommand { mut command, owner } = f
        .admit(RepositoryPlan::Settings(RepositorySettingsPlan {
            repo: id,
            general: Some(settings()),
            rename: Some("taken".parse().unwrap()),
            archival: Some(RepositoryArchiveChange::Unarchive),
        }))
        .await;
    settled(command.execute_next().await).result.unwrap();
    let report = settled(command.execute_next().await);
    assert_eq!(report.boundary, RepositorySqlBoundary::RolledBack);
    assert!(matches!(report.result, Err(StorageError::Conflict("repository name"))));
    let completion = command.finish_after_successful_backend(owner).await.unwrap();
    assert_eq!(completion.progress.committed_forward_steps, 1);
    assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
    let row = f.db.repositories().find_by_path("alice", "prefix").await.unwrap().unwrap().repo;
    assert_eq!(row.default_branch, "next");
    assert_eq!(row.visibility, Visibility::Public);
    assert_eq!(row.archived_at, None);
}

#[tokio::test]
async fn shared_capacity_stale_stop_and_pure_retained_reservation_do_not_block_close() {
    let f = Fixture::new().await;
    let factory = f.db.repository_commands();
    let first = factory.reserve(RepositoryPlan::Create(f.new_repo("first"))).unwrap();
    let second = factory.clone().reserve(RepositoryPlan::Create(f.new_repo("second"))).unwrap();
    assert!(matches!(
        factory.reserve(RepositoryPlan::Create(f.new_repo("third"))),
        Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Capacity, .. })
    ));
    drop(second);
    let retained = factory.reserve(RepositoryPlan::Create(f.new_repo("retained"))).unwrap();
    drop(f.db.clone().close()); // Synchronous stop even without polling returned future.
    assert!(matches!(
        factory.reserve(RepositoryPlan::Create(f.new_repo("fresh-after-stop"))),
        Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Stopped, .. })
    ));
    assert!(matches!(first.acquire().await, Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Stopped, .. })));
    timeout(WAIT, f.db.close()).await.unwrap();
    drop(retained); // Count was never reset; ineligible is not all resources gone.
}

#[tokio::test]
async fn close_waits_acquired_suspended_command_and_allows_same_token_continuation() {
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("during-close"))).await;
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    // Other enrolled writer can proceed while command's same enrollment is suspended.
    f.db.accounts().rename(f.bob, &"renamed-bob".parse().unwrap()).await.unwrap();
    let db = f.db.clone();
    let mut close = Box::pin(db.close());
    pending(close.as_mut()).await;
    settled(command.execute_next().await).result.unwrap();
    command.finish_after_successful_backend(owner).await.unwrap();
    timeout(WAIT, close).await.unwrap();
    assert!(f.db.pool.is_closed());
}

#[tokio::test]
async fn deadline_does_not_release_acquired_command_and_expired_drain_is_deterministic() {
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("cancel"))).await;
    assert_eq!(f.db.drain_repository_commands(Instant::now()).await, Err(RepositoryCommandDrainError::Deadline));
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    command.cancel_before_effects(owner).await.unwrap();
    f.db.drain_repository_commands(Instant::now() + WAIT).await.unwrap();
}

#[tokio::test]
async fn pending_acquire_cancellation_is_known_no_work_and_shares_neutral_serial() {
    let f = Fixture::new().await;
    let quiescent = f.db.authority_domain.drain(Instant::now() + WAIT).await.unwrap();
    let mut acquire = Box::pin(f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("queued"))).unwrap().acquire());
    pending(acquire.as_mut()).await;
    assert_eq!(f.db.authority_domain.snapshot().queued, 1);
    drop(acquire);
    drop(quiescent);
    assert_eq!(f.db.authority_domain.snapshot().queued, 0);
    assert!(!f.db.authority_domain.snapshot().uncertain);
    f.create("later").await;
}

#[tokio::test]
async fn actual_phase_cancellation_stays_busy_after_owner_loss_until_future_dropped() {
    let f = Fixture::new().await;
    let id = f.create("locked").await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::General { repo: id, settings: settings() }).await;
    let mut blocker = f.db.pool.begin().await.unwrap();
    sqlx::query("UPDATE repositories SET description = description WHERE id = ?").bind(id.0).execute(&mut *blocker).await.unwrap();
    let mut phase = Box::pin(command.execute_next());
    pending(phase.as_mut()).await;
    drop(owner);
    let db = f.db.clone();
    let mut close = Box::pin(db.close());
    pending(close.as_mut()).await;
    // First poll proves phase enrollment/held-writer contention, not actual driver entry.
    drop(phase);
    drop(command);
    blocker.rollback().await.unwrap();
    timeout(WAIT, close).await.unwrap();
    assert!(f.db.authority_domain.snapshot().uncertain);
}

#[tokio::test]
async fn terminal_resume_cancel_and_unpolled_terminals_are_unknown_not_standalone_follower_ack() {
    let f = Fixture::new().await;
    let id = f.create("terminal").await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::General { repo: id, settings: settings() }).await;
    settled(command.execute_next().await).result.unwrap();
    let holder = f.db.authority_domain.writer().begin().await.unwrap();
    let mut terminal = Box::pin(command.finish_after_successful_backend(owner));
    pending(terminal.as_mut()).await;
    drop(terminal);
    holder.complete();
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
    assert!(f.db.authority_domain.snapshot().uncertain);
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("unpolled-cancel"))).await;
    drop(command.cancel_before_effects(owner));
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("unpolled-failure"))).await;
    drop(command.finish_failed_backend(owner));
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
}

#[tokio::test]
async fn lost_owner_or_foreign_owner_cannot_complete_or_start_later_phase() {
    let f = Fixture::new().await;
    let AdmittedRepositoryCommand { command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("first"))).await;
    let AdmittedRepositoryCommand { command: mut other, owner: foreign } = f.admit(RepositoryPlan::Create(f.new_repo("other"))).await;
    assert!(command.finish_failed_backend(foreign).await.is_err());
    assert!(matches!(other.execute_next().await, RepositoryPhaseResult::Unconfirmed { .. }));
    drop(other.quarantine(owner));
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
    timeout(WAIT, f.db.close()).await.unwrap(); // Unknown idle cells cannot request later SQL.
}

#[tokio::test]
async fn deferred_fk_commit_failure_quarantines_actual_sql_outcome_without_fake_rollback_ack() {
    let f = Fixture::with_commit_fault(true).await;
    sqlx::query("UPDATE fixture_gate SET enabled = 1").execute(&f.db.pool).await.unwrap();
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("commit-fault"))).await;
    assert!(matches!(
        command.execute_next().await,
        RepositoryPhaseResult::Unconfirmed {
            original: StorageError::Sqlite(sqlx::Error::Database(error)), rollback_error: None
        } if error.code().as_deref() == Some("787") && error.message() == "FOREIGN KEY constraint failed"
    ));
    assert!(command.finish_failed_backend(owner).await.is_err());
    let spare = f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("spare"))).unwrap();
    assert!(matches!(
        f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("full"))),
        Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Capacity, .. })
    ));
    drop(spare);
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
}

#[test]
fn invalid_repository_row_is_a_conversion_unit_not_post_commit_fault_injection() {
    let row = RepositoryRow {
        id: 7,
        owner_id: 1,
        name: "bad/name".into(),
        description: None,
        visibility: "private".into(),
        default_branch: "main".into(),
        created_at: 1,
        updated_at: 1,
        pushed_at: None,
        archived_at: None,
    };
    assert!(matches!(Repository::try_from(row), Err(StorageError::Corrupt(_))));
}

#[tokio::test]
async fn missing_rows_commit_original_unit_and_false_results_for_every_existing_plan() {
    let f = Fixture::new().await;
    let repo = RepoId(-999_999);
    for plan in [
        RepositoryPlan::General { repo, settings: settings() },
        RepositoryPlan::Rename { repo, name: "missing".parse().unwrap() },
        RepositoryPlan::Transfer { repo, new_owner: AccountId(-888_888) },
        RepositoryPlan::Delete { repo, via: RepositoryDeleteSource::Repositories },
        RepositoryPlan::Delete { repo, via: RepositoryDeleteSource::Settings },
    ] {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
        let report = settled(command.execute_next().await);
        assert_eq!(report.boundary, RepositorySqlBoundary::Committed);
        assert!(matches!(report.result, Ok(RepositoryOutput::Unit)));
        command.finish_after_successful_backend(owner).await.unwrap();
    }
    for plan in [RepositoryPlan::Archive { repo, by: AccountId(-888_888), at: Timestamp(100) }, RepositoryPlan::Unarchive { repo }] {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
        let report = settled(command.execute_next().await);
        assert_eq!(report.boundary, RepositorySqlBoundary::Committed);
        assert!(matches!(report.result, Ok(RepositoryOutput::Changed(false))));
        command.finish_after_successful_backend(owner).await.unwrap();
    }
}

#[tokio::test]
async fn actual_compensation_statement_refusal_with_acknowledged_rollback_releases_known_partial_credit() {
    let f = Fixture::with_commit_fault(true).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::Create(f.new_repo("keep-row"))).await;
    let report = settled(command.execute_next().await);
    let id = report.repository_id.unwrap();
    report.result.unwrap();
    sqlx::query("UPDATE fixture_gate SET enabled = 2").execute(&f.db.pool).await.unwrap();
    let report = settled(command.compensate_created().await);
    assert_eq!(report.boundary, RepositorySqlBoundary::RolledBack);
    assert!(
        matches!(report.result, Err(StorageError::Sqlite(sqlx::Error::Database(error))) if error.code().as_deref() == Some("1811") && error.message() == "injected compensation statement refusal")
    );
    let completion = command.finish_failed_backend(owner).await.unwrap();
    assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
    assert_eq!(completion.progress.create_compensation, Some(RepositorySqlBoundary::RolledBack));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories WHERE id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 1);
    f.create("later").await; // No claim the failed compensation reconciled the retained row.
}

#[tokio::test]
async fn final_resume_rechecks_private_unknown_marker_before_acknowledgement() {
    let f = Fixture::new().await;
    let id = f.create("recheck").await;
    let AdmittedRepositoryCommand { mut command, owner } =
        f.admit(RepositoryPlan::Rename { repo: id, name: "new-name".parse().unwrap() }).await;
    settled(command.execute_next().await).result.unwrap();
    let cell = command.cell.clone();
    let holder = f.db.authority_domain.writer().begin().await.unwrap();
    let mut terminal = Box::pin(command.finish_after_successful_backend(owner));
    pending(terminal.as_mut()).await;
    // Explicit private marker injection, not a fake driver ACK/backend failure.
    // It calls the same production marker used by actual owner Drop.
    cell.unknown();
    holder.complete();
    assert!(timeout(WAIT, terminal).await.unwrap().is_err());
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
}

#[tokio::test]
async fn neutral_capacity_refusal_does_not_consume_two_repository_cells_or_issue_sql() {
    let f = Fixture::new().await;
    let mut held = Vec::new();
    for _ in 0..rbit_authority_fence::MAX_MUTATIONS {
        held.push(f.db.authority_domain.writer().begin().await.unwrap().suspend());
    }
    let result = f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("not-admitted"))).unwrap().acquire().await;
    assert!(matches!(result, Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Capacity, .. })));
    for token in held {
        token.resume().await.complete();
    } // No standalone follower complete.
    assert!(f.db.repositories().find_by_path("alice", "not-admitted").await.unwrap().is_none());
    f.create("later").await;
}

#[tokio::test]
async fn actual_raise_rollback_preserves_query_error_and_distinct_explicit_rollback_rejection() {
    let f = Fixture::with_commit_fault(true).await;
    let id = f.create("rollback-fixture").await;
    sqlx::query("UPDATE fixture_gate SET enabled = 3").execute(&f.db.pool).await.unwrap();
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(RepositoryPlan::General { repo: id, settings: settings() }).await;
    let result = command.execute_next().await;
    // Explicit fixture RAISE(ROLLBACK): physical SQLite rollback does not update
    // SQLx's tracked depth. This tests real returned rejection, not a lost ACK.
    match result {
        RepositoryPhaseResult::Unconfirmed {
            original: StorageError::Sqlite(sqlx::Error::Database(original)),
            rollback_error: Some(StorageError::Sqlite(sqlx::Error::Database(rollback))),
        } => {
            assert_eq!(original.code().as_deref(), Some("1811"));
            assert_eq!(original.message(), "injected outer rollback");
            assert_eq!(rollback.code().as_deref(), Some("1"));
            assert_eq!(rollback.message(), "cannot rollback - no transaction is active");
        }
        other => panic!("expected original query error and actual rollback rejection: {other:?}"),
    }
    assert!(command.finish_failed_backend(owner).await.is_err());
    let spare = f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("retained-spare"))).unwrap();
    assert!(matches!(
        f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("full"))),
        Err(StorageError::AdmissionUnavailable { reason: AdmissionReason::Capacity, .. })
    ));
    drop(spare);
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
    // No retry, tracked-depth repair or cleanup acknowledgement is inferred.
}

#[tokio::test]
async fn concrete_owned_rust_caller_can_move_command_execute_and_acknowledge_without_facade_spawn() {
    let f = Fixture::new().await;
    let admitted = f.admit(RepositoryPlan::Create(f.new_repo("owned-caller"))).await;
    let task = tokio::spawn(async move {
        let AdmittedRepositoryCommand { mut command, owner } = admitted;
        let report = settled(command.execute_next().await);
        report.result.unwrap();
        let completion = command.finish_after_successful_backend(owner).await.unwrap();
        (report.repository_id.unwrap(), completion)
    });
    let (id, completion) = timeout(WAIT, task).await.unwrap().unwrap();
    assert_eq!(completion.logical, RepositoryLogicalSettlement::EnrollmentAcknowledged);
    assert_eq!(f.db.repositories().find_by_path("alice", "owned-caller").await.unwrap().unwrap().repo.id, id);
    // SQL-only caller: backend assertion is vacuous, not actual Git/HTTP adoption.
}

// Fixed-continuation contract cases; no Services/Git adoption or timing parity claim.
async fn creator_plan(f: &Fixture, name: &str, actor: AccountId) -> RepositoryPlan {
    let org =
        f.db.orgs()
            .create(NewOrganization { name: name.parse().unwrap(), display_name: None, description: None, creator: f.alice })
            .await
            .unwrap()
            .account
            .id;
    if actor != f.alice {
        f.db.orgs().add_member(org, actor, OrgRole::Member).await.unwrap();
    }
    let mut repository = f.new_repo("created");
    repository.owner_id = org;
    RepositoryPlan::CreateWithCreator {
        repository,
        creator: OrganizationCreator { actor, actor_is_site_admin: false, owner_is_organization: true },
    }
}
fn deferred(id: RepoId, name: &str, raw: Option<&str>, archival: Option<RepositoryDeferredArchive>) -> RepositoryPlan {
    RepositoryPlan::DeferredSettings(RepositoryDeferredSettingsPlan {
        repo: id,
        initial_name: name.parse().unwrap(),
        general: settings(),
        rename_input: raw.map(str::to_owned),
        archival,
    })
}

#[tokio::test]
async fn fixed_creator_grant_uses_insert_id_and_original_actor_provenance() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "creator-org", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let created = settled(command.execute_next().await);
    let id = created.repository_id.unwrap();
    created.result.unwrap();
    assert_eq!(f.db.authority_domain.snapshot().active, 1);
    let grant = settled(command.execute_next().await);
    assert_eq!(grant.boundary, RepositorySqlBoundary::Committed);
    assert!(matches!(grant.result, Ok(RepositoryOutput::CreatorGrant(CreatorGrantOutcome::Granted))));
    let row: (i64, i64, String) =
        sqlx::query_as("SELECT repo_id, added_by, permission FROM collaborators WHERE repo_id = ? AND user_id = ?")
            .bind(id.0)
            .bind(f.bob.0)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(row, (id.0, f.bob.0, "admin".to_owned()));
    command.finish_after_successful_backend(owner).await.unwrap();
    assert_eq!(f.db.authority_domain.snapshot().active, 0);
}

#[tokio::test]
async fn creator_reads_role_after_insert_and_skip_does_not_wait_for_serial() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "late-role", f.bob).await;
    let org = match &plan {
        RepositoryPlan::CreateWithCreator { repository, .. } => repository.owner_id,
        _ => unreachable!(),
    };
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    f.db.orgs().set_role(org, f.bob, OrgRole::Owner).await.unwrap();
    let serial = f.db.authority_domain.writer().begin().await.unwrap();
    let (result, work) = crate::count_work(timeout(WAIT, command.execute_next())).await;
    let report = settled(result.unwrap());
    assert_eq!(report.boundary, RepositorySqlBoundary::NoTransaction);
    assert!(matches!(report.result, Ok(RepositoryOutput::CreatorGrant(CreatorGrantOutcome::NotMember))));
    assert_eq!(work.statements, 1, "actual late role read; no grant transaction");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM collaborators WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    serial.complete();
    command.finish_after_successful_backend(owner).await.unwrap();
}

#[tokio::test]
async fn creator_personal_and_site_admin_short_circuit_without_role_query() {
    let f = Fixture::new().await;
    for (name, is_org, is_admin) in [("personal-skip", false, false), ("admin-skip", true, true)] {
        let AdmittedRepositoryCommand { mut command, owner } = f
            .admit(RepositoryPlan::CreateWithCreator {
                repository: f.new_repo(name),
                creator: OrganizationCreator { actor: f.bob, owner_is_organization: is_org, actor_is_site_admin: is_admin },
            })
            .await;
        settled(command.execute_next().await).result.unwrap();
        let serial = f.db.authority_domain.writer().begin().await.unwrap();
        let (result, work) = crate::count_work(timeout(WAIT, command.execute_next())).await;
        assert!(matches!(settled(result.unwrap()).result, Ok(RepositoryOutput::CreatorGrant(CreatorGrantOutcome::NotRequired))));
        assert_eq!(work, crate::Work { acquisitions: 0, statements: 0, cache_writes: 0 });
        serial.complete();
        command.finish_after_successful_backend(owner).await.unwrap();
    }
}

#[tokio::test]
async fn creator_compensation_consumes_unentered_stage_and_is_forbidden_after_entry() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "compensate-org", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    settled(command.compensate_created().await).result.unwrap();
    command.finish_failed_backend(owner).await.unwrap();
    assert!(f.db.admin().find_repository(id).await.unwrap().is_none());
    let plan = creator_plan(&f, "entered-org", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    settled(command.execute_next().await).result.unwrap();
    assert!(matches!(command.compensate_created().await, RepositoryPhaseResult::Unconfirmed { .. }));
    assert!(f.db.admin().find_repository(id).await.unwrap().is_some());
    assert!(command.finish_failed_backend(owner).await.is_err());
}

#[tokio::test]
async fn creator_statement_failure_rolls_back_without_deleting_created_repository() {
    let f = Fixture::with_commit_fault(true).await;
    let plan = creator_plan(&f, "refused-grant", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    sqlx::query("UPDATE fixture_gate SET enabled = 4").execute(&f.db.pool).await.unwrap();
    let report = settled(command.execute_next().await);
    assert_eq!(report.boundary, RepositorySqlBoundary::RolledBack);
    assert!(
        matches!(report.result, Err(StorageError::Sqlite(sqlx::Error::Database(error))) if error.code().as_deref() == Some("1811") && error.message() == "injected creator grant refusal")
    );
    assert!(f.db.admin().find_repository(id).await.unwrap().is_some());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM collaborators WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    let result = command.finish_failed_backend(owner).await.unwrap();
    assert_eq!(result.logical, RepositoryLogicalSettlement::FailureRecorded);
    assert!(f.db.authority_domain.snapshot().uncertain);
    f.create("later-after-grant-refusal").await;
}

#[tokio::test]
async fn creator_read_error_has_no_write_transaction_and_failed_terminal_reuses_credit() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "closed-read", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    settled(command.execute_next().await).result.unwrap();
    // Actual closed read pool, not a fabricated SQL error or lost acknowledgement.
    f.db.pool.close().await;
    let report = settled(command.execute_next().await);
    assert_eq!(report.boundary, RepositorySqlBoundary::NoTransaction);
    assert!(matches!(report.result, Err(StorageError::Sqlite(sqlx::Error::PoolClosed))));
    command.finish_failed_backend(owner).await.unwrap();
    let first = f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("reserved-one"))).unwrap();
    let second = f.db.repository_commands().reserve(RepositoryPlan::Create(f.new_repo("reserved-two"))).unwrap();
    drop((first, second));
}

#[tokio::test]
async fn deferred_invalid_name_is_lazy_no_resume_or_dml_and_preserves_general_commit() {
    let f = Fixture::new().await;
    let id = f.create("lazy-name").await;
    let AdmittedRepositoryCommand { mut command, owner } =
        f.admit(deferred(id, "lazy-name", Some("bad/name"), Some(RepositoryDeferredArchive::Archive { by: f.alice }))).await;
    settled(command.execute_next().await).result.unwrap();
    let serial = f.db.authority_domain.writer().begin().await.unwrap();
    let (result, work) = crate::count_work(timeout(WAIT, command.execute_next())).await;
    assert!(matches!(result.unwrap(), RepositoryPhaseResult::RejectedName(NameError::InvalidCharacter { kind: "repository name" })));
    assert_eq!(work, crate::Work { acquisitions: 0, statements: 0, cache_writes: 0 });
    let row: (String, String, String) = sqlx::query_as("SELECT name, description, visibility FROM repositories WHERE id = ?")
        .bind(id.0)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(row, ("lazy-name".to_owned(), "changed".to_owned(), "public".to_owned()));
    let archives: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repo_archives WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(archives, 0);
    serial.complete();
    let completion = command.finish_failed_backend(owner).await.unwrap();
    assert_eq!(completion.progress.committed_forward_steps, 1);
    assert_eq!(completion.progress.last_forward_boundary, Some(RepositorySqlBoundary::NoTransaction));
    assert_eq!(completion.logical, RepositoryLogicalSettlement::FailureRecorded);
}

#[tokio::test]
async fn deferred_name_filter_and_case_only_change_preserve_fixed_outputs_and_redirects() {
    let f = Fixture::new().await;
    let id = f.create("case-name").await;
    for (raw, changed) in [(None, false), (Some(" case-name "), false), (Some("CASE-NAME"), true)] {
        let AdmittedRepositoryCommand { mut command, owner } = f.admit(deferred(id, "case-name", raw, None)).await;
        settled(command.execute_next().await).result.unwrap();
        let report = settled(command.execute_next().await);
        if changed {
            assert_eq!(report.boundary, RepositorySqlBoundary::Committed);
            assert!(matches!(report.result, Ok(RepositoryOutput::Renamed(ref name)) if name.as_str() == "CASE-NAME"));
        } else {
            assert_eq!(report.boundary, RepositorySqlBoundary::NoTransaction);
            assert!(matches!(report.result, Ok(RepositoryOutput::RenameUnchanged(ref name)) if name.as_str() == "case-name"));
        }
        command.finish_after_successful_backend(owner).await.unwrap();
    }
    let redirects: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repo_redirects WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(redirects, 0);
}

#[tokio::test]
async fn fixed_archive_now_and_creator_continue_after_stop_with_no_new_admission() {
    let f = Fixture::new().await;
    let id = f.create("archive-late").await;
    let creator = creator_plan(&f, "stopped-creator", f.bob).await;
    let AdmittedRepositoryCommand { command: mut create, owner: create_owner } = f.admit(creator).await;
    settled(create.execute_next().await).result.unwrap();
    let AdmittedRepositoryCommand { mut command, owner } =
        f.admit(deferred(id, "archive-late", None, Some(RepositoryDeferredArchive::Archive { by: f.alice }))).await;
    settled(command.execute_next().await).result.unwrap();
    settled(command.execute_next().await).result.unwrap();
    drop(f.db.clone().close()); // Stops new registrations, but does not poll pool closure.
    assert!(matches!(settled(create.execute_next().await).result, Ok(RepositoryOutput::CreatorGrant(CreatorGrantOutcome::Granted))));
    create.finish_after_successful_backend(create_owner).await.unwrap();
    let before = Timestamp::now().seconds();
    assert!(matches!(settled(command.execute_next().await).result, Ok(RepositoryOutput::Changed(true))));
    let after = Timestamp::now().seconds();
    let attribution: (i64, i64) = sqlx::query_as("SELECT archived_by, archived_at FROM repo_archives WHERE repo_id = ?")
        .bind(id.0)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(attribution.0, f.alice.0);
    assert!((before..=after).contains(&attribution.1)); // Value bounds; capture-before-resume is source-reviewed separately.
    command.finish_after_successful_backend(owner).await.unwrap();
    assert_eq!(f.db.authority_domain.snapshot().active, 0);
}

#[tokio::test]
async fn creator_phase_cancellation_during_preparation_or_resume_retains_busy_until_future_drop() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "cancel-creator", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    let serial = f.db.authority_domain.writer().begin().await.unwrap();
    let mut phase = Box::pin(command.execute_next());
    pending(phase.as_mut()).await; // Enrollment/progress only, not proof the role read/driver completed.
    drop(owner);
    let mut drain = Box::pin(f.db.drain_repository_commands(Instant::now() + WAIT));
    pending(drain.as_mut()).await;
    drop(phase);
    serial.complete();
    assert_eq!(timeout(WAIT, drain).await.unwrap(), Err(RepositoryCommandDrainError::Unconfirmed));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM collaborators WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    drop(command);
}

#[tokio::test]
async fn creator_upsert_preserves_preexisting_added_by_and_created_at() {
    let f = Fixture::new().await;
    let plan = creator_plan(&f, "existing-grant", f.bob).await;
    let AdmittedRepositoryCommand { mut command, owner } = f.admit(plan).await;
    let id = settled(command.execute_next().await).repository_id.unwrap();
    f.db.collaborators().add(id, f.bob, Permission::Write, f.alice).await.unwrap();
    let before: (i64, i64) = sqlx::query_as("SELECT added_by, created_at FROM collaborators WHERE repo_id = ? AND user_id = ?")
        .bind(id.0)
        .bind(f.bob.0)
        .fetch_one(&f.db.pool)
        .await
        .unwrap();
    assert_eq!(before.0, f.alice.0);
    settled(command.execute_next().await).result.unwrap();
    let after: (i64, i64, String) =
        sqlx::query_as("SELECT added_by, created_at, permission FROM collaborators WHERE repo_id = ? AND user_id = ?")
            .bind(id.0)
            .bind(f.bob.0)
            .fetch_one(&f.db.pool)
            .await
            .unwrap();
    assert_eq!(after, (before.0, before.1, "admin".to_owned()));
    command.finish_after_successful_backend(owner).await.unwrap();
}

#[tokio::test]
async fn rejected_name_halts_declared_archive_and_cannot_acknowledge_success() {
    let f = Fixture::new().await;
    let id = f.create("halted").await;
    let AdmittedRepositoryCommand { mut command, owner } =
        f.admit(deferred(id, "halted", Some("bad/name"), Some(RepositoryDeferredArchive::Archive { by: f.alice }))).await;
    settled(command.execute_next().await).result.unwrap();
    assert!(matches!(command.execute_next().await, RepositoryPhaseResult::RejectedName(_)));
    let (next, work) = crate::count_work(command.execute_next()).await;
    assert!(matches!(next, RepositoryPhaseResult::Unconfirmed { original: StorageError::Invariant(_), .. }));
    assert_eq!(work, crate::Work { acquisitions: 0, statements: 0, cache_writes: 0 });
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repo_archives WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    assert!(command.finish_after_successful_backend(owner).await.is_err());
}

#[tokio::test]
async fn rejected_name_direct_successful_terminal_is_refused_without_another_invalid_phase() {
    let f = Fixture::new().await;
    let id = f.create("direct-rejection").await;
    let AdmittedRepositoryCommand { mut command, owner } =
        f.admit(deferred(id, "direct-rejection", Some("bad/name"), Some(RepositoryDeferredArchive::Archive { by: f.alice }))).await;
    settled(command.execute_next().await).result.unwrap();
    assert!(matches!(command.execute_next().await, RepositoryPhaseResult::RejectedName(_)));
    assert!(!command.cell.lock().unknown, "isolate the failed-only terminal guard before any invalid call");
    assert_eq!(command.script.terminal_requirement, TerminalRequirement::FailedFlow);
    assert!(command.script.steps.is_empty());
    assert_eq!(command.repository_id(), Some(id));
    assert!(command.finish_after_successful_backend(owner).await.is_err());
    let row: (String, String) =
        sqlx::query_as("SELECT name, description FROM repositories WHERE id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(row, ("direct-rejection".to_owned(), "changed".to_owned()));
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repo_archives WHERE repo_id = ?").bind(id.0).fetch_one(&f.db.pool).await.unwrap();
    assert_eq!(count, 0);
    assert!(f.db.authority_domain.snapshot().uncertain);
    assert_eq!(f.db.drain_repository_commands(Instant::now() + WAIT).await, Err(RepositoryCommandDrainError::Unconfirmed));
}
