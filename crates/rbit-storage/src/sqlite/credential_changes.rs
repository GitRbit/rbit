//! Owned forced password replacement followed by acknowledged session revocation.
//!
//! SQL and KV commits are sequential, not a distributed transaction. No execution,
//! physical store ownership, crash recovery or all-credential-flow certificate.
//! The two slots bound application owners and retained KV workers, not residual
//! `SQLx` driver commands after a canceled SQL future. Such loss stays uncertain.
use std::sync::{Arc, Mutex, MutexGuard};

use rbit_authority_fence::{Mutation, WriterRegistration};
use rbit_core::{AccountId, Timestamp};
use tokio::sync::{Notify, oneshot};

use super::{
    profiles::ProfileStore,
    recovery::{EmailTokenRecord, RecoveryStore},
};
use crate::{SessionStore, StorageError};

const CAPACITY: usize = 2;

#[derive(Debug, Default)]
struct State {
    reserved: usize,
    stopped: bool,
    uncertain: bool,
}
#[derive(Debug, Default)]
struct Inner {
    state: Mutex<State>,
    changed: Notify,
}
impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| {
            let mut state = poisoned.into_inner();
            state.stopped = true;
            state.uncertain = true;
            state
        })
    }
    fn uncertain(&self) {
        self.state().uncertain = true;
        self.changed.notify_waiters();
    }
}

/// Shared only by clones of one Database; does not bind separate physical opens.
#[derive(Debug, Clone, Default)]
pub(super) struct Coordinator {
    inner: Arc<Inner>,
}
impl Coordinator {
    pub(super) fn bind(
        &self,
        profiles: ProfileStore,
        recovery: RecoveryStore,
        sessions: SessionStore,
        writer: WriterRegistration,
    ) -> ForcedPasswordChanges {
        ForcedPasswordChanges { coordinator: self.clone(), profiles, recovery, sessions, writer }
    }
    pub(super) fn stop(&self) {
        self.inner.state().stopped = true;
        self.inner.changed.notify_waiters();
    }
    pub(super) async fn drain(&self) -> Result<(), ForcedPasswordDrainError> {
        loop {
            let changed = self.inner.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.inner.state();
                if state.reserved == 0 {
                    return if state.uncertain { Err(ForcedPasswordDrainError) } else { Ok(()) };
                }
            }
            changed.await;
        }
    }
}

/// A completed drain found an unacknowledged command. Never a rollback statement.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("forced password command settlement is uncertain")]
pub struct ForcedPasswordDrainError;

/// A typed SQL/session pairing validated by Database before this handle exists.
/// No arbitrary mutation, callback, executor or ownership-token input is exposed.
#[derive(Debug, Clone)]
pub struct ForcedPasswordChanges {
    coordinator: Coordinator,
    profiles: ProfileStore,
    recovery: RecoveryStore,
    sessions: SessionStore,
    writer: WriterRegistration,
}
impl ForcedPasswordChanges {
    /// Reserves before hashing. Dropping an unstarted reservation is known no-write.
    pub fn reserve(&self) -> Result<ForcedPasswordReservation, StorageError> {
        let mut state = self.coordinator.inner.state();
        if state.stopped {
            return Err(StorageError::Conflict("forced password commands stopped"));
        }
        if state.reserved >= CAPACITY {
            return Err(StorageError::Conflict("forced password command capacity"));
        }
        state.reserved += 1;
        let command = Arc::new(Command { inner: self.coordinator.inner.clone(), phase: Mutex::new(Phase::Reserved) });
        Ok(ForcedPasswordReservation { changes: self.clone(), command })
    }
}

/// Nonclone reservation; no SQL begins until the synchronous start call admits it.
#[derive(Debug)]
pub struct ForcedPasswordReservation {
    changes: ForcedPasswordChanges,
    command: Arc<Command>,
}
impl ForcedPasswordReservation {
    /// Starts owned work before returning a receiver-only waiter. The input is an
    /// encoded password hash; no command/path/SQL/worker callback is accepted.
    pub fn start(self, account: AccountId, password_hash: String) -> Result<ForcedPasswordResult, StorageError> {
        self.start_using(account, password_hash, |changes, command, account, hash| async move {
            run_with(changes, command, account, &hash, |sessions, account, mutation, command| async move {
                sessions.finish_forced_password(account, mutation, command).await
            })
            .await
        })
    }

    /// Redeems an email reset and immediately revokes every session. `now` retains
    /// the caller's captured-time policy; this does not promise commit-time expiry.
    pub fn start_reset(
        self,
        digest: [u8; 32],
        password_hash: String,
        now: Timestamp,
    ) -> Result<ForcedPasswordResult<Option<ResetPasswordSettlement>>, StorageError> {
        self.start_operation(move |changes, command| async move {
            run_reset_with(changes, command, &digest, &password_hash, now, |sessions, account, mutation, command| async move {
                sessions.finish_forced_password(account, mutation, command).await
            })
            .await
        })
    }

    fn start_using<F, Fut>(self, account: AccountId, password_hash: String, operation: F) -> Result<ForcedPasswordResult, StorageError>
    where
        F: FnOnce(ForcedPasswordChanges, Arc<Command>, AccountId, String) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<usize, StorageError>> + Send + 'static,
    {
        self.start_operation(move |changes, command| operation(changes, command, account, password_hash))
    }

