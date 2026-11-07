//! Actual `PasswordWork` over a one-snapshot port; no current session route is activated.
use super::fixture::{PASSWORD, StoreError, user, work};
use rbit_identity::{
    PasswordWork,
    accounts::{AccountError, LoginRecord, PasswordAuthentication, PasswordProofError},
};
use rbit_identity_models::credential::{CredentialEpoch, CredentialFence, CredentialLoginRepository, CredentialLoginSnapshot};
use rbit_kernel::AdmissionError;
use std::{
    future::{Future, ready},
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct State {
    record: Option<LoginRecord>,
    epoch: i64,
    pending: bool,
    calls: Vec<String>,
    fail: bool,
}
#[derive(Clone, Default)]
struct Repository(Arc<Mutex<State>>);
impl CredentialLoginRepository for Repository {
    type Error = StoreError;
    fn find_login_snapshot(&self, login: &str) -> impl Future<Output = Result<Option<CredentialLoginSnapshot>, Self::Error>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("snapshot:{login}"));
        ready(if state.fail {
            Err(StoreError("lookup failed"))
        } else {
            Ok(state.record.as_ref().map(|record| CredentialLoginSnapshot {
                login: LoginRecord { user: record.user.clone(), password_hash: record.password_hash.clone(), suspended: record.suspended },
                fence: CredentialFence { epoch: CredentialEpoch::new(state.epoch).unwrap(), pending: state.pending },
            }))
        })
    }
}
async fn repository(work: &PasswordWork) -> Repository {
    let hash = work.begin().unwrap().hash(PASSWORD.into()).await.unwrap();
    let repository = Repository::default();
    repository.0.lock().unwrap().record = Some(LoginRecord { user: user(), password_hash: hash, suspended: false });
    repository
}

#[tokio::test]
async fn receipt_keeps_exact_preverify_snapshot_even_if_reset_wins_during_callback() {
    let work = work();
    let repository = repository(&work).await;
    let reset = repository.clone();
    let (account, result) = PasswordAuthentication::new(&repository, &work)
        .authenticate_proven_resolved(" alice ", PASSWORD.into(), move |_| {
            let mut state = reset.0.lock().unwrap();
            state.epoch = 1;
            state.pending = true;
            state.record.as_mut().unwrap().password_hash = "replacement after captured snapshot".into();
            Ok::<_, ()>(())
        })
        .await;
    assert_eq!(account, Some(user().id()));
    let proven = result.unwrap();
    assert_eq!(proven.user().id(), user().id());
    assert_eq!(proven.receipt().original_epoch(), CredentialEpoch::INITIAL);
    assert_eq!(repository.0.lock().unwrap().calls, ["snapshot:alice"]);
    assert_eq!(work.admitted(), 0, "completed setup and verification release their admission slots");
    let debug = format!("{proven:?} {:?}", proven.receipt());
    assert!(!debug.contains(PASSWORD) && !debug.contains("alice") && !debug.contains("argon2"));
    let (user, receipt) = proven.into_parts();
    let mut foreign = user.clone();
    foreign.account.id = rbit_identity::AccountId(999);
    let receipt = receipt.bind_user(foreign).unwrap_err();
    let restored = receipt.bind_user(user).unwrap();
    assert_eq!(restored.receipt().original_epoch(), CredentialEpoch::INITIAL);
}

