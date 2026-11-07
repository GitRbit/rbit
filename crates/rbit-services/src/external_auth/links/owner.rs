//! Bounded original-flow context, independent of legacy stateless flow cookies.
use super::{Callback, EpochSessionStore, Flow, OriginalLinkCallback, ServiceError, SessionPromotion, unavailable};
use std::collections::{HashMap, hash_map::Entry};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
const MAX_PENDING: usize = 10_000;
const TTL: Duration = Duration::from_secs(super::super::FLOW_TTL_SECONDS.cast_unsigned());
#[derive(Default)]
pub(super) struct State {
    reserved: usize,
    pending: HashMap<String, Pending>,
}
struct Pending {
    provider: String,
    spec: SessionPromotion,
    started: Instant,
}
#[derive(Clone)]
pub(in crate::external_auth) struct OriginalLinks {
    state: Arc<Mutex<State>>,
    pub(super) sessions: EpochSessionStore,
}
pub(super) struct Reservation {
    state: Arc<Mutex<State>>,
    active: bool,
}
impl std::fmt::Debug for OriginalLinks {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("OriginalLinks").finish_non_exhaustive()
    }
}
impl OriginalLinks {
    pub(in crate::external_auth) fn new(sessions: EpochSessionStore) -> Self {
        Self { state: Arc::default(), sessions }
    }
    pub(super) fn reserve(&self) -> Result<Reservation, ServiceError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        // Each sweep is bounded by MAX_PENDING, including outstanding reservations.
        state.pending.retain(|_, pending| pending.started.elapsed() < TTL);
        if state.reserved + state.pending.len() >= MAX_PENDING {
            return Err(unavailable());
        }
        state.reserved += 1;
        Ok(Reservation { state: Arc::clone(&self.state), active: true })
    }
    pub(in crate::external_auth) fn verified(&self, nonce: &str, callback: Callback) -> Result<OriginalLinkCallback, ServiceError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let pending = state.pending.get(nonce).filter(|pending| pending.started.elapsed() < TTL).ok_or_else(unavailable)?;
        if callback.identity.provider_id != pending.provider || callback.link_account != Some(pending.spec.account()) {
            return Err(ServiceError::Forbidden);
        }
        Ok(OriginalLinkCallback { callback, nonce: nonce.to_owned(), spec: pending.spec, owner: Arc::downgrade(&self.state) })
    }
    pub(in crate::external_auth) fn validate_callback(&self, nonce: &str, provider: &str) -> Result<SessionPromotion, ServiceError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state
            .pending
            .get(nonce)
            .filter(|pending| pending.started.elapsed() < TTL && pending.provider == provider)
            .map(|pending| pending.spec)
            .ok_or_else(unavailable)
    }
    pub(super) fn claim(&self, callback: OriginalLinkCallback) -> Result<Callback, ServiceError> {
        let OriginalLinkCallback { callback, nonce, spec, owner } = callback;
        if !owner.ptr_eq(&Arc::downgrade(&self.state)) {
            return Err(ServiceError::Forbidden);
        }
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match state.pending.entry(nonce) {
            Entry::Occupied(entry)
                if entry.get().started.elapsed() < TTL
                    && entry.get().spec == spec
                    && entry.get().provider == callback.identity.provider_id =>
            {
                entry.remove();
                Ok(callback)
            }
            _ => Err(unavailable()),
        }
    }
}
impl Reservation {
    pub(super) fn publish(mut self, flow: &Flow, spec: SessionPromotion) -> Result<(), ServiceError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let Entry::Vacant(entry) = state.pending.entry(flow.state.clone()) else {
            return Err(unavailable());
        };
        entry.insert(Pending { provider: flow.provider.clone(), spec, started: Instant::now() });
        state.reserved -= 1;
        self.active = false;
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.active {
            self.state.lock().unwrap_or_else(PoisonError::into_inner).reserved -= 1;
        }
    }
}

#[cfg(test)]
mod tests;
