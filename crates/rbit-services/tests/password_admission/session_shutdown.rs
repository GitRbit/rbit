//! Real production delegate: a pre-commit admitted mutation remains owned after close.
use super::*;

#[test]
fn shutdown_waits_for_password_hash_then_late_commit_and_session_revocation() {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1).build().unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new().await;
        let alice = fixture.user("alice", false).await;
        let session = fixture.services.accounts.start_session(&alice).await.unwrap();
        let old_hash = fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash;
        let (release, wait) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
        });
        ready.await.unwrap();
        let profiles = fixture.services.profiles.clone();
        let user = alice.clone();
        let change = tokio::spawn(async move { profiles.change_password(&user, PASSWORD.into(), REPLACEMENT.into()).await });
        tokio::time::timeout(Duration::from_secs(10), async {
            while fixture.passwords.admitted() == 0 { tokio::task::yield_now().await; }
        }).await.unwrap();
        // Verification is queued behind the explicit barrier; SQL has not committed.
        assert_eq!(fixture.services.accounts.session_effect_report().active_owners, 1);
        assert_eq!(fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, old_hash);
        let mut shutdown = Box::pin(fixture.services.shutdown());
        tokio::select! { biased; () = &mut shutdown => panic!("shutdown escaped an admitted pre-commit mutation"), () = tokio::task::yield_now() => {} }
        assert!(fixture.services.accounts.session_effect_report().closed);
        release.send(()).unwrap();
        blocker.await.unwrap();
        change.await.unwrap().unwrap();
        shutdown.await;
        assert!(fixture.services.accounts.session_effect_report().drained());
        assert_eq!(fixture.services.accounts.session_effect_report().abnormal, 0);
        assert_ne!(fixture.database.accounts().find_login("alice").await.unwrap().unwrap().password_hash, old_hash);
        assert!(fixture.services.accounts.resume_session(&session).await.unwrap().is_none());
        unavailable(&fixture.services.profiles.change_password(&alice, REPLACEMENT.into(), PASSWORD.into()).await);
    });
}
