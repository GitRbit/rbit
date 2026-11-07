//! Real recovery subscriber ownership against the shared stores and mail fixture.

use super::*;
use rbit_kernel::TaskState;
use sqlx::Connection as _;

#[tokio::test]
async fn duplicate_start_is_one_subscriber_and_shutdown_stops_future_token_writes() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice", false).await;
    let bob = fixture.user("bob", false).await;
    fixture.recovery.spawn_background();
    fixture.recovery.spawn_background();
    assert_eq!(fixture.recovery.background_state(), TaskState::Running);
    fixture.services.events.publish(DomainEvent::UserSignedUp { user: alice.id() });
    fixture.mail(1).await;
    fixture.recovery.shutdown().await;
    assert_eq!(fixture.recovery.background_state(), TaskState::Stopped);
    fixture.recovery.spawn_background();
    fixture.services.events.publish(DomainEvent::UserSignedUp { user: bob.id() });

    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.dir.path().join("rbit.db"));
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_tokens WHERE account_id = ?")
        .bind(alice.id().0)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(count, 1, "two starts must not mint two verification tokens");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_tokens WHERE account_id = ?")
        .bind(bob.id().0)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(count, 0, "a drained owner must not process newly published events");
}

#[tokio::test]
async fn disabled_email_creates_no_background_task() {
    let fixture = Fixture::with_transport(MailTransport::Disabled).await;
    assert_eq!(fixture.recovery.background_state(), TaskState::Idle);
    fixture.recovery.spawn_background();
    assert_eq!(fixture.recovery.background_state(), TaskState::Idle);
    fixture.recovery.shutdown().await;
    assert_eq!(fixture.recovery.background_state(), TaskState::Stopped);
}
