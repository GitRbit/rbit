use super::*;
use crate::{ServiceError, Services};
use crate::{SignUp, accounts::ClientInfo};
use rbit_core::Config;
use rbit_storage::{Database, KvStore};
use std::{sync::Arc, time::Duration};

#[tokio::test]
async fn original_claim_cannot_revive_cancelled_or_expired_pending() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.storage.data_dir = dir.path().to_path_buf();
    let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    let password = "correct horse battery";
    services
        .accounts
        .create_user(SignUp { username: "alice".into(), email: "alice@example.com".into(), password: password.into() }, None)
        .await
        .unwrap();
    for expired in [false, true] {
        let proven = services.accounts.authenticate_original_proof("alice", password.into(), |_| Ok(())).await.1.unwrap();
        let pending = services.two_factor.start_original_password_pending(proven);
        let (_, receipt) = services.two_factor.pending_proof(&pending).unwrap();
        let receipt = receipt.unwrap();
        assert!(services.two_factor.finish_pending(&pending).is_none());
        assert_eq!(services.two_factor.pending_account(&pending), Some(receipt.account()));
        let legacy = services.two_factor.start_pending(receipt.account(), "password");
        assert!(services.two_factor.finish_original_pending(&legacy, &receipt).is_none());
        assert_eq!(services.two_factor.pending_account(&legacy), Some(receipt.account()));
        assert_eq!(services.two_factor.finish_pending(&legacy).as_deref(), Some("password"));
        assert!(services.two_factor.finish_pending(&legacy).is_none());
        if expired {
            services.two_factor.lock_pending().get_mut(&pending.digest()).unwrap().started = Instant::now()
                .checked_sub(PENDING_SIGN_IN_TTL)
                .and_then(|instant| instant.checked_sub(Duration::from_secs(1)))
                .expect("monotonic clock supports expired pending fixture");
        } else {
            services.two_factor.end_pending(&pending);
        }
        assert!(services.two_factor.finish_original_pending(&pending, &receipt).is_none());
        assert!(services.two_factor.pending_account(&pending).is_none());
        assert!(matches!(services.complete_original_sign_in(&pending, "bad", &ClientInfo::default()).await, Err(ServiceError::NotFound)));
    }
    let proven = services.accounts.authenticate_original_proof("alice", password.into(), |_| Ok(())).await.1.unwrap();
    let pending = services.two_factor.start_original_password_pending(proven);
    let (_, receipt) = services.two_factor.pending_proof(&pending).unwrap();
    let receipt = receipt.unwrap();
    assert_eq!(services.two_factor.finish_original_pending(&pending, &receipt).as_deref(), Some("password"));
    assert!(services.two_factor.finish_original_pending(&pending, &receipt).is_none());
    assert!(services.two_factor.finish_pending(&pending).is_none());
}
