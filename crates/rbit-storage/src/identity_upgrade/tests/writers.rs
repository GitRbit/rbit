use super::*;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
#[tokio::test]
async fn cached_released_v1_writer_refuses_after_layout_upgrade_and_default_writer_refuses_preparing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old.db");
    let old = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true)
        .collation("rbit_credential_writer_v1", str::cmp);
    let mut old = SqliteConnection::connect_with(&old).await.unwrap();
    sqlx::migrate!("./migrations").run_to(66, &mut old).await.unwrap();
    sqlx::query("INSERT INTO accounts(id,kind,name,created_at,updated_at) VALUES(1,'user','old',1,1)").execute(&mut old).await.unwrap();
    sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("before").execute(&mut old).await.unwrap();
    let database = crate::Database::open(&path).await.unwrap();
    database.close().await;
    assert!(sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("after").execute(&mut old).await.is_err());
    let f = Fixture::new().await;
    let mut current = f.connection().await;
    sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("allowed legacy").execute(&mut current).await.unwrap();
    let mut owner = f.owner().await;
    owner.begin_preparing().await.unwrap();
    assert!(sqlx::query("UPDATE accounts SET bio=? WHERE id=1").bind("refused preparing").execute(&mut current).await.is_err());
    let value: String = sqlx::query_scalar("SELECT bio FROM accounts WHERE id=1").fetch_one(&mut current).await.unwrap();
    assert_eq!(value, "allowed legacy");
    assert!(sqlx::query("UPDATE credential_protocol SET mode='legacy'").execute(&mut current).await.is_err());
}
#[test]
fn checkpoint_encoding_rejects_unknown_negative_and_exhausted_data_without_debug_authority() {
    use super::super::model::Checkpoint;
    let c = Checkpoint { generation: [1; 16], pair: [2; 16], floor: 50, retired: 9, accounts: 2 };
    assert!(Checkpoint::decode(&c.encode()).unwrap() == c);
    for offset in [0, 1] {
        let mut bytes = c.encode();
        bytes[offset] = 99;
        assert!(Checkpoint::decode(&bytes).is_err());
    }
    let mut bad = c;
    bad.floor = -1;
    assert!(Checkpoint::decode(&bad.encode()).is_err());
    bad = c;
    bad.accounts = i64::MAX;
    assert!(Checkpoint::decode(&bad.encode()).is_err());
}
