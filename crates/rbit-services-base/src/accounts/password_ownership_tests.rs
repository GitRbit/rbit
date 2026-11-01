//! Real SQL/session acknowledgements. Finite gates denote injected hash/audit phases.
use super::*;
use crate::{ServiceContext, Services};
use std::{
    future::{Future, poll_fn},
    pin::Pin,
    task::Poll,
    time::Duration,
};
use tokio::{sync::oneshot, time::timeout};

const WAIT: Duration = Duration::from_secs(5);
const PASSWORD: &str = "correct horse battery";
const NEW_PASSWORD: &str = "a whole new password";
struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    user: User,
    current: SessionToken,
    other: SessionToken,
}
impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().to_owned();
        let database = rbit_storage::Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = database.open_kv(&config.storage.kv_path()).unwrap();
        let services = Box::pin(Services::new(Arc::new(config), &database, &kv)).await.unwrap();
        let user = database
            .accounts()
            .create_user(NewUser {
                name: "alice".parse().unwrap(),
                email: "alice@example.com".parse().unwrap(),
                display_name: None,
                password_hash: Passwords::hash(PASSWORD.to_owned()).await.unwrap(),
                is_admin: true,
            })
            .await
            .unwrap();
        let current = services.accounts.start_session_for(&user, &ClientInfo::default()).await.unwrap();
        let other = services.accounts.start_session_for(&user, &ClientInfo::default()).await.unwrap();
        Self { _directory: directory, services, user, current, other }
    }
    fn ctx(&self) -> &ServiceContext {
        self.services.context()
    }
    async fn live(&self, token: &SessionToken) -> bool {
        self.ctx().kv.sessions().get(token.digest()).await.unwrap().is_some()
    }
    async fn hash(&self) -> String {
        self.ctx().database.profiles().password_hash(self.user.id()).await.unwrap().unwrap()
    }
    async fn change(&self) {
        timeout(WAIT, self.services.profiles.change_password(&self.user, PASSWORD.into(), NEW_PASSWORD.into())).await.unwrap().unwrap();
    }
    async fn drain(&self) {
        timeout(WAIT, self.services.accounts.drain_password_changes()).await.unwrap().unwrap();
        timeout(WAIT, self.ctx().database.drain_self_password_changes()).await.unwrap().unwrap();
    }
    // Only verification/hash are bypassed in this fixture projection; real typed
    // SQL commit, ledger, audit and event ordering remain the production sequence.
    async fn injected_commit(&self, hash: &str) {
        let accounts = self.services.accounts.clone();
        let user = self.user.clone();
        let expected = self.hash().await;
        let hash = hash.to_owned();
        let (owner, changes) = self.services.accounts.self_password_owner();
        timeout(
            WAIT,
            owner.run(changes, move |reservation, lease| async move {
                let pending = reservation.start(user.id(), hash, expected)?.wait().await?.unwrap();
                accounts.password_changed_preserving_current(user.id(), pending, lease).await?;
                accounts.audit.record(AuditEntry::new(AuditAction::PasswordChanged).by(&user)).await;
                accounts.events.publish(DomainEvent::PasswordChanged { user: user.id() });
                Ok(())
            }),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
async fn pending<F: Future + ?Sized>(mut future: Pin<&mut F>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn combined_handler_releases_serial_and_unpolled_finish_owns_revocation() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    f.change().await;
    assert!(f.live(&f.current).await && f.live(&f.other).await);
    timeout(WAIT, f.services.profiles.change_email(&f.user, NEW_PASSWORD.into(), "new@example.com")).await.unwrap().unwrap();
    timeout(WAIT, f.services.profiles.change_username(&f.user, NEW_PASSWORD.into(), "renamed")).await.unwrap().unwrap();
    drop(guard.finish(true)); // Synchronous decision, no future polling required.
    f.drain().await;
    assert!(f.live(&f.current).await);
    assert!(!f.live(&f.other).await);
    let found = f.ctx().database.accounts().find_user(f.user.id()).await.unwrap().unwrap();
    assert_eq!(found.name().as_str(), "renamed");
    assert_eq!(found.email.as_str(), "new@example.com");
}

#[tokio::test]
async fn last_success_takes_both_real_commitments_in_one_end_all_union() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    f.injected_commit("first").await;
    f.injected_commit("second").await;
    assert!(f.live(&f.current).await && f.live(&f.other).await);
    {
        let entry = f.services.accounts.password_changes.lock();
        assert_eq!(entry.get(&f.user.id()).unwrap().commits.len(), 2);
    }
    timeout(WAIT, guard.finish(true)).await.unwrap();
    f.drain().await;
    assert!(!f.live(&f.current).await && !f.live(&f.other).await);
    let log =
        f.services.audit.list(&f.user, Some(crate::audit::AuditCategory::Sessions), &crate::admin::ListQuery::default()).await.unwrap();
    let ended: Vec<_> = log.items.iter().filter(|e| e.action == AuditAction::SessionsRevokedAfterPasswordChange).collect();
    assert_eq!(ended.len(), 1, "the union is one acknowledged KV/audit batch");
    assert_eq!(ended[0].detail, "2 sessions ended");
}

#[tokio::test]
async fn no_action_guard_retains_other_pending_and_guards_consume_no_operation_slots() {
    let f = Fixture::new().await;
    let current = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    let unrelated = f.services.accounts.guard_password_change(f.user.id(), &f.other);
    let spare = f.services.accounts.guard_password_change(f.user.id(), &f.other);
    f.injected_commit("new").await; // Ordinary guards themselves did not saturate max2.
    unrelated.finish(false).await;
    spare.finish(false).await;
    assert!(f.live(&f.current).await && f.live(&f.other).await);
    let mut shutdown = Box::pin(f.services.accounts.drain_password_changes());
    pending(shutdown.as_mut()).await;
    current.finish(true).await; // Preowned FINAL remains eligible after close.
    timeout(WAIT, shutdown).await.unwrap().unwrap();
    assert!(f.live(&f.current).await);
    assert!(!f.live(&f.other).await);
}

#[tokio::test]
async fn force_while_response_pending_cannot_be_undone_by_keep_current() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    f.change().await;
    let forced = f.ctx().database.forced_password_changes(&f.ctx().kv).unwrap();
    assert_eq!(timeout(WAIT, forced.reserve().unwrap().start(f.user.id(), "forced".into()).unwrap().wait()).await.unwrap().unwrap(), 2);
    guard.finish(true).await;
    f.drain().await;
    assert_eq!(f.hash().await, "forced");
    assert!(!f.live(&f.current).await && !f.live(&f.other).await);
}

