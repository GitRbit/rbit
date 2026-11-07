use super::*;
use crate::{Work, count_work, sqlite::plans};
fn new(name: &str, hash: &str) -> NewUser {
    NewUser {
        name: name.parse().unwrap(),
        email: format!("{name}@example.com").parse().unwrap(),
        display_name: None,
        password_hash: hash.into(),
        is_admin: false,
    }
}
#[tokio::test]
async fn principal_login_read_ignores_reclaimed_names_and_uses_one_pk_query() {
    let (_directory, database) = plans::database().await;
    let store = database.accounts();
    let alice = store.create_user(new("alice", "alice PHC")).await.unwrap();
    let bob = store.create_user(new("bob", "bob PHC")).await.unwrap();
    store.rename(alice.id(), &"renamed".parse().unwrap()).await.unwrap();
    store.rename(bob.id(), &"alice".parse().unwrap()).await.unwrap();
    assert_eq!(store.find_login(alice.name().as_str()).await.unwrap().unwrap().user.id(), bob.id());
    let (found, work) = count_work(store.find_login_by_id(alice.id())).await;
    let found = found.unwrap().unwrap();
    assert_eq!(found.user.id(), alice.id());
    assert_eq!(found.user.name().as_str(), "renamed");
    assert_eq!(found.password_hash, "alice PHC");
    assert!(!found.suspended);
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    assert!(store.find_login_by_id(AccountId(999)).await.unwrap().is_none());
    plans::assert_no_scans(&database, LOGIN_BY_ID, &[]).await;
}
