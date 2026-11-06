#[path = "session_application/fixture.rs"]
mod fixture;
use fixture::{Clock, policy, record, store, user};
use rbit_contracts::Timestamp;
use rbit_identity::{
    SessionToken,
    sessions::{ClientInfo, SessionApplication, SessionError, SessionPolicy, StoredSession, session_id},
};

#[tokio::test]
async fn start_preserves_clock_sampling_cap_canonical_ip_and_unicode_limit() {
    let store = store();
    let app = SessionApplication::with_clock(&store, &store, SessionPolicy { max_age_days: 1, ..policy() }, Clock::at(&[10, 20, 30]));
    let token =
        app.start(&user(), &ClientInfo { ip: Some("::ffff:192.0.2.1".parse().unwrap()), user_agent: "界".repeat(300) }).await.unwrap();
    assert!(SessionToken::parse(&token.encode()).is_some());
    let s = store.0.lock().unwrap();
    let record = s.record.as_ref().unwrap();
    assert_eq!(record.expires_at, Timestamp(10 + 86_400));
    assert_eq!((record.created_at, record.last_seen_at), (Timestamp(30), Timestamp(30)));
    assert_eq!(record.ip_address.as_deref(), Some("192.0.2.1"));
    assert_eq!(record.user_agent, "界".repeat(256));
    assert_eq!(s.calls, ["put"]);
}

#[tokio::test]
async fn resume_reads_one_projection_without_write_before_seen_threshold() {
    let store = store();
    let app = SessionApplication::with_clock(&store, &store, policy(), Clock::at(&[1299, 1299]));
    let resolved = app.resume(&SessionToken::generate()).await.unwrap().unwrap();
    assert!(resolved.password_change_required && resolved.has_unread_notifications);
    assert_eq!(store.0.lock().unwrap().calls, ["get", "user:7:true"]);
}

#[tokio::test]
async fn seen_threshold_and_lost_atomic_renewal_do_not_resurrect_snapshot() {
    let store = store();
    store.0.lock().unwrap().renewal_survives = false;
    let app = SessionApplication::with_clock(&store, &store, policy(), Clock::at(&[1300, 1300]));
    assert!(app.resume(&SessionToken::generate()).await.unwrap().is_none());
    let s = store.0.lock().unwrap();
    assert_eq!(s.calls, ["get", "user:7:true", "renew"]);
    assert_eq!(s.renewed, Some((Timestamp(174_100), Timestamp(1300))));
}

#[tokio::test]
async fn absolute_age_equality_expires_before_account_query_and_missing_user_deletes() {
    let store = store();
    let app = SessionApplication::with_clock(&store, &store, SessionPolicy { max_age_days: 1, ..policy() }, Clock::at(&[86400]));
    assert!(app.resume(&SessionToken::generate()).await.unwrap().is_none());
    assert_eq!(store.0.lock().unwrap().calls, ["get", "delete"]);
    let store = fixture::store();
    store.0.lock().unwrap().user = None;
    let app = SessionApplication::with_clock(&store, &store, SessionPolicy { notifications: false, ..policy() }, Clock::at(&[1001]));
    assert!(app.resume(&SessionToken::generate()).await.unwrap().is_none());
    assert_eq!(store.0.lock().unwrap().calls, ["get", "user:7:false", "delete"]);
}

#[tokio::test]
async fn expiry_renewal_requires_strictly_more_than_one_day() {
    for (difference, expected) in [(86400, false), (86401, true)] {
        let store = store();
        store.0.lock().unwrap().record.as_mut().unwrap().expires_at = Timestamp(1001 + 172_800 - difference);
        let app = SessionApplication::with_clock(&store, &store, policy(), Clock::at(&[1001, 1001]));
        assert!(app.resume(&SessionToken::generate()).await.unwrap().is_some());
        assert_eq!(store.0.lock().unwrap().renewed.is_some(), expected);
    }
}

#[tokio::test]
async fn list_and_revocation_preserve_owner_current_marker_and_not_found_boundary() {
    let store = store();
    let current = SessionToken::generate();
    store.0.lock().unwrap().listed = vec![StoredSession { digest: current.digest(), record: record() }];
    let app = SessionApplication::with_clock(&store, &store, policy(), Clock::at(&[]));
    let listed = app.list(&user(), Some(&current)).await.unwrap();
    assert!(listed[0].is_current);
    assert!(matches!(app.revoke(&user(), "unowned").await, Err(SessionError::NotFound)));
    assert_eq!(store.0.lock().unwrap().calls, ["list:7", "list:7"]);
    assert_eq!(app.revoke(&user(), &session_id(&current.digest())).await.unwrap(), record());
    assert_eq!(app.revoke_others(user().id(), &current).await.unwrap(), 3);
    assert_eq!(store.0.lock().unwrap().kept, Some(current.digest()));
    app.end(&current).await.unwrap();
}

#[tokio::test]
async fn missing_session_never_queries_accounts_or_samples_clock() {
    let store = store();
    store.0.lock().unwrap().record = None;
    let app = SessionApplication::with_clock(&store, &store, policy(), Clock::at(&[]));
    assert!(app.resume(&SessionToken::generate()).await.unwrap().is_none());
    assert_eq!(store.0.lock().unwrap().calls, ["get"]);
}