#[tokio::test]
async fn lagged_event_attribution_uses_frozen_response_status() {
    for success in [true, false] {
        let f = Fixture::new().await;
        let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
        f.injected_commit("new").await;
        // The original PasswordChanged event is overwritten; legacy Lagged trusts status.
        for _ in 0..2048 {
            f.services.events.publish(DomainEvent::UserSignedUp { user: f.user.id() });
        }
        let completion = guard.finish(success);
        f.services.events.publish(DomainEvent::PasswordChanged { user: f.user.id() }); // Too late to change frozen attribution.
        drop(completion);
        f.drain().await;
        assert_eq!(f.live(&f.current).await, success);
        assert!(!f.live(&f.other).await);
    }
}

#[tokio::test]
async fn waiter_loss_before_sql_continues_and_guard_ending_early_revokes_all() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    let (entered, entering) = oneshot::channel();
    let (release, releasing) = oneshot::channel();
    let (done, finished) = oneshot::channel();
    let account = f.services.accounts.clone();
    let expected = f.hash().await;
    let user = f.user.id();
    let request = tokio::spawn(async move {
        let (owner, changes) = account.self_password_owner();
        let accounts = account.clone();
        owner
            .run(changes, move |reservation, lease| async move {
                entered.send(()).unwrap(); // Injected validated/hash gate before SQL.
                timeout(WAIT, releasing).await.map_err(ServiceError::internal)?.map_err(ServiceError::internal)?;
                let change = reservation.start(user, "new".into(), expected)?.wait().await?.unwrap();
                accounts.password_changed_preserving_current(user, change, lease).await?;
                done.send(()).unwrap();
                Ok(())
            })
            .await
    });
    timeout(WAIT, entering).await.unwrap().unwrap();
    request.abort();
    assert!(timeout(WAIT, request).await.unwrap().unwrap_err().is_cancelled());
    drop(guard); // No Pending yet; subsequent committed() must choose immediate EndAll.
    release.send(()).unwrap();
    timeout(WAIT, finished).await.unwrap().unwrap();
    f.drain().await;
    assert_eq!(f.hash().await, "new");
    assert!(!f.live(&f.current).await && !f.live(&f.other).await);
}

