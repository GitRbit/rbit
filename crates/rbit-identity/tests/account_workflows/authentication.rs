use rbit_identity::{
    PasswordWork,
    accounts::{AccountError, LoginRecord, PasswordAuthentication},
};
use rbit_kernel::AdmissionError;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;

fn authentication<'a>(repository: &'a Repository, work: &'a PasswordWork) -> PasswordAuthentication<'a, Repository> {
    PasswordAuthentication::new(repository, work)
}

#[tokio::test]
async fn callback_precedence_and_lookup_failure_are_preserved_before_admission() {
    let repository = Repository::default();
    let work = work();
    let occupied = work.begin().unwrap();
    repository.0.lock().unwrap().login = Some(LoginRecord { user: user(), password_hash: "malformed".into(), suspended: false });
    let (account, result) = authentication(&repository, &work)
        .authenticate_resolved(" alice ", PASSWORD.into(), |account| {
            assert_eq!(account, Some(user().id()));
            Err("throttled")
        })
        .await;
    assert_eq!(account, Some(user().id()));
    assert!(matches!(result, Err(AccountError::BeforeVerify("throttled"))));
    assert_eq!(repository.0.lock().unwrap().calls, ["find:alice"]);
    repository.0.lock().unwrap().fail_lookup = true;
    let (account, result) = authentication(&repository, &work)
        .authenticate_resolved("alice", PASSWORD.into(), |_| -> Result<(), ()> { panic!("lookup failure must not call throttle") })
        .await;
    assert_eq!(account, None);
    assert!(matches!(result, Err(AccountError::Store(StoreError("lookup failed")))));
    assert_eq!(work.admitted(), 1);
    drop(occupied);
}

#[tokio::test]
async fn busy_and_closed_do_covering_admission_for_known_missing_and_suspended_shapes() {
    for closed in [false, true] {
        let repository = Repository::default();
        let work = work();
        let occupied = if closed {
            work.close();
            None
        } else {
            Some(work.begin().unwrap())
        };
        for shape in 0..3 {
            repository.0.lock().unwrap().login =
                (shape != 0).then(|| LoginRecord { user: user(), password_hash: "malformed".into(), suspended: shape == 2 });
            let callbacks = Arc::new(AtomicUsize::new(0));
            let observed = Arc::clone(&callbacks);
            let (account, result) = authentication(&repository, &work)
                .authenticate_resolved("alice", PASSWORD.into(), move |_| {
                    observed.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(())
                })
                .await;
            assert_eq!(account, (shape != 0).then(|| user().id()));
            assert_eq!(callbacks.load(Ordering::SeqCst), 1);
            assert!(
                matches!(result, Err(AccountError::Admission(reason)) if reason == if closed { AdmissionError::Closed } else { AdmissionError::Busy })
            );
            assert_eq!(repository.0.lock().unwrap().writes, 0);
        }
        assert_eq!(work.admitted(), usize::from(!closed));
        drop(occupied);
    }
}

#[tokio::test]
async fn oversized_and_unpolled_futures_never_lookup_callback_or_reserve() {
    let repository = Repository::default();
    let work = work();
    let future = authentication(&repository, &work).authenticate_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(()));
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 0);
    drop(future);
    let (account, result) = authentication(&repository, &work)
        .authenticate_resolved("alice", "a".repeat(1025), |_| -> Result<(), ()> { panic!("oversized input must short circuit") })
        .await;
    assert_eq!(account, None);
    assert!(matches!(result, Err(AccountError::InvalidCredentials)));
    assert_eq!(repository.0.lock().unwrap().calls, [] as [String; 0]);
    assert_eq!(work.admitted(), 0);
    work.close();
    let (_, result) =
        authentication(&repository, &work).authenticate_resolved("alice", format!("{}a", "雪".repeat(341)), |_| Ok::<_, ()>(())).await;
    assert!(matches!(result, Err(AccountError::Admission(AdmissionError::Closed))), "1024 bytes must reach lookup/admission");
    assert_eq!(repository.0.lock().unwrap().calls, ["find:alice"]);
}

#[tokio::test]
async fn suspension_is_only_revealed_after_real_password_verification() {
    let repository = Repository::default();
    let work = work();
    let hash = work.begin().unwrap().hash(PASSWORD.into()).await.unwrap();
    repository.0.lock().unwrap().login = Some(LoginRecord { user: user(), password_hash: hash, suspended: true });
    let (_, result) = authentication(&repository, &work).authenticate_resolved("alice", "wrong".into(), |_| Ok::<_, ()>(())).await;
    assert!(matches!(result, Err(AccountError::InvalidCredentials)));
    let (_, result) = authentication(&repository, &work).authenticate_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
    assert!(matches!(result, Err(AccountError::Suspended(name)) if name.as_str() == user().name().as_str()));
    repository.0.lock().unwrap().login.as_mut().unwrap().suspended = false;
    let (_, result) = authentication(&repository, &work).authenticate_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
    assert_eq!(result.unwrap().id(), user().id());
    for stored in [None, Some("corrupt PHC".to_owned())] {
        repository.0.lock().unwrap().login = stored.map(|password_hash| LoginRecord { user: user(), password_hash, suspended: false });
        let (_, result) = authentication(&repository, &work).authenticate_resolved("alice", PASSWORD.into(), |_| Ok::<_, ()>(())).await;
        assert!(matches!(result, Err(AccountError::InvalidCredentials)));
    }
}
