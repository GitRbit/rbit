use rbit_identity::sessions::effects::{SessionEffectScope, SessionEffectsError};
use tokio::sync::oneshot;

struct Destroyed(Option<oneshot::Sender<()>>);
impl Drop for Destroyed {
    fn drop(&mut self) {
        if let Some(signal) = self.0.take() {
            let _ = signal.send(());
        }
    }
}

#[tokio::test]
async fn close_waits_for_held_permit_late_adoption_and_real_future_destruction() {
    let scope = SessionEffectScope::new();
    let permit = scope.reserve().unwrap();
    let mut draining = Box::pin(scope.shutdown());
    tokio::select! { biased; _ = &mut draining => panic!("admitted lease must prevent zero"), () = tokio::task::yield_now() => {} }
    assert!(matches!(scope.reserve(), Err(SessionEffectsError::Closed)));
    let (began, started) = oneshot::channel();
    let (continue_work, ready) = oneshot::channel();
    let (destroyed, mut destruction) = oneshot::channel();
    let marker = Destroyed(Some(destroyed));
    let task = permit
        .submit(move |_context| async move {
            let _marker = marker;
            began.send(()).unwrap();
            ready.await.unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    started.await.unwrap();
    assert!(!scope.report().drained());
    // Cancel the drain waiter, not the owned worker; a later wait resumes ownership.
    drop(draining);
    drop(task);
    continue_work.send(()).unwrap();
    let report = scope.shutdown().await;
    assert!(matches!(destruction.try_recv(), Ok(())), "drain must include future destruction");
    assert!(report.drained());
    assert_eq!(report.completed, 1);
    assert_eq!(report.abnormal, 0);
}

#[test]
fn no_runtime_is_truthful_and_releases_unsubmitted_ownership() {
    let scope = SessionEffectScope::new();
    let result = scope.reserve().unwrap().submit(|_context| async { Ok::<_, ()>(()) });
    assert!(matches!(result, Err(SessionEffectsError::NoRuntime)));
    scope.close();
    let report = scope.report();
    assert!(report.drained());
    assert_eq!(report.abnormal, 1);
    assert_eq!(report.completed, 0);
}

#[test]
fn runtime_drop_of_an_unpolled_submitted_task_is_not_reported_as_completion() {
    let scope = SessionEffectScope::new();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let entered = runtime.enter();
    let task = scope
        .reserve()
        .unwrap()
        .submit(|_context| async {
            std::future::pending::<()>().await;
            Ok::<_, ()>(())
        })
        .unwrap();
    scope.close();
    drop(entered);
    drop(runtime);
    drop(task);
    let report = scope.report();
    assert!(report.drained());
    assert_eq!(report.abnormal, 1);
    assert_eq!(report.completed, 0);
}

async fn panicking_update() -> Result<(), ()> {
    tokio::task::yield_now().await;
    panic!("deliberate finalization fault");
}
#[tokio::test]
async fn returned_errors_and_panics_are_distinct_truthful_outcomes() {
    let scope = SessionEffectScope::new();
    assert!(scope.reserve().unwrap().submit(|_context| async { Err::<(), _>("store failed") }).unwrap().await.unwrap().is_err());
    assert!(scope.reserve().unwrap().submit(|_context| panicking_update()).unwrap().await.unwrap_err().is_panic());
    let report = scope.shutdown().await;
    assert!(report.drained());
    assert_eq!(report.completed, 1);
    assert_eq!(report.returned_errors, 1);
    assert_eq!(report.abnormal, 1);
}
