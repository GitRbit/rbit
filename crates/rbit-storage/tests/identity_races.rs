//! Identity/session regression group: contenders released together through a barrier on a
//! multi-threaded runtime race against the real redb and `SQLite` stores. Assertions hold
//! for every interleaving, so they never depend on timing or sleeps.

use std::{future::Future, sync::Arc};

use rbit_core::User;
use rbit_storage::{Database, NewUser};

mod authenticators;
mod credentials;
mod sessions;

/// Rounds per scenario: each round is a fresh contest, so several interleavings are hit.
const ROUNDS: u8 = 16;

/// Spawns `contenders` tasks and releases them together; returns outcomes by index.
async fn race<T, F, Fut>(contenders: usize, start: F) -> Vec<T>
where
    F: Fn(usize) -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let barrier = Arc::new(tokio::sync::Barrier::new(contenders));
    let tasks: Vec<_> = (0..contenders)
        .map(|index| {
            let barrier = Arc::clone(&barrier);
            let work = start(index);
            tokio::spawn(async move {
                barrier.wait().await;
                work.await
            })
        })
        .collect();
    let mut outcomes = Vec::with_capacity(contenders);
    for task in tasks {
        outcomes.push(task.await.expect("contender panicked"));
    }
    outcomes
}

async fn open_database(dir: &tempfile::TempDir) -> Database {
    Database::open(&dir.path().join("rbit.db")).await.unwrap()
}

async fn create_user(database: &Database, name: &str, password_hash: &str) -> User {
    database
        .accounts()
        .create_user(NewUser {
            name: name.parse().unwrap(),
            email: format!("{name}@example.com").parse().unwrap(),
            display_name: None,
            password_hash: password_hash.to_owned(),
            is_admin: false,
        })
        .await
        .unwrap()
}
