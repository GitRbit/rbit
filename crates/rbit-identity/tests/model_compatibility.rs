//! Legacy identity module paths remain aliases even with runtime features selected.

use std::any::TypeId;

use rbit_identity as legacy;
use rbit_identity_models as models;

#[test]
fn every_public_model_path_remains_the_exact_same_type() {
    macro_rules! same {
        ($left:ty, $right:ty) => {
            assert_eq!(TypeId::of::<$left>(), TypeId::of::<$right>());
        };
    }
    same!(legacy::Account, models::Account);
    same!(legacy::account::User, models::User);
    same!(legacy::AccountKind, models::account::AccountKind);
    same!(legacy::AccountId, models::AccountId);
    same!(legacy::AccountName, models::AccountName);
    same!(legacy::names::Email, models::Email);
    same!(legacy::TokenDigest, models::TokenDigest);
    same!(legacy::token::TokenDigest, models::token::TokenDigest);
    same!(legacy::accounts::SignUp, models::accounts::SignUp);
    same!(legacy::accounts::NewUser, models::accounts::NewUser);
    same!(legacy::accounts::LoginRecord, models::accounts::LoginRecord);
    same!(legacy::sessions::SessionRecord, models::sessions::SessionRecord);
    same!(legacy::sessions::StoredSession, models::sessions::StoredSession);
    same!(legacy::sessions::SessionUser, models::sessions::SessionUser);
    same!(legacy::sessions::ClientInfo, models::sessions::ClientInfo);
    same!(legacy::sessions::ActiveSession, models::sessions::ActiveSession);
}

#[test]
fn legacy_session_paths_keep_fixed_wire_bytes_and_deterministic_application_clock() {
    use rbit_contracts::{AccountId, Timestamp};
    let record = models::sessions::SessionRecord::at(AccountId(7), Timestamp(100), None, "x", Timestamp(90));
    assert_eq!(record.last_seen_at, Timestamp(90));
    let legacy: legacy::sessions::SessionRecord = record;
    let bytes = [14, 200, 1, 180, 1, 180, 1, 0, 1, 120];
    assert_eq!(postcard::to_allocvec(&legacy).unwrap(), bytes);
    assert_eq!(postcard::from_bytes::<models::sessions::SessionRecord>(&bytes).unwrap(), legacy);
    assert_eq!(
        serde_json::to_value(&legacy).unwrap(),
        serde_json::json!({
            "account_id":7,"expires_at":100,"created_at":90,"last_seen_at":90,"ip_address":null,"user_agent":"x",
        })
    );
    let digest: legacy::TokenDigest = [0xab; 32];
    assert_eq!(legacy::sessions::session_id(&digest), models::sessions::session_id(&digest));
}
