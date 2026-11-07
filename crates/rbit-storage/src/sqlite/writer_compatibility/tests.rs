//! Finite adversarial vectors complement the canonical-key ordering construction.
use super::*;
use std::cmp::Ordering::{Equal, Greater, Less};
const VALUES: &[&str] = &[
    "",
    "\0",
    "a",
    "v2",
    "v3",
    "é",
    "שלום",
    "🦀",
    "rbit-fenced-v2/accepted",
    "rbit-fenced-v2/requested",
    "rbit-active-v3/accepted",
    "rbit-active-v3/requested",
    "rbit-fenced-v2/accepted\0",
    "rbit-fenced-v2/requested-more",
];
#[test]
fn capability_comparisons_obey_reflexivity_symmetry_transitivity_and_distinct_scopes() {
    for compare in [fenced_comparison, active_comparison] {
        for &a in VALUES {
            assert_eq!(compare(a, a), Equal);
            for &b in VALUES {
                assert_eq!(compare(a, b), compare(b, a).reverse());
                for &c in VALUES {
                    if compare(a, b) == Equal && compare(b, c) == Equal {
                        assert_eq!(compare(a, c), Equal);
                    }
                    if compare(a, b) == Less && compare(b, c) == Less {
                        assert_eq!(compare(a, c), Less);
                    }
                    if compare(a, b) == Greater && compare(b, c) == Greater {
                        assert_eq!(compare(a, c), Greater);
                    }
                }
            }
        }
    }
    assert_eq!(fenced_comparison("rbit-fenced-v2/requested", "rbit-fenced-v2/accepted"), Equal);
    assert_eq!(active_comparison("rbit-active-v3/requested", "rbit-active-v3/accepted"), Equal);
    assert_ne!(fenced_comparison("rbit-active-v3/requested", "rbit-active-v3/accepted"), Equal);
    assert_ne!(active_comparison("rbit-fenced-v2/requested", "rbit-fenced-v2/accepted"), Equal);
}
#[tokio::test]
async fn actual_sqlite_default_and_stopped_markers_are_valid_orderings_with_distinct_authority() {
    use sqlx::{Connection, SqliteConnection};
    for stopped in [false, true] {
        let options = sqlx::sqlite::SqliteConnectOptions::new().in_memory(true).pragma("trusted_schema", "OFF");
        let options = if stopped { register_stopped(options) } else { register(options) };
        let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
        let v2: bool =
            sqlx::query_scalar("SELECT 'rbit-fenced-v2/requested' COLLATE rbit_credential_fenced_v2 = 'rbit-fenced-v2/accepted'")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        let v3: bool =
            sqlx::query_scalar("SELECT 'rbit-active-v3/requested' COLLATE rbit_credential_active_v3 = 'rbit-active-v3/accepted'")
                .fetch_one(&mut connection)
                .await
                .unwrap();
        assert_eq!(v2, stopped);
        assert!(!v3, "stopped authority cannot grant ACTIVE authority");
        let reflexive: bool =
            sqlx::query_scalar("SELECT 'unknown' COLLATE rbit_credential_fenced_v2 = 'unknown'").fetch_one(&mut connection).await.unwrap();
        assert!(reflexive, "DENY is not an invalid nonreflexive comparator");
        connection.close().await.unwrap();
    }
}