#[tokio::test]
async fn initial_loss_overlapping_final_success_cannot_clear_uncertainty() {
    let f = Fixture::new().await;
    let mut guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    let (entered, entering) = oneshot::channel();
    let (release, releasing) = oneshot::channel();
    let accounts = f.services.accounts.clone();
    let expected = f.hash().await;
    let user = f.user.id();
    let request = tokio::spawn(async move {
        let account = accounts.clone();
        let (owner, changes) = accounts.self_password_owner();
        owner
            .run(changes, move |reservation, lease| async move {
                let change = reservation.start(user, "new".into(), expected)?.wait().await?.unwrap();
                account.password_changed_preserving_current(user, change, lease).await?;
                entered.send(()).unwrap(); // Real SQL handoff; injected INITIAL audit gate.
                timeout(WAIT, releasing).await.unwrap().unwrap();
                panic!("injected initial owner panic after ledger handoff");
            })
            .await
    });
    timeout(WAIT, entering).await.unwrap().unwrap();
    guard.receiver.take();
    let decision = f.services.accounts.password_changes.take(user, false, f.current.digest()).unwrap();
    drop(guard);
    let (final_entered, final_entering) = oneshot::channel();
    let (final_release, final_releasing) = oneshot::channel();
    let final_waiter = decision.dispatch_test(move |enforcer, user, removed| async move {
        final_entered.send(removed).unwrap(); // Actual KV COMMIT already acknowledged.
        timeout(WAIT, final_releasing).await.unwrap().unwrap();
        enforcer.record_ended_sessions(user, removed).await;
    });
    assert_eq!(timeout(WAIT, final_entering).await.unwrap().unwrap(), 2);
    release.send(()).unwrap();
    assert!(timeout(WAIT, request).await.unwrap().unwrap().is_err());
    let mut drain = Box::pin(f.services.accounts.drain_password_changes());
    pending(drain.as_mut()).await; // FINAL lease remains even after INITIAL lost.
    final_release.send(()).unwrap();
    timeout(WAIT, final_waiter.wait()).await.unwrap().unwrap();
    assert!(timeout(WAIT, drain).await.unwrap().is_err());
    timeout(WAIT, f.ctx().database.drain_self_password_changes()).await.unwrap().unwrap();
    assert!(!f.live(&f.current).await && !f.live(&f.other).await);
}

#[tokio::test]
async fn pending_finalizer_has_no_strong_ledger_back_edge_during_real_audit_tail() {
    let f = Fixture::new().await;
    let mut guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    let ledger = Arc::downgrade(&f.services.accounts.password_changes);
    f.injected_commit("new").await;
    guard.receiver.take();
    let decision = f.services.accounts.password_changes.take(f.user.id(), false, f.current.digest()).unwrap();
    drop(guard);
    let (entered, entering) = oneshot::channel();
    let (release, releasing) = oneshot::channel();
    let completion = decision.dispatch_test(move |enforcer, user, removed| async move {
        entered.send(removed).unwrap();
        timeout(WAIT, releasing).await.unwrap().unwrap();
        enforcer.record_ended_sessions(user, removed).await;
    });
    assert_eq!(timeout(WAIT, entering).await.unwrap().unwrap(), 2);
    let db = f.ctx().database.clone();
    drop(f.services); // FINAL still owns resources/slot, but must NOT own the ledger.
    assert!(ledger.upgrade().is_none());
    release.send(()).unwrap();
    timeout(WAIT, completion.wait()).await.unwrap().unwrap();
    timeout(WAIT, db.drain_self_password_changes()).await.unwrap().unwrap();
}

#[tokio::test]
async fn admitted_self_slots_stay_charged_while_forced_quota_remains_independent() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    f.injected_commit("one").await;
    f.injected_commit("two").await;
    let (owner, changes) = f.services.accounts.self_password_owner();
    let refusal = owner.run(changes, |_, _| async { panic!("capacity must refuse before verification/spawn") }).await;
    assert!(matches!(refusal, Err(ServiceError::Conflict("self password command capacity"))));
    let forced = f.ctx().database.forced_password_changes(&f.ctx().kv).unwrap();
    let a = forced.reserve().unwrap();
    let b = forced.reserve().unwrap();
    drop((a, b));
    drop(guard.finish(true));
    f.drain().await;
}

