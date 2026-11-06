use rbit_identity::sessions::effects::SessionEffectScope;

#[tokio::test]
async fn admitted_completed_value_survives_close_and_detached_waiter_until_real_completion() {
    let scope = SessionEffectScope::new();
    let permit = scope.reserve().unwrap();
    scope.close();
    let (started, start) = tokio::sync::oneshot::channel();
    let (release, resume) = tokio::sync::oneshot::channel();
    let handle = permit
        .submit_value(move |_context| async move {
            started.send(()).unwrap();
            resume.await.unwrap();
            Ok::<usize, ()>(42)
        })
        .unwrap();
    start.await.unwrap();
    assert!(!scope.report().drained());
    drop(handle);
    assert!(!scope.report().drained());
    release.send(()).unwrap();
    let report = scope.shutdown().await;
    assert!(report.drained());
    assert_eq!(report.completed, 1);
    assert_eq!(report.returned_errors, 0);
    assert_eq!(report.abnormal, 0);
}

#[tokio::test]
async fn completed_value_and_returned_error_use_the_same_existing_accounting() {
    let scope = SessionEffectScope::new();
    let permit = scope.reserve().unwrap();
    assert_eq!(permit.submit_value(|_| async { Ok::<usize, ()>(7) }).unwrap().await.unwrap(), Ok(7));
    let permit = scope.reserve().unwrap();
    assert_eq!(permit.submit_value(|_| async { Err::<usize, ()>(()) }).unwrap().await.unwrap(), Err(()));
    let report = scope.shutdown().await;
    assert!(report.drained());
    assert_eq!(report.completed, 2);
    assert_eq!(report.returned_errors, 1);
    assert_eq!(report.abnormal, 0);
}
