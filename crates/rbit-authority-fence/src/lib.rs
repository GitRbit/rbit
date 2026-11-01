//! Closed-only coordination for explicitly enrolled application mutations.
//!
//! There is no execution, generation issuance, reopening or OS/store ownership API.
//! Other domains, unregistered writers, SQL drivers and physical file leases are
//! outside this primitive. An observed successful application mutation is not proof
//! that a pool/worker closed; cancellation after acquisition remains uncertain.
use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard},
    time::{Instant, timeout_at},
};

pub const MAX_MUTATIONS: usize = 128;

/// Point-in-time enrolled counters, never an execution/readiness certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub active: usize,
    pub queued: usize,
    pub uncertain: bool,
    pub writers_stopped: bool,
}
#[derive(Debug)]
struct State {
    active: usize,
    queued: usize,
    uncertain: bool,
    writers_stopped: bool,
}
#[derive(Debug)]
struct Inner {
    state: Mutex<State>,
    serial: Arc<AsyncMutex<()>>,
    changed: Notify,
}
impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.uncertain = true;
                state
            }
        }
    }
}

/// Always closed. Construction starts no worker, timer, query or file operation.
#[derive(Debug, Clone)]
pub struct Domain {
    inner: Arc<Inner>,
}
impl Default for Domain {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State { active: 0, queued: 0, uncertain: false, writers_stopped: false }),
                serial: Arc::new(AsyncMutex::new(())),
                changed: Notify::new(),
            }),
        }
    }
}
impl Domain {
    pub fn new() -> Self {
        Self::default()
    }
    /// Enrolls a writer in this domain only; never issues generation authority.
    pub fn writer(&self) -> WriterRegistration {
        WriterRegistration { inner: self.inner.clone() }
    }
    pub fn snapshot(&self) -> Snapshot {
        let state = self.inner.state();
        Snapshot { active: state.active, queued: state.queued, uncertain: state.uncertain, writers_stopped: state.writers_stopped }
    }
    /// Irreversibly rejects future and queued-not-started enrolled mutations.
    /// This happens synchronously; dropping the token never re-enables writers.
    pub fn stop_writers(&self) -> WriterShutdown {
        self.inner.state().writers_stopped = true;
        self.inner.changed.notify_waiters();
        WriterShutdown { inner: self.inner.clone() }
    }
    /// Holds serialization on success. Only enrolled mutations are quiescent;
    /// queued/new writers cannot execute while the returned guard is retained.
    /// No physical SQL worker/pool, other domain, or store shutdown proof.
    pub async fn drain(&self, deadline: Instant) -> Result<QuiescentGuard, DrainError> {
        loop {
            if Instant::now() >= deadline {
                return Err(DrainError::Deadline);
            }
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let permit = timeout_at(deadline, self.inner.serial.clone().lock_owned()).await.map_err(|_| DrainError::Deadline)?;
            if Instant::now() >= deadline {
                return Err(DrainError::Deadline);
            }
            let idle = {
                let state = self.inner.state();
                let idle = state.active == 0 && state.queued == 0;
                if idle && state.uncertain {
                    return Err(DrainError::Uncertain);
                }
                idle
            };
            if idle {
                if Instant::now() >= deadline {
                    return Err(DrainError::Deadline);
                }
                return Ok(QuiescentGuard { _permit: permit });
            }
            drop(permit);
            timeout_at(deadline, notified).await.map_err(|_| DrainError::Deadline)?;
            tokio::task::consume_budget().await;
        }
    }
}