#[test]
fn guard_drop_outside_runtime_dispatches_preowned_work_on_captured_handle() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let f = runtime.block_on(Fixture::new());
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    runtime.block_on(f.injected_commit("new"));
    drop(guard); // No entered runtime; finalizer must use the preowned captured Handle.
    runtime.block_on(async {
        f.drain().await;
        assert!(!f.live(&f.current).await && !f.live(&f.other).await);
    });
}

#[test]
fn closed_captured_runtime_cannot_report_clean_after_deferred_sql_commit() {
    let (finished, receive) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let f = runtime.block_on(Fixture::new());
        let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
        runtime.block_on(f.injected_commit("new"));
        assert_eq!(runtime.block_on(f.hash()), "new");
        drop(runtime);
        drop(guard); // Synchronous task destruction must not hold admission/ledger locks.
        let check = tokio::runtime::Runtime::new().unwrap();
        check.block_on(async {
            assert!(timeout(WAIT, f.services.accounts.drain_password_changes()).await.unwrap().is_err());
            assert!(timeout(WAIT, f.ctx().database.drain_self_password_changes()).await.unwrap().is_err());
        });
        drop(f);
        drop(check);
        finished.send(()).unwrap();
    });
    receive.recv_timeout(WAIT).expect("closed-runtime finalizer dispatch deadlocked");
    thread.join().unwrap();
}

#[tokio::test]
async fn known_wrong_password_and_stopped_start_do_not_fake_lost_task_outcomes() {
    let f = Fixture::new().await;
    let guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
    assert!(matches!(
        f.services.profiles.change_password(&f.user, "wrong".into(), NEW_PASSWORD.into()).await,
        Err(ServiceError::InvalidCredentials)
    ));
    guard.finish(false).await;
    f.drain().await;
    assert!(f.live(&f.current).await && f.live(&f.other).await);
    let result = f.services.profiles.change_password(&f.user, PASSWORD.into(), NEW_PASSWORD.into()).await;
    assert!(matches!(result, Err(ServiceError::Conflict("self password commands stopped"))));
}

#[tokio::test]
async fn service_tail_capacity_is_per_construction_but_storage_binding_is_shared() {
    let f = Fixture::new().await;
    let mut tails = Vec::new();
    for hash in ["first", "second"] {
        let mut guard = f.services.accounts.guard_password_change(f.user.id(), &f.current);
        f.injected_commit(hash).await;
        guard.receiver.take();
        let decision = f.services.accounts.password_changes.take(f.user.id(), false, f.current.digest()).unwrap();
        drop(guard);
        let (entered, entering) = oneshot::channel();
        let (release, releasing) = oneshot::channel();
        let completion = decision.dispatch_test(move |enforcer, user, removed| async move {
            entered.send(()).unwrap(); // Actual KV acknowledged; only injected audit tail remains.
            timeout(WAIT, releasing).await.unwrap().unwrap();
            enforcer.record_ended_sessions(user, removed).await;
        });
        timeout(WAIT, entering).await.unwrap().unwrap();
        tails.push((release, completion));
    }
    let (owner, changes) = f.services.accounts.self_password_owner();
    assert!(matches!(
        owner.run(changes, |_, _| async { panic!("same construction full") }).await,
        Err(ServiceError::Conflict("self password command capacity"))
    ));
    let independent = AccountService::new(
        f.ctx().config.clone(),
        f.ctx().database.accounts(),
        f.ctx().kv.sessions(),
        f.services.instance.clone(),
        EventBus::new(),
        None,
        f.ctx().database.self_password_changes(&f.ctx().kv).unwrap(),
    );
    assert!(!Arc::ptr_eq(&independent.password_changes, &f.services.accounts.password_changes));
    let (other_owner, other_changes) = independent.self_password_owner();
    timeout(
        WAIT,
        other_owner.run(other_changes, |reservation, _lease| async move {
            drop(reservation); // Known prewrite; shared Storage quota is free after acknowledged KV.
            Ok(())
        }),
    )
    .await
    .unwrap()
    .unwrap();
    for (release, completion) in tails {
        release.send(()).unwrap();
        timeout(WAIT, completion.wait()).await.unwrap().unwrap();
    }
    f.drain().await;
    timeout(WAIT, independent.drain_password_changes()).await.unwrap().unwrap();
}
