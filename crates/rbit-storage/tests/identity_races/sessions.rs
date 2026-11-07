//! Session renewal racing revocation: a renewal may win or lose, but once every contender
//! has finished, a revoked session never exists again, in either table.

use rbit_core::{AccountId, Timestamp};
use rbit_storage::{KvStore, SessionRecord, SessionStore};

use super::{ROUNDS, race};

/// Renewals released alongside each revocation.
const RENEWALS: usize = 7;

fn sessions(dir: &tempfile::TempDir) -> SessionStore {
    KvStore::open(&dir.path().join("kv.redb")).unwrap().sessions()
}

fn live(account: i64) -> SessionRecord {
    SessionRecord::new(AccountId(account), Timestamp(Timestamp::now().0 + 3600), Some("192.0.2.1".into()), "race")
}

fn digest(round: u8, slot: u8) -> [u8; 32] {
    let mut digest = [slot; 32];
    digest[0] = round;
    digest
}

/// What a renewal from a request that read the session just before revocation writes.
async fn renew(store: SessionStore, digest: [u8; 32]) -> bool {
    let now = Timestamp::now();
    store.renew(digest, Timestamp(now.0 + 30 * 24 * 3600), now).await.unwrap()
}

async fn assert_gone(store: &SessionStore, account: i64, digest: [u8; 32]) {
    assert!(store.get(digest).await.unwrap().is_none(), "a renewal resurrected a revoked session");
    assert!(store.list_for_account(AccountId(account)).await.unwrap().iter().all(|stored| stored.digest != digest));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn renewals_racing_account_wide_revocation_never_resurrect_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = sessions(&dir);
    let bystander = digest(0, 0xee);
    store.put(bystander, live(8)).await.unwrap();
    for round in 0..ROUNDS {
        let target = digest(round, 1);
        store.put(target, live(7)).await.unwrap();
        let outcomes = race(RENEWALS + 1, |index| {
            let store = store.clone();
            async move {
                if index == 0 {
                    usize::from(store.delete_for_account(AccountId(7), None).await.unwrap() == 1)
                } else {
                    usize::from(renew(store, target).await)
                }
            }
        })
        .await;
        assert_eq!(outcomes[0], 1, "the revocation removed exactly the live session");
        assert_gone(&store, 7, target).await;
        // No index entry survived either: a second sweep finds nothing to remove.
        assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 0);
    }
    assert!(store.get(bystander).await.unwrap().is_some(), "another account's session is untouched");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn renewals_racing_single_revocation_never_resurrect_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = sessions(&dir);
    for round in 0..ROUNDS {
        let target = digest(round, 2);
        store.put(target, live(7)).await.unwrap();
        race(RENEWALS + 1, |index| {
            let store = store.clone();
            async move {
                if index == 0 {
                    store.delete(target).await.unwrap();
                } else {
                    renew(store, target).await;
                }
            }
        })
        .await;
        assert_gone(&store, 7, target).await;
        assert!(!renew(store.clone(), target).await, "a later renewal is refused too");
    }
    assert_eq!(store.delete_for_account(AccountId(7), None).await.unwrap(), 0, "no index entry outlived its session");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn revoking_other_sessions_keeps_only_the_current_one_under_renewal() {
    let dir = tempfile::tempdir().unwrap();
    let store = sessions(&dir);
    for round in 0..ROUNDS {
        let (current, others) = (digest(round, 3), [digest(round, 4), digest(round, 5)]);
        for session in [current, others[0], others[1]] {
            store.put(session, live(7)).await.unwrap();
        }
        let outcomes = race(RENEWALS + 1, |index| {
            let store = store.clone();
            let renewed = if index % 3 == 0 { current } else { others[index % 2] };
            async move {
                if index == 0 {
                    store.delete_for_account(AccountId(7), Some(current)).await.unwrap() == 2
                } else {
                    renew(store, renewed).await
                }
            }
        })
        .await;
        assert!(outcomes[0], "exactly the two other sessions were revoked");
        for other in others {
            assert_gone(&store, 7, other).await;
        }
        let kept: Vec<_> = store.list_for_account(AccountId(7)).await.unwrap().into_iter().map(|stored| stored.digest).collect();
        assert_eq!(kept, [current], "the current session survives with its index entry");
        store.delete(current).await.unwrap();
    }
}
