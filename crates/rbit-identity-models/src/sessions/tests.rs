//! Fixed wire vectors prove that session models need no token/crypto/runtime feature.

use super::{ActiveSession, SessionRecord, StoredSession, session_id};
use rbit_contracts::{AccountId, Timestamp};

fn record() -> SessionRecord {
    SessionRecord {
        account_id: AccountId(7),
        expires_at: Timestamp(100),
        created_at: Timestamp(90),
        last_seen_at: Timestamp(95),
        ip_address: None,
        user_agent: "x".into(),
    }
}

#[test]
fn session_field_order_preserves_exact_postcard_and_json_representation() {
    let record = record();
    let bytes = [14, 200, 1, 180, 1, 190, 1, 0, 1, 120];
    assert_eq!(postcard::to_allocvec(&record).unwrap(), bytes);
    assert_eq!(postcard::from_bytes::<SessionRecord>(&bytes).unwrap(), record);
    assert_eq!(
        serde_json::to_string(&record).unwrap(),
        r#"{"account_id":7,"expires_at":100,"created_at":90,"last_seen_at":95,"ip_address":null,"user_agent":"x"}"#
    );
    assert!(postcard::from_bytes::<SessionRecord>(&[0xff; 3]).is_err());
}

#[test]
fn user_agent_bound_is_characters_and_constructor_keeps_supplied_expiry() {
    let before = Timestamp::now();
    let record = SessionRecord::new(AccountId(-1), Timestamp(100), Some("192.0.2.1".into()), &"雪".repeat(300));
    let after = Timestamp::now();
    assert_eq!(record.user_agent.chars().count(), 256);
    assert_eq!(record.user_agent.len(), 768);
    assert!(before <= record.created_at && record.created_at <= after);
    assert_eq!(record.last_seen_at, record.created_at);
    assert_eq!(record.expires_at, Timestamp(100));
    assert!(record.is_expired());
}

#[test]
fn public_id_and_current_session_projection_preserve_owner_display_contract() {
    let digest = [0xab; 32];
    assert_eq!(session_id(&digest), "ab".repeat(12));
    let stored = StoredSession { digest, record: record() };
    let current = ActiveSession::from_stored(stored.clone(), Some(&digest));
    assert!(current.is_current);
    assert_eq!(current.id, "ab".repeat(12));
    assert_eq!(current.created_at, Timestamp(90));
    assert_eq!(current.last_seen_at, Timestamp(95));
    assert_eq!(current.user_agent, "x");
    assert!(!ActiveSession::from_stored(stored, Some(&[0; 32])).is_current);
}
