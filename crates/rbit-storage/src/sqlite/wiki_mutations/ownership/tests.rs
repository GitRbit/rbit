use super::*;

#[tokio::test]
async fn ownership_does_not_park_the_only_pooled_connection() {
    let (_directory, original) = crate::sqlite::plans::database().await;
    let options = original.pool.connect_options().as_ref().clone();
    let key = original.key.clone();
    original.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    let database = crate::sqlite::Database { pool, key, protocol: crate::sqlite::protocol::SqlProtocol::legacy() };
    let lease = database.wiki_catalog_lease().await.unwrap();
    let identity = tokio::time::timeout(std::time::Duration::from_secs(2), database.wikis().identity()).await.unwrap().unwrap();
    assert_eq!(lease.store_id(), identity);
    assert_eq!(format!("{lease:?}"), "WikiCatalogLease(..)");
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    drop(lease);
    database.wiki_catalog_lease().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn hardlinked_catalog_is_refused_before_creating_a_lease_file() {
    let (directory, database) = crate::sqlite::plans::database().await;
    std::fs::hard_link(directory.path().join("rbit.db"), directory.path().join("alias.db")).unwrap();
    assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Aliased)));
    assert!(!directory.path().join("rbit.db.wiki-writer-v1.lock").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn observed_subject_replacement_is_refused_without_adopting_the_new_file() {
    let (directory, database) = crate::sqlite::plans::database().await;
    let lease = database.wiki_catalog_lease().await.unwrap();
    let path = directory.path().join("rbit.db");
    std::fs::rename(&path, directory.path().join("original.db")).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    assert!(matches!(lease.check_bound(), Err(WikiCatalogLeaseError::Changed)));
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
}

#[cfg(windows)]
#[tokio::test]
async fn held_subject_prevents_rename_even_after_the_sql_pool_closes() {
    let (directory, database) = crate::sqlite::plans::database().await;
    let lease = database.wiki_catalog_lease().await.unwrap();
    database.close().await;
    let path = directory.path().join("rbit.db");
    let moved = directory.path().join("moved.db");
    assert!(std::fs::rename(&path, &moved).is_err());
    drop(lease);
    std::fs::rename(&path, &moved).unwrap();
}
