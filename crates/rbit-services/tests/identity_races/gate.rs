//! A pass-through session repository over the real redb store that parks one request at
//! a chosen point, so another request can commit there before it continues.

use std::{
    future::Future,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use rbit_core::{AccountId, Timestamp};
use rbit_identity::{
    TokenDigest,
    sessions::{SessionClock, SessionRecord, SessionRepository, StoredSession},
};
use rbit_storage::{SessionStore, StorageError};
use tokio::sync::oneshot;

/// Where the parked request stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    /// After the live session was read, before the account lookup.
    AfterRead,
    /// After every check passed, immediately before the renewal write.
    BeforeRenewal,
}

#[derive(Debug)]
struct Armed {
    at: Pause,
    reached: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

/// The parked request and the means to let it go.
#[derive(Debug)]
pub struct Parked {
    reached: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

impl Parked {
    /// Resolves once the request is parked; the returned sender lets it continue.
    pub async fn reached(self) -> oneshot::Sender<()> {
        self.reached.await.expect("the request never reached its pause point");
        self.release
    }
}

#[derive(Debug)]
pub struct PausingSessions {
    inner: SessionStore,
    armed: Mutex<Option<Armed>>,
    renewals: AtomicUsize,
}

impl PausingSessions {
    pub fn new(inner: SessionStore) -> Self {
        Self { inner, armed: Mutex::new(None), renewals: AtomicUsize::new(0) }
    }

    pub fn arm(&self, at: Pause) -> Parked {
        let (reached, reached_rx) = oneshot::channel();
        let (release_tx, release) = oneshot::channel();
        *self.armed.lock().unwrap() = Some(Armed { at, reached, release });
        Parked { reached: reached_rx, release: release_tx }
    }

    /// Renewal writes that reached the store (whether or not they applied).
    pub fn renewals(&self) -> usize {
        self.renewals.load(Ordering::SeqCst)
    }

    async fn pause(&self, at: Pause) {
        let armed = {
            let mut slot = self.armed.lock().unwrap();
            if slot.as_ref().is_some_and(|armed| armed.at == at) { slot.take() } else { None }
        };
        if let Some(Armed { reached, release, .. }) = armed {
            reached.send(()).unwrap();
            release.await.expect("the parked request was never released");
        }
    }
}

impl SessionRepository for PausingSessions {
    type Error = StorageError;
    fn put(&self, digest: TokenDigest, record: SessionRecord) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.put(digest, record)
    }
    async fn get_live(&self, digest: TokenDigest) -> Result<Option<SessionRecord>, Self::Error> {
        let record = self.inner.get(digest).await?;
        self.pause(Pause::AfterRead).await;
        Ok(record)
    }
    async fn renew_live(&self, digest: TokenDigest, expires: Timestamp, seen: Timestamp) -> Result<bool, Self::Error> {
        self.pause(Pause::BeforeRenewal).await;
        self.renewals.fetch_add(1, Ordering::SeqCst);
        self.inner.renew(digest, expires, seen).await
    }
    fn delete(&self, digest: TokenDigest) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.delete(digest)
    }
    fn list_for_account(&self, account: AccountId) -> impl Future<Output = Result<Vec<StoredSession>, Self::Error>> + Send {
        self.inner.list_for_account(account)
    }
    fn delete_for_account(&self, account: AccountId, keep: Option<TokenDigest>) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        self.inner.delete_for_account(account, keep)
    }
    fn purge_expired(&self) -> impl Future<Output = Result<usize, Self::Error>> + Send {
        self.inner.purge_expired()
    }
}

/// Ten minutes ahead of the wall clock: past the last-seen resolution, so every resume
/// of a fresh session writes a renewal.
#[derive(Debug, Clone, Copy)]
pub struct TenMinutesLater;

pub const AHEAD_SECONDS: i64 = 600;

impl SessionClock for TenMinutesLater {
    fn now(&self) -> Timestamp {
        Timestamp(Timestamp::now().0 + AHEAD_SECONDS)
    }
}
