use super::*;
use rbit_identity_models::sessions::{
    SessionPairIdentity, SessionRevocationDecision as Decision, SessionRevocationOutcome as Outcome, SessionRevocationRequest,
};

async fn bound() -> (tempfile::TempDir, EpochSessionStore, SessionPairIdentity, EpochSnapshot) {
    let (dir, _legacy, store) = store();
    let pair = store.initialize_identity_if_empty().await.unwrap();
    let current = store.put_bound([1; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap().unwrap();
    store.put_bound([2; 32], record(7, 3600), AccountId(7), epoch(1)).await.unwrap();
    (dir, store, pair, current)
}
fn request(current: &EpochSnapshot) -> SessionRevocationRequest {
    SessionRevocationRequest::from_storage(1, AccountId(7), epoch(2), Decision::KeepCurrent(current.promotion_spec().unwrap())).unwrap()
}

#[tokio::test]
async fn same_incarnation_replay_preserves_wire_metadata_and_exact_fixed_epoch() {
    let (_dir, store, pair, current) = bound().await;
    let encoded = encode_session(current.record()).unwrap();
    let spec = current.promotion_spec().unwrap();
    let request = request(&current);
    let first = store.apply_pending_revocation(request, pair).await.unwrap();
    assert_eq!(first.outcome(), Outcome::KeptCurrent);
    let again = store.apply_pending_revocation(request, pair).await.unwrap();
    assert_eq!(again.outcome(), Outcome::KeptCurrent);
    let promoted = store.get_live([1; 32]).await.unwrap().unwrap();
    assert_eq!(promoted.epoch(), epoch(2));
    assert_eq!(encode_session(promoted.record()).unwrap(), encoded);
    assert_eq!(&promoted.promotion_spec().unwrap().storage_bytes()[48..], &spec.storage_bytes()[48..]);
    assert!(store.get_snapshot([2; 32]).await.unwrap().is_none());
    assert!(!store.delete(&current).await.unwrap());
    assert!(store.logout(&current).await.unwrap());
}

#[tokio::test]
async fn invalid_pair_or_cross_account_index_cannot_commit_partial_revocation() {
    let (_dir, store, pair, current) = bound().await;
    let request = request(&current);
    let mut wrong = pair.storage_bytes();
    wrong[0] ^= 1;
    assert!(store.apply_pending_revocation(request, SessionPairIdentity::from_storage(&wrong).unwrap()).await.is_err());
    assert!(store.get_live([2; 32]).await.unwrap().is_some());
    store.put_bound([3; 32], record(8, 3600), AccountId(8), epoch(1)).await.unwrap();
    let tx = store.db.begin_write().unwrap();
    tx.open_table(ACCOUNT_SESSIONS).unwrap().insert((7, &[3; 32]), ()).unwrap();
    tx.commit().unwrap();
    assert!(store.apply_pending_revocation(request, pair).await.is_err());
    assert_eq!(store.get_live([1; 32]).await.unwrap().unwrap().epoch(), epoch(1));
    assert!(store.get_live([2; 32]).await.unwrap().is_some(), "earlier removal rolled back");
    assert!(store.get_live([3; 32]).await.unwrap().is_some(), "other account untouched");
}

#[tokio::test]
async fn identity_initialization_refuses_records_or_orphans_without_purging_or_adopting() {
    for kind in ["record", "index", "binding", "legacy"] {
        let (_dir, legacy, store) = store();
        if kind == "record" {
            legacy.put([1; 32], record(7, 3600)).await.unwrap();
        } else {
            let tx = store.db.begin_write().unwrap();
            match kind {
                "index" => {
                    tx.open_table(ACCOUNT_SESSIONS).unwrap().insert((7, &[1; 32]), ()).unwrap();
                }
                "binding" => {
                    let binding = Binding::with_random(AccountId(7), epoch(1), |bytes| {
                        bytes.fill(1);
                        Ok(())
                    })
                    .unwrap();
                    tx.open_table(EPOCHS).unwrap().insert(&[1; 32], &binding.encode()).unwrap();
                }
                "legacy" => {
                    tx.open_table(super::super::super::LEGACY_SESSIONS).unwrap().insert(&[1; 32], &[1; 16]).unwrap();
                }
                _ => unreachable!(),
            }
            tx.commit().unwrap();
        }
        assert!(store.initialize_identity_if_empty().await.is_err(), "{kind}");
        assert!(store.pair_identity().await.unwrap().is_none(), "no identity published after {kind}");
        if kind == "record" {
            assert!(legacy.get([1; 32]).await.unwrap().is_some());
        }
    }
}

#[tokio::test]
async fn identity_is_stable_and_missing_or_malformed_state_never_repairs_itself() {
    let (_dir, _legacy, mut store) = store();
    store.random = |_| Err(());
    assert!(store.initialize_identity_if_empty().await.is_err());
    assert!(store.pair_identity().await.unwrap().is_none());
    store.random = |bytes| getrandom::fill(bytes).map_err(|_| ());
    let pair = store.initialize_identity_if_empty().await.unwrap();
    store.random = |_| Err(());
    assert_eq!(store.initialize_identity_if_empty().await.unwrap(), pair, "existing identity needs no new entropy");
    let tx = store.db.begin_write().unwrap();
    tx.open_table(super::super::identity::PAIR).unwrap().insert(1, &[9; 16]).unwrap();
    tx.commit().unwrap();
    assert!(store.pair_identity().await.is_err());
    assert!(store.initialize_identity_if_empty().await.is_err());
}
