//! Private connection-local Wiki compatibility; never a public default capability.
use sqlx::sqlite::SqliteConnectOptions;
use std::cmp::Ordering;

const NAME: &str = "rbit_wiki_writer_v1";

fn legacy_key(value: &str) -> &str {
    if matches!(value, "legacy" | "permit") { "legacy" } else { value }
}
fn private_key(value: &str) -> &str {
    if matches!(value, "legacy" | "preparing" | "managed" | "permit") { "legacy" } else { value }
}
// Both are well-formed equivalence-class collations, not nonreflexive comparisons.
fn legacy(left: &str, right: &str) -> Ordering {
    legacy_key(left).cmp(legacy_key(right))
}
fn private(left: &str, right: &str) -> Ordering {
    private_key(left).cmp(private_key(right))
}

pub(crate) fn legacy_options(options: SqliteConnectOptions) -> SqliteConnectOptions {
    options.collation(NAME, legacy)
}
pub(super) fn owned_options<K: Send + Sync + 'static>(options: SqliteConnectOptions, objects: K) -> SqliteConnectOptions {
    options.collation(NAME, move |left, right| {
        // Lifetime only: this particular registration NEVER changes ordering.
        // The actual root is retained until accounted physical connection close.
        let _objects = &objects;
        private(left, right)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_marker_does_not_grant_managed_writes_and_comparisons_remain_reflexive() {
        for mode in ["legacy", "preparing", "managed", "permit", "other"] {
            assert_eq!(legacy(mode, mode), Ordering::Equal);
            assert_eq!(private(mode, mode), Ordering::Equal);
        }
        assert_eq!(legacy("legacy", "permit"), Ordering::Equal);
        assert_ne!(legacy("preparing", "permit"), Ordering::Equal);
        assert_ne!(legacy("managed", "permit"), Ordering::Equal);
        assert_eq!(private("managed", "permit"), Ordering::Equal);
        assert_ne!(private("other", "permit"), Ordering::Equal);
    }

    #[test]
    fn each_registered_comparator_is_a_total_preorder_for_mode_and_foreign_inputs() {
        let values = ["", "legacy", "preparing", "managed", "permit", "other", "\0", "é"];
        for compare in [legacy as fn(&str, &str) -> Ordering, private] {
            for a in values {
                for b in values {
                    assert_eq!(compare(a, b), compare(b, a).reverse());
                    for c in values {
                        if compare(a, b) == Ordering::Equal && compare(b, c) == Ordering::Equal {
                            assert_eq!(compare(a, c), Ordering::Equal);
                        }
                        if compare(a, b) == Ordering::Less && compare(b, c) == Ordering::Less {
                            assert_eq!(compare(a, c), Ordering::Less);
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn already_cached_unregistered_writer_refuses_after_protocol_schema_arrives() {
        use sqlx::Connection;
        const CACHED: &str = "UPDATE wiki_store_identity SET id=id WHERE id=1";
        // Mechanism fixture only: exact production0069 is applied to minimal
        // protected table shapes on the SAME old connection, avoiding external-DDL
        // pool races. Full migrated catalogs are exercised by the recovery tests.
        let mut old = sqlx::SqliteConnection::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE _sqlx_migrations(version INTEGER PRIMARY KEY,success INTEGER); INSERT INTO _sqlx_migrations VALUES(66,1),(68,1);",
        )
        .execute(&mut old)
        .await
        .unwrap();
        for table in [
            "wiki_store_identity",
            "wiki_repositories",
            "wiki_revisions",
            "wiki_pages",
            "wiki_page_revisions",
            "wiki_actor_incarnations",
            "wiki_page_allocations",
            "wiki_mutation_intents",
        ] {
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY); INSERT INTO {table} VALUES(1);")))
                .execute(&mut old)
                .await
                .unwrap();
        }
        sqlx::query(CACHED).execute(&mut old).await.unwrap();
        assert_ne!(old.cached_statements_size(), 0);
        sqlx::raw_sql(include_str!("../../../migrations/0069_wiki_writer_protocol.sql")).execute(&mut old).await.unwrap();
        let error = sqlx::query(CACHED).execute(&mut old).await.unwrap_err();
        assert!(error.as_database_error().unwrap().message().contains("rbit_wiki_writer_v1"));
        old.close().await.unwrap();
    }
}
