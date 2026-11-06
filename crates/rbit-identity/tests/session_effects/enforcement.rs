use rbit_contracts::AccountId;
use rbit_identity::{
    TokenDigest,
    sessions::effects::{
        EnforcementError, PasswordChangeEvidence, PasswordChangeEvidenceSource, PasswordSessionEnforcement, SessionEffectsError,
    },
};
use std::{
    collections::VecDeque,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Semaphore;

#[derive(Debug)]
struct StoreError;
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("store failed")
    }
}
impl std::error::Error for StoreError {}

#[derive(Debug, Clone)]
struct Effects {
    calls: Arc<Mutex<Vec<Option<TokenDigest>>>>,
    started: Arc<Semaphore>,
    release: Arc<Semaphore>,
    blocked: Arc<AtomicBool>,
}
impl Effects {
    fn new(blocked: bool) -> Self {
        Self {
            calls: Arc::default(),
            started: Arc::new(Semaphore::new(0)),
            release: Arc::new(Semaphore::new(0)),
            blocked: Arc::new(AtomicBool::new(blocked)),
        }
    }
    async fn wait_started(&self) {
        self.started.acquire().await.unwrap().forget();
    }
    fn release(&self) {
        self.blocked.store(false, Ordering::SeqCst);
        self.release.add_permits(1);
    }
}
impl rbit_identity::sessions::effects::SessionRevocationEffects for Effects {
    type Error = StoreError;
    async fn revoke(&self, _account: AccountId, keep: Option<TokenDigest>) -> Result<(), Self::Error> {
        self.started.add_permits(1);
        if self.blocked.load(Ordering::SeqCst) {
            self.release.acquire().await.unwrap().forget();
        }
        self.calls.lock().unwrap().push(keep);
        Ok(())
    }
    fn failed(&self, _: AccountId, _: Option<TokenDigest>, _: &Self::Error) {}
    fn unproved(&self, _: AccountId, _: &'static str) {}
}
struct Evidence(VecDeque<PasswordChangeEvidence>);
impl PasswordChangeEvidenceSource for Evidence {
    fn try_next(&mut self) -> PasswordChangeEvidence {
        self.0.pop_front().unwrap_or(PasswordChangeEvidence::Empty)
    }
}
fn changed() -> Evidence {
    Evidence([PasswordChangeEvidence::Changed(AccountId(1))].into())
}

#[tokio::test]
async fn guard_finish_adopts_after_close_and_caller_cancellation_cannot_lose_write() {
    let effects = Effects::new(true);
    let owner = PasswordSessionEnforcement::new(effects.clone());
    let guard = owner.guard(AccountId(1), [7; 32], changed).unwrap();
    let finalizer = owner.finalizer();
    owner
        .reserve()
        .unwrap()
        .submit(move |context| async move { finalizer.committed(&context, AccountId(1), true).await })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    owner.close();
    let request = tokio::spawn(guard.finish(true));
    effects.wait_started().await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(!owner.report().drained());
    assert_eq!(*effects.calls.lock().unwrap(), [] as [Option<TokenDigest>; 0]);
    effects.release();
    let report = owner.shutdown().await;
    assert!(report.drained());
    assert_eq!(report.abnormal, 0);
    assert_eq!(*effects.calls.lock().unwrap(), [Some([7; 32])]);
}

#[tokio::test]
async fn dropped_unpolled_guard_finish_uses_fail_closed_all_session_effect() {
    let effects = Effects::new(false);
    let owner = PasswordSessionEnforcement::new(effects.clone());
    let guard = owner.guard(AccountId(1), [7; 32], changed).unwrap();
    let finalizer = owner.finalizer();
    owner
        .reserve()
        .unwrap()
        .submit(move |context| async move { finalizer.committed(&context, AccountId(1), true).await })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    owner.close();
    drop(guard.finish(true));
    let report = owner.shutdown().await;
    assert!(report.drained());
    assert_eq!(*effects.calls.lock().unwrap(), [None]);
}

#[tokio::test]
async fn foreign_scope_is_rejected_before_effect_or_attribution_mutation() {
    let effects = Effects::new(false);
    let owner = PasswordSessionEnforcement::new(effects.clone());
    let other = PasswordSessionEnforcement::new(effects.clone());
    let finalizer = owner.finalizer();
    let result = other
        .reserve()
        .unwrap()
        .submit(move |context| async move { finalizer.committed(&context, AccountId(1), false).await })
        .unwrap()
        .await
        .unwrap();
    assert!(matches!(result, Err(EnforcementError::Ownership(SessionEffectsError::WrongScope))));
    assert_eq!(*effects.calls.lock().unwrap(), [] as [Option<TokenDigest>; 0]);
    assert!(other.shutdown().await.drained());
    assert!(owner.shutdown().await.drained());
}

#[tokio::test]
async fn reset_enforcement_ignores_guard_and_lagged_unrelated_evidence_cannot_claim_future_change() {
    let effects = Effects::new(false);
    let owner = PasswordSessionEnforcement::new(effects.clone());
    let guard =
        owner.guard(AccountId(1), [7; 32], || Evidence([PasswordChangeEvidence::Other, PasswordChangeEvidence::Lagged].into())).unwrap();
    let finalizer = owner.finalizer();
    owner
        .reserve()
        .unwrap()
        .submit(move |context| async move { finalizer.committed(&context, AccountId(1), false).await })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    guard.finish(true).await;
    assert_eq!(*effects.calls.lock().unwrap(), [None], "without deferred self-service commits, lag cannot invent ownership");
    assert!(owner.shutdown().await.drained());
}

#[tokio::test]
async fn subscription_panic_cannot_leave_ghost_guard_attribution_or_ownership() {
    let effects = Effects::new(false);
    let owner = PasswordSessionEnforcement::new(effects.clone());
    let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        owner.guard::<Evidence>(AccountId(1), [7; 32], || panic!("subscription failed"))
    }));
    assert!(attempted.is_err());
    assert_eq!(owner.report().active_owners, 0);
    let finalizer = owner.finalizer();
    owner
        .reserve()
        .unwrap()
        .submit(move |context| async move { finalizer.committed(&context, AccountId(1), true).await })
        .unwrap()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(*effects.calls.lock().unwrap(), [None], "no ghost guard may defer the next committed change");
    assert!(owner.shutdown().await.drained());
}
