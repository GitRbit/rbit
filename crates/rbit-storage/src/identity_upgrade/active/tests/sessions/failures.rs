use super::*;

#[tokio::test]
async fn witness_failure_rolls_back_each_real_mutation_not_only_a_standalone_counter() {
    for operation in ["put", "renew", "delete", "others", "logout", "purge", "apply"] {
        let f = RuntimeFixture::new().await;
        let store = f.epoch();
        let original = record(if operation == "purge" { -3600 } else { 3600 });
        // Explicit corruption injection: MAX cannot be issued by the checked
        // global allocator. The shared bodies must roll back payload/index changes
        // when their same-transaction witness refuses this exhausted authority.
        let snapshot = f.unfenced_for_fault().put_bound([1; 32], original.clone(), AccountId(1), epoch(i64::MAX)).await.unwrap().unwrap();
        let second = put(&f.unfenced_for_fault(), 2, i64::MAX).await;
        let before = f.floor();
        match operation {
            "put" => assert!(store.put_bound([3; 32], record(3600), AccountId(1), epoch(i64::MAX)).await.is_err()),
            "renew" => assert!(
                store.renew(&snapshot, Timestamp(original.expires_at.0 + 100), Timestamp(original.last_seen_at.0 + 100)).await.is_err()
            ),
            "delete" => assert!(store.delete(&snapshot).await.is_err()),
            "others" => assert!(store.revoke_others(&snapshot).await.is_err()),
            "logout" => assert!(store.logout(&snapshot).await.is_err()),
            "purge" => assert!(store.purge_expired_bound().await.is_err()),
            "apply" => {
                let pair = store.pair_identity().await.unwrap().unwrap();
                assert!(store.apply_pending_revocation(request(i64::MAX, Decision::EndAll), pair).await.is_err());
            }
            _ => unreachable!(),
        }
        assert_eq!(f.floor(), before);
        assert_eq!(store.get_snapshot([1; 32]).await.unwrap().unwrap().record(), &original);
        assert_eq!(store.get_snapshot([2; 32]).await.unwrap().unwrap().record(), second.record());
        assert!(store.get_snapshot([3; 32]).await.unwrap().is_none());
    }
}

#[tokio::test]
async fn missing_floor_or_changed_publication_rolls_back_without_repair() {
    for missing in [false, true] {
        let f = RuntimeFixture::new().await;
        let store = f.epoch();
        let snapshot = put(&store, 1, 1).await;
        {
            let tx = f.accepted.owner.db.begin_write().unwrap();
            if missing {
                tx.delete_table(runtime::FLOOR).unwrap();
            } else {
                let mut bytes = f.accepted.checkpoint.encode();
                bytes[8] ^= 1;
                tx.open_table(runtime::ACTIVE).unwrap().insert(0, &bytes).unwrap();
            }
            tx.commit().unwrap();
        }
        assert!(store.delete(&snapshot).await.is_err());
        assert_eq!(store.get_snapshot([1; 32]).await.unwrap().unwrap().record(), snapshot.record());
        if missing {
            assert_eq!(runtime::read(&f.accepted.owner.db).unwrap(), None);
        }
    }
}
