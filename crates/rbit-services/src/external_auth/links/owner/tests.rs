//! Owner-only resource tests; synthetic replay data is not authentication authority.
use super::*;
use crate::external_auth::VerifiedIdentity;
use rbit_core::{AccountId, Timestamp};
fn spec() -> SessionPromotion {
    let mut bytes = [7; 64];
    bytes[32..40].copy_from_slice(&1_i64.to_be_bytes());
    bytes[40..48].copy_from_slice(&1_i64.to_be_bytes());
    SessionPromotion::from_storage(AccountId(1), &bytes).unwrap()
}
#[test]
fn pending_and_inflight_share_one_capacity_and_dropped_reservations_release_it() {
    let dir = tempfile::tempdir().unwrap();
    let kv = rbit_storage::KvStore::open(&dir.path().join("sessions.redb")).unwrap();
    let links = OriginalLinks::new(kv.sessions().epoch_sidecar());
    let held: Vec<_> = (0..MAX_PENDING).map(|_| links.reserve().unwrap()).collect();
    assert!(matches!(links.reserve(), Err(ServiceError::Unavailable { .. })));
    drop(held);
    assert_eq!(links.state.lock().unwrap().reserved, 0);
    let flow = Flow::new("github", "/", Some(1), Timestamp::now().seconds());
    links.reserve().unwrap().publish(&flow, spec()).unwrap();
    let held: Vec<_> = (1..MAX_PENDING).map(|_| links.reserve().unwrap()).collect();
    assert!(links.reserve().is_err());
    let debug = format!("{links:?}");
    assert!(!debug.contains(&flow.state));
    drop(held);
    {
        let mut state = links.state.lock().unwrap();
        state.pending.get_mut(&flow.state).unwrap().started = Instant::now().checked_sub(TTL).unwrap();
    }
    // Deterministic expiry, no sleep and no reliance on a background sweep.
    let reservation = links.reserve().unwrap();
    assert_eq!(links.state.lock().unwrap().pending.len(), 0);
    drop(reservation);
}
#[test]
fn nonce_collision_never_replaces_origin_and_scope_cannot_transfer_to_another_owner() {
    let dir = tempfile::tempdir().unwrap();
    let kv = rbit_storage::KvStore::open(&dir.path().join("sessions.redb")).unwrap();
    let links = OriginalLinks::new(kv.sessions().epoch_sidecar());
    let other = OriginalLinks::new(kv.sessions().epoch_sidecar());
    let flow = Flow::new("github", "/", Some(1), Timestamp::now().seconds());
    links.reserve().unwrap().publish(&flow, spec()).unwrap();
    assert!(links.reserve().unwrap().publish(&flow, spec()).is_err());
    assert_eq!(links.state.lock().unwrap().reserved, 0);
    let callback = Callback {
        identity: VerifiedIdentity {
            provider_id: "github".into(),
            subject: "subject".into(),
            email: None,
            email_verified: false,
            username: None,
            display_name: None,
            mfa: false,
        },
        next: "/".into(),
        link_account: Some(AccountId(1)),
    };
    let context = links.verified(&flow.state, callback).unwrap();
    assert!(other.claim(context).is_err());
    assert!(links.validate_callback(&flow.state, "github").is_ok(), "foreign owner cannot consume the original context");
    assert!(links.validate_callback(&flow.state, "other").is_err());
}