/// Cloneable enrollment, not a file/process ownership or execution capability.
#[derive(Debug, Clone)]
pub struct WriterRegistration {
    inner: Arc<Inner>,
}
impl WriterRegistration {
    /// Whether both registrations enroll the same in-process domain.
    /// This does not establish physical store identity or execution authority.
    pub fn same_domain(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Registers before waiting. A canceled queued ticket is known to have
    /// executed no enrolled work; after acquisition, Drop is sticky uncertainty.
    pub async fn begin(&self) -> Result<Mutation, BeginError> {
        {
            let mut state = self.inner.state();
            if state.writers_stopped {
                return Err(BeginError::Stopped);
            }
            if state.active.checked_add(state.queued).is_none_or(|n| n >= MAX_MUTATIONS) {
                return Err(BeginError::Capacity);
            }
            state.queued += 1;
        }
        let mut ticket = Ticket { inner: self.inner.clone(), queued: true };
        let permit = self.inner.serial.clone().lock_owned().await;
        {
            let mut state = self.inner.state();
            // A preexisting ticket may acquire serial only after shutdown.
            // Returning here drops its Ticket cleanly: no enrolled work began.
            if state.writers_stopped {
                return Err(BeginError::Stopped);
            }
            state.queued -= 1;
            state.active += 1;
            ticket.queued = false;
        }
        self.inner.changed.notify_waiters();
        Ok(Mutation { inner: self.inner.clone(), _permit: permit, complete: false })
    }
}
#[derive(Debug)]
struct Ticket {
    inner: Arc<Inner>,
    queued: bool,
}
impl Drop for Ticket {
    fn drop(&mut self) {
        if self.queued {
            self.inner.state().queued -= 1;
            self.inner.changed.notify_waiters();
        }
    }
}

/// Nonclone owned mutation. Complete only after the entire enrolled operation
/// returns a certified success; errors/cancellation/panic leave sticky uncertainty.
#[derive(Debug)]
pub struct Mutation {
    inner: Arc<Inner>,
    _permit: OwnedMutexGuard<()>,
    complete: bool,
}
impl Mutation {
    pub fn complete(mut self) {
        {
            let mut state = self.inner.state();
            state.active -= 1;
            self.complete = true;
        }
        self.inner.changed.notify_waiters();
    }
}
impl Drop for Mutation {
    fn drop(&mut self) {
        if !self.complete {
            let mut state = self.inner.state();
            state.active -= 1;
            state.uncertain = true;
            drop(state);
            self.inner.changed.notify_waiters();
        }
    }
}

/// Serialization retained for enrolled quiescence only. Drop does not open any
/// generation; later enrolled writes may proceed, still with execution closed.
#[derive(Debug)]
pub struct QuiescentGuard {
    _permit: OwnedMutexGuard<()>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeginError {
    Capacity,
    Stopped,
}
impl fmt::Display for BeginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Capacity => "enrolled mutation capacity",
            Self::Stopped => "enrolled writers permanently closed",
        })
    }
}
impl std::error::Error for BeginError {}

/// Terminal closure token for this enrolled domain only, never physical ownership.
#[derive(Debug)]
pub struct WriterShutdown {
    inner: Arc<Inner>,
}
impl WriterShutdown {
    /// Waits for acquired mutations only. Old queued tickets may remain parked,
    /// but the irreversible pre/post-acquisition check prevents their work forever.
    /// Success does not say queued futures, SQL workers or resources were drained.
    pub async fn drain(&self, deadline: Instant) -> Result<StoppedWriters, DrainError> {
        loop {
            if Instant::now() >= deadline {
                return Err(DrainError::Deadline);
            }
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let settled = {
                let state = self.inner.state();
                if !state.writers_stopped {
                    return Err(DrainError::Uncertain);
                }
                if state.active == 0 && state.uncertain {
                    return Err(DrainError::Uncertain);
                }
                state.active == 0
            };
            if settled {
                if Instant::now() >= deadline {
                    return Err(DrainError::Deadline);
                }
                return Ok(StoppedWriters { _inner: self.inner.clone() });
            }
            timeout_at(deadline, notified).await.map_err(|_| DrainError::Deadline)?;
            tokio::task::consume_budget().await;
        }
    }
}
/// Permanently closed enrolled authority with no acquired uncertain mutation.
/// Known-ineligible queued futures may still exist. Not an execution capability,
/// all-worker/physical-store shutdown certificate or cross-domain ownership proof.
#[derive(Debug)]
pub struct StoppedWriters {
    _inner: Arc<Inner>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainError {
    Deadline,
    Uncertain,
}
impl fmt::Display for DrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Deadline => "enrolled drain deadline",
            Self::Uncertain => "enrolled mutation settlement uncertain",
        })
    }
}
impl std::error::Error for DrainError {}

#[cfg(test)]
mod tests;
