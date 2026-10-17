//! Atomic first-user selection across password and external registration.

use rbit_storage::{Database, NewUser, StorageError, sqlite::external_identities::NewExternalIdentity};

fn user(name: &str) -> NewUser {
    NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: "test hash".to_owned(),
        is_admin: false,
    }
}

async fn concurrent_registration(open: bool) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let accounts = database.accounts();
    let external = database.external_identities();
    let identity = NewExternalIdentity { provider_id: "corp", subject: "external", email: None, username: None };
    let (alice, bob, carol, dave) = tokio::join!(
        accounts.register_user(user("alice"), open),
        accounts.register_user(user("bob"), open),
        accounts.register_user(user("carol"), open),
        external.register(user("dave"), true, identity, open),
    );
    let outcomes = [alice, bob, carol, dave.map(Result::unwrap)];
    let admitted: Vec<_> = outcomes.iter().filter_map(|outcome| outcome.as_ref().ok()).collect();
    assert_eq!(admitted.len(), if open { 4 } else { 1 });
    assert_eq!(admitted.iter().filter(|user| user.is_admin).count(), 1);
    assert_eq!(accounts.count_users().await.unwrap(), if open { 4 } else { 1 });
    for (name, outcome) in ["alice", "bob", "carol", "dave"].into_iter().zip(outcomes) {
        match outcome {
            Ok(created) => {
                let stored = accounts.find_user(created.id()).await.unwrap().unwrap();
                assert_eq!(stored.id(), created.id());
                assert_eq!(stored.name(), created.name());
                assert_eq!(stored.is_admin, created.is_admin);
            }
            Err(StorageError::RegistrationClosed) if !open => {
                assert!(accounts.find_account_by_name(name).await.unwrap().is_none(), "no orphan account for {name}");
            }
            other => panic!("unexpected registration outcome: {other:?}"),
        }
    }
}

#[tokio::test]
async fn open_registration_elects_exactly_one_administrator() {
    concurrent_registration(true).await;
}

#[tokio::test]
async fn closed_registration_admits_exactly_one_bootstrap_user() {
    concurrent_registration(false).await;
}

#[tokio::test]
async fn explicit_admin_creation_keeps_the_requested_role() {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
    let accounts = database.accounts();
    assert!(!accounts.create_user(user("alice")).await.unwrap().is_admin);
    let mut admin = user("bob");
    admin.is_admin = true;
    assert!(accounts.create_user(admin).await.unwrap().is_admin);
    assert!(!accounts.register_user(user("carol"), true).await.unwrap().is_admin);
}