#[tokio::test]
async fn pending_is_only_a_valid_active_password_outcome_after_shared_verification() {
    let work = work();
    let repository = repository(&work).await;
    repository.0.lock().unwrap().pending = true;
    let (_, wrong) =
        PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved("alice", "wrong".into(), |_| Ok::<_, ()>(())).await;
    assert!(matches!(wrong, Err(PasswordProofError::Account(AccountError::InvalidCredentials))));
    let (_, valid) =
        PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
    assert!(matches!(valid, Err(PasswordProofError::Pending)));
    repository.0.lock().unwrap().record.as_mut().unwrap().suspended = true;
    let (_, suspended) =
        PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
    assert!(matches!(suspended, Err(PasswordProofError::Account(AccountError::Suspended(_)))));
    repository.0.lock().unwrap().record = None;
    let (account, absent) =
        PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved("missing", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
    assert_eq!(account, None);
    assert!(matches!(absent, Err(PasswordProofError::Account(AccountError::InvalidCredentials))));
    assert_eq!(work.admitted(), 0, "completed real and covering attempts leave no active admission");
}

#[tokio::test]
async fn callback_and_store_errors_keep_precedence_over_busy_closed_and_pending() {
    for closed in [false, true] {
        let work = work();
        let repository = Repository::default();
        repository.0.lock().unwrap().pending = true;
        let held = if closed {
            work.close();
            None
        } else {
            Some(work.begin().unwrap())
        };
        let (_, denied) = PasswordAuthentication::new(&repository, &work)
            .authenticate_proven_resolved("alice", PASSWORD.into(), |_| Err("throttled"))
            .await;
        assert!(matches!(denied, Err(PasswordProofError::Account(AccountError::BeforeVerify("throttled")))));
        for suspended in [false, true] {
            repository.0.lock().unwrap().record = Some(LoginRecord { user: user(), password_hash: "malformed".into(), suspended });
            let (_, busy) = PasswordAuthentication::new(&repository, &work)
                .authenticate_proven_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(()))
                .await;
            assert!(matches!(busy,Err(PasswordProofError::Account(AccountError::Admission(reason)))
                if reason==if closed {AdmissionError::Closed} else {AdmissionError::Busy}));
        }
        repository.0.lock().unwrap().fail = true;
        let (_, failed) = PasswordAuthentication::new(&repository, &work)
            .authenticate_proven_resolved("alice", PASSWORD.into(), |_| -> Result<(), ()> {
                panic!("lookup failure must not run callback")
            })
            .await;
        assert!(matches!(failed, Err(PasswordProofError::Account(AccountError::Store(StoreError("lookup failed"))))));
        assert_eq!(work.admitted(), usize::from(!closed));
        drop(held);
    }
}

#[tokio::test]
async fn unpolled_and_oversize_inputs_do_no_lookup_or_work_but_exact_boundary_preserves_admission() {
    fn assert_send<T: Send>(_: &T) {}
    let work = work();
    let repository = Repository::default();
    let operation =
        PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(()));
    assert_send(&operation);
    drop(operation);
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 0);
    let (_, oversize) = PasswordAuthentication::new(&repository, &work)
        .authenticate_proven_resolved("alice", "a".repeat(1025), |_| -> Result<(), ()> { panic!("oversize must not run callback") })
        .await;
    assert!(matches!(oversize, Err(PasswordProofError::Account(AccountError::InvalidCredentials))));
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    work.close();
    let (_, boundary) = PasswordAuthentication::new(&repository, &work)
        .authenticate_proven_resolved("alice", format!("{}a", "雪".repeat(341)), |_| Ok::<_, ()>(()))
        .await;
    assert!(matches!(boundary, Err(PasswordProofError::Account(AccountError::Admission(AdmissionError::Closed)))));
    assert_eq!(repository.0.lock().unwrap().calls, ["snapshot:alice"]);
}
#[test]
fn cancelled_proof_future_retains_submitted_covering_worker_capacity() {
    use std::{future::poll_fn, sync::mpsc, task::Poll, time::Duration};
    let runtime = tokio::runtime::Builder::new_current_thread().max_blocking_threads(1).enable_all().build().unwrap();
    runtime.block_on(async {
        let work = work();
        let repository = Repository::default();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, proceed) = mpsc::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            proceed.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        ready.await.unwrap();
        let mut operation = Box::pin(PasswordAuthentication::new(&repository, &work).authenticate_proven_resolved(
            "missing",
            PASSWORD.into(),
            |_| Ok::<_, ()>(()),
        ));
        poll_fn(|context| {
            assert!(operation.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(repository.0.lock().unwrap().calls, ["snapshot:missing"]);
        assert_eq!(work.admitted(), 1, "the submitted verifier owns one slot");
        drop(operation);
        assert_eq!(work.admitted(), 1, "cancellation retains the actual worker slot");
        assert_eq!(work.begin().unwrap_err(), AdmissionError::Busy);
        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match work.begin() {
                    Ok(attempt) => {
                        drop(attempt);
                        break;
                    }
                    Err(AdmissionError::Busy) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected admission state: {error:?}"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(work.admitted(), 0, "settled worker releases its slot");
    });
}
