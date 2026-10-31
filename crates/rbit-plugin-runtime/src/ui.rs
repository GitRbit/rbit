//! Atomic declarative UI registration, owned by one plugin generation.

use crate::{Admission, AdmissionError, PluginScope};
use rbit_plugin_api::{Generation, PluginId, UiContribution, UiContributionSet, UiValidationError};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UiRegistryError {
    #[error(transparent)]
    Validation(#[from] UiValidationError),
    #[error(transparent)]
    Admission(#[from] AdmissionError),
    #[error("plugin already has a UI registration")]
    Conflict,
    #[error("UI registration capacity exhausted")]
    Capacity,
    #[error("UI registration is no longer current")]
    Stale,
}

/// Pure data captured by one render. It carries no operation leases or callbacks.
#[derive(Debug, Clone)]
pub struct UiEntry {
    pub owner: PluginId,
    pub generation: Generation,
    pub contribution: UiContribution,
}
#[derive(Debug, Default, Clone)]
pub struct UiSnapshot {
    pub revision: u64,
    pub entries: Arc<[UiEntry]>,
}

#[derive(Debug)]
struct Group {
    generation: Generation,
    token: u64,
    admission: Admission,
    contributions: Vec<UiContribution>,
    published: bool,
}
#[derive(Debug)]
struct State {
    groups: BTreeMap<PluginId, Group>,
    next_token: u64,
    snapshot: UiSnapshot,
}
impl Default for State {
    fn default() -> Self {
        Self { groups: BTreeMap::new(), next_token: 1, snapshot: UiSnapshot { revision: 1, entries: Arc::from([]) } }
    }
}
impl State {
    fn rebuild(&mut self) {
        let mut entries: Vec<_> = self
            .groups
            .iter()
            .filter(|(_, group)| group.published && !group.admission.is_closed())
            .flat_map(|(owner, group)| {
                group.contributions.iter().map(move |contribution| UiEntry {
                    owner: owner.clone(),
                    generation: group.generation,
                    contribution: contribution.clone(),
                })
            })
            .collect();
        entries.sort_by(|left, right| {
            (left.contribution.slot, left.contribution.order, &left.owner, &left.contribution.id).cmp(&(
                right.contribution.slot,
                right.contribution.order,
                &right.owner,
                &right.contribution.id,
            ))
        });
        self.snapshot = UiSnapshot { revision: self.snapshot.revision.saturating_add(1), entries: entries.into() };
    }
    fn purge_closed(&mut self) {
        let visible = self.groups.values().any(|group| group.published && group.admission.is_closed());
        self.groups.retain(|_, group| !group.admission.is_closed());
        if visible {
            self.rebuild();
        }
    }
    fn remove(&mut self, owner: &PluginId, token: u64) -> bool {
        if self.groups.get(owner).is_none_or(|group| group.token != token) {
            return false;
        }
        let published = self.groups.remove(owner).is_some_and(|group| group.published);
        if published {
            self.rebuild();
        }
        true
    }
}

/// Registrations stay owned by the scope even if all public handles are dropped.
#[derive(Debug, Clone, Default)]
pub struct UiRegistry {
    state: Arc<Mutex<State>>,
}
impl UiRegistry {
    /// Validates a complete set before mutating registry state. Staging is invisible;
    /// the host calls `publish` only after successful plugin activation/readiness.
    pub fn stage(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        set: UiContributionSet,
    ) -> Result<UiRegistration, UiRegistryError> {
        set.validate(&owner)?;
        let _lease = scope.try_acquire(generation)?;
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.purge_closed();
        if state.groups.contains_key(&owner) {
            return Err(UiRegistryError::Conflict);
        }
        if state.groups.len() >= crate::MAX_PLUGINS {
            return Err(UiRegistryError::Capacity);
        }
        let token = state.next_token;
        let next = token.checked_add(1).ok_or(UiRegistryError::Capacity)?;
        let weak = Arc::downgrade(&self.state);
        let cleanup_owner = owner.clone();
        // Reserve before attaching cleanup, even if closing wins afterward: an old
        // queued cleanup must never share a token with a subsequent registration.
        state.next_token = next;
        scope.register_cleanup(Box::new(move || {
            if let Some(state) = weak.upgrade() {
                state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&cleanup_owner, token);
            }
            None
        }))?;
        if scope.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        let admission = scope.admission();
        state.groups.insert(
            owner.clone(),
            Group { generation, token, admission: admission.clone(), contributions: set.contributions, published: false },
        );
        Ok(UiRegistration { state: Arc::downgrade(&self.state), owner, generation, token, admission })
    }
    /// Capturing a snapshot never holds a plugin admission lease, including for
    /// administrator lifecycle requests. Closed gates are omitted synchronously.
    pub fn snapshot(&self) -> UiSnapshot {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.purge_closed();
        state.snapshot.clone()
    }
}

#[derive(Debug, Clone)]
pub struct UiRegistration {
    state: Weak<Mutex<State>>,
    owner: PluginId,
    generation: Generation,
    token: u64,
    admission: Admission,
}
impl UiRegistration {
    pub fn owner(&self) -> &PluginId {
        &self.owner
    }
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    pub fn publish(&self) -> Result<(), UiRegistryError> {
        if self.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        let state = self.state.upgrade().ok_or(UiRegistryError::Stale)?;
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.purge_closed();
        let group = state
            .groups
            .get_mut(&self.owner)
            .filter(|group| group.token == self.token && group.generation == self.generation)
            .ok_or(UiRegistryError::Stale)?;
        if group.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        if !group.published {
            group.published = true;
            state.rebuild();
        }
        Ok(())
    }
    /// Atomically replaces this generation's complete declaration. Invalid/stale
    /// updates leave publication untouched; staging updates remain invisible.
    pub fn replace(&self, set: UiContributionSet) -> Result<(), UiRegistryError> {
        set.validate(&self.owner)?;
        if self.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        let state = self.state.upgrade().ok_or(UiRegistryError::Stale)?;
        let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.purge_closed();
        let group = state
            .groups
            .get_mut(&self.owner)
            .filter(|group| group.token == self.token && group.generation == self.generation)
            .ok_or(UiRegistryError::Stale)?;
        if group.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        if group.contributions != set.contributions {
            group.contributions = set.contributions;
            if group.published {
                state.rebuild();
            }
        }
        Ok(())
    }
    /// Removes only this registration, never another generation or replacement.
    pub fn unpublish(&self) -> bool {
        self.state
            .upgrade()
            .is_some_and(|state| state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&self.owner, self.token))
    }
}