    fn start_operation<T, F, Fut>(self, operation: F) -> Result<ForcedPasswordResult<T>, StorageError>
    where
        T: Send + 'static,
        F: FnOnce(ForcedPasswordChanges, Arc<Command>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, StorageError>> + Send + 'static,
    {
        let runtime =
            tokio::runtime::Handle::try_current().map_err(|_| StorageError::Conflict("forced password command runtime unavailable"))?;
        let (send, receive) = oneshot::channel();
        let inner = self.command.inner.clone();
        // Same mutex as stop/reserve: a stale reservation cannot start after stop.
        let state = inner.state();
        if state.stopped {
            drop(state);
            return Err(StorageError::Conflict("forced password commands stopped"));
        }
        let Self { changes, command } = self;
        // Ownership was admitted above. Drain already counts it; release the
        // mutex before spawn, which may synchronously destroy a rejected task.
        drop(state);
        drop(runtime.spawn(async move {
            let result = operation(changes, command).await;
            let _ = send.send(result);
        }));
        Ok(ForcedPasswordResult { receive })
    }
}

/// Dropping this waiter never cancels the actual command or frees its reservation.
#[derive(Debug)]
pub struct ForcedPasswordResult<T = usize> {
    receive: oneshot::Receiver<Result<T, StorageError>>,
}
impl<T> ForcedPasswordResult<T> {
    pub async fn wait(self) -> Result<T, StorageError> {
        self.receive.await.map_err(|_| StorageError::Conflict("forced password command outcome unavailable"))?
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Reserved,
    Authority,
    Acknowledged,
    Uncertain,
}
/// Private storage ownership bridge. The actual KV closure retains an Arc, not
/// merely a join waiter; the slot lasts until every actual owner releases it.
#[derive(Debug)]
pub(crate) struct Command {
    inner: Arc<Inner>,
    phase: Mutex<Phase>,
}
impl Command {
    fn phase(&self) -> MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(|poisoned| {
            self.inner.uncertain();
            let mut phase = poisoned.into_inner();
            *phase = Phase::Uncertain;
            phase
        })
    }
    fn owner_dropped(&self) {
        let mut phase = self.phase();
        if *phase == Phase::Authority {
            *phase = Phase::Uncertain;
            self.inner.uncertain();
        }
    }
    /// Called within the actual KV worker after its commit, or by the reset task
    /// only after the SQL helper has explicitly acknowledged rollback on None.
    pub(crate) fn acknowledge(&self, mutation: Mutation, success: bool) {
        let mut phase = self.phase();
        if success && *phase == Phase::Authority {
            mutation.complete();
            *phase = Phase::Acknowledged;
        } else {
            *phase = Phase::Uncertain;
            self.inner.uncertain();
            // Incomplete Mutation drops here; later success cannot clear uncertainty.
        }
    }
}
impl Drop for Command {
    fn drop(&mut self) {
        self.owner_dropped();
        let mut state = self.inner.state();
        state.reserved -= 1;
        drop(state);
        self.inner.changed.notify_waiters();
    }
}
#[derive(Debug)]
struct AsyncOwner(Arc<Command>);
impl Drop for AsyncOwner {
    fn drop(&mut self) {
        // This runs even while a worker Arc survives: last-Arc Drop is too late.
        self.0.owner_dropped();
    }
}
// Private monomorphized phase seam. The public API always selects the fixed
// revoke-all worker; tests can gate/fault that actual worker without public hooks.
async fn run_with<F, Fut>(
    changes: ForcedPasswordChanges,
    command: Arc<Command>,
    account: AccountId,
    hash: &str,
    finish: F,
) -> Result<usize, StorageError>
where
    F: FnOnce(SessionStore, AccountId, Mutation, Arc<Command>) -> Fut,
    Fut: std::future::Future<Output = Result<usize, StorageError>>,
{
    let _owner = AsyncOwner(command.clone());
    let mutation = changes.writer.begin().await.map_err(|error| {
        StorageError::Conflict(match error {
            rbit_authority_fence::BeginError::Capacity => "forced password mutation capacity",
            rbit_authority_fence::BeginError::Stopped => "forced password writer stopped",
        })
    })?;
    *command.phase() = Phase::Authority;
    // Preserve the unconditional setter's missing-account/no-op behavior.
    changes.profiles.replace_password_hash(account, hash, None).await?;
    finish(changes.sessions, account, mutation, command).await
}

/// A committed reset and acknowledged end-all revocation, never a crash-atomic receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetPasswordSettlement {
    pub token: EmailTokenRecord,
    pub removed_sessions: usize,
}

// Shared command/guard lifecycle, with a private fixed production continuation.
async fn run_reset_with<F, Fut>(
    changes: ForcedPasswordChanges,
    command: Arc<Command>,
    digest: &[u8; 32],
    hash: &str,
    now: Timestamp,
    finish: F,
) -> Result<Option<ResetPasswordSettlement>, StorageError>
where
    F: FnOnce(SessionStore, AccountId, Mutation, Arc<Command>) -> Fut,
    Fut: std::future::Future<Output = Result<usize, StorageError>>,
{
    let _owner = AsyncOwner(command.clone());
    let mutation = changes.writer.begin().await.map_err(|error| {
        StorageError::Conflict(match error {
            rbit_authority_fence::BeginError::Capacity => "forced password mutation capacity",
            rbit_authority_fence::BeginError::Stopped => "forced password writer stopped",
        })
    })?;
    *command.phase() = Phase::Authority;
    let Some(token) = changes.recovery.reset_password_raw(digest, hash, now).await? else {
        // Explicit awaited rollback occurred in the unchanged raw helper. No KV
        // worker or success event is needed for this known no-write refusal.
        command.acknowledge(mutation, true);
        return Ok(None);
    };
    let removed_sessions = finish(changes.sessions, token.account_id, mutation, command).await?;
    Ok(Some(ResetPasswordSettlement { token, removed_sessions }))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod reset_tests;
