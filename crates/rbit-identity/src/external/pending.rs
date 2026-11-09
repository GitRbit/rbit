//! Pending "choose a username" steps, kept in memory.
//!
//! Each step is identified by a fresh [`SessionToken`] the browser keeps in a short-lived
//! cookie; only its digest is held. A step lasts [`REGISTRATION_TTL`]. At most
//! [`MAX_PENDING_REGISTRATIONS`] are kept: starting one drops expired steps and then,
//! when full, the oldest. A restart forgets them all; the user signs in again.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use crate::{SessionToken, TokenDigest};

/// How long the "choose a username" step stays open.
pub const REGISTRATION_TTL: Duration = Duration::from_mins(15);

/// Pending registrations kept at most (oldest beyond this are dropped).
pub const MAX_PENDING_REGISTRATIONS: usize = 10_000;

/// The open steps and what each one registers.
#[derive(Debug)]
pub struct PendingRegistrations<T> {
    entries: Mutex<HashMap<TokenDigest, (T, Instant)>>,
}

impl<T> Default for PendingRegistrations<T> {
    fn default() -> Self {
        Self { entries: Mutex::default() }
    }
}

impl<T: Clone> PendingRegistrations<T> {
    fn lock(&self) -> MutexGuard<'_, HashMap<TokenDigest, (T, Instant)>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens a step for `value` and returns its token.
    pub fn start(&self, value: T) -> SessionToken {
        let token = SessionToken::generate();
        let now = Instant::now();
        let mut entries = self.lock();
        entries.retain(|_, (_, started)| now.duration_since(*started) < REGISTRATION_TTL);
        if entries.len() >= MAX_PENDING_REGISTRATIONS
            && let Some(oldest) = entries.iter().min_by_key(|(_, (_, started))| *started).map(|(digest, _)| *digest)
        {
            entries.remove(&oldest);
        }
        entries.insert(token.digest(), (value, now));
        token
    }

    /// The step behind `token`, while it is open; an expired one is dropped.
    pub fn get(&self, token: &SessionToken) -> Option<T> {
        let mut entries = self.lock();
        let digest = token.digest();
        match entries.get(&digest) {
            Some((value, started)) if started.elapsed() < REGISTRATION_TTL => Some(value.clone()),
            Some(_) => {
                entries.remove(&digest);
                None
            }
            None => None,
        }
    }

    /// Closes the step behind `token`.
    pub fn end(&self, token: &SessionToken) {
        self.lock().remove(&token.digest());
    }
}

#[cfg(test)]
mod tests;
