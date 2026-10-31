//! Reusable scoped storage for trusted host extension interfaces and callbacks.
//!
//! Hosts validate typed values before staging. Callback access requires admission;
//! retained snapshots cannot invoke withdrawn/replaced batches. Embedded native
//! values must have cooperative, nonblocking destructors; this is not a sandbox.

use crate::{Admission, AdmissionError, OperationLease, PluginScope};
use rbit_plugin_api::{Generation, PluginId};
use std::{
    any::Any,
    collections::BTreeMap,
    fmt,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio_util::sync::CancellationToken;

pub const MAX_SCOPED_EXTENSIONS: usize = 32;
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionRegistryError {
    #[error("scoped extension registry is permanently closed")]
    Closed,
    #[error(transparent)]
    Admission(#[from] AdmissionError),
    #[error("plugin already has a scoped extension registration")]
    Conflict,
    #[error("scoped extension capacity exhausted")]
    Capacity,
    #[error("scoped extension registration is no longer current")]
    Stale,
}
type Metadata = Arc<dyn Any + Send + Sync>;
struct ExtensionValue<T> {
    value: Arc<T>,
    metadata: Option<Metadata>,
}
struct Batch<T> {
    values: Vec<ExtensionValue<T>>,
    active: Arc<AtomicBool>,
    cancellation: CancellationToken,
}
impl<T> Batch<T> {
    fn new(values: Vec<(T, Option<Metadata>)>, parent: &CancellationToken) -> Self {
        Self {
            values: values.into_iter().map(|(value, metadata)| ExtensionValue { value: Arc::new(value), metadata }).collect(),
            active: Arc::new(AtomicBool::new(false)),
            cancellation: parent.child_token(),
        }
    }
    fn revoke(&self) {
        self.active.store(false, Ordering::Release);
    }
}
impl<T> Drop for Batch<T> {
    fn drop(&mut self) {
        self.revoke();
        self.cancellation.cancel();
    }
}
struct Group<T> {
    generation: Generation,
    token: u64,
    admission: Admission,
    cancellation: CancellationToken,
    batch: Batch<T>,
    published: bool,
}
// Final Arc<T> destructors may reenter a host registry. Retain all replaced data
// until its mutex has been released, including obsolete snapshot entry arrays.
struct Retired<T> {
    groups: Vec<Group<T>>,
    batches: Vec<Batch<T>>,
    snapshots: Vec<ExtensionSnapshot<T>>,
}
impl<T> Default for Retired<T> {
    fn default() -> Self {
        Self { groups: Vec::new(), batches: Vec::new(), snapshots: Vec::new() }
    }
}
struct State<T> {
    closed: bool,
    closing_tokens: Vec<CancellationToken>,
    groups: BTreeMap<PluginId, Group<T>>,
    next_token: u64,
    snapshot: ExtensionSnapshot<T>,
    capacity: usize,
    retired: Retired<T>,
}
impl<T> Default for State<T> {
    fn default() -> Self {
        Self {
            closed: false,
            closing_tokens: Vec::new(),
            groups: BTreeMap::new(),
            next_token: 1,
            snapshot: ExtensionSnapshot { revision: 1, entries: Arc::from([]) },
            capacity: crate::MAX_PLUGINS * MAX_SCOPED_EXTENSIONS,
            retired: Retired::default(),
        }
    }
}
impl<T> State<T> {
    fn count(&self) -> usize {
        self.groups.values().map(|group| group.batch.values.len()).sum()
    }
    fn rebuild(&mut self) {
        let entries: Vec<_> = self
            .groups
            .iter()
            .filter(|(_, group)| group.published && !group.admission.is_closed())
            .flat_map(|(owner, group)| {
                group.batch.values.iter().map(move |value| ExtensionEntry {
                    owner: owner.clone(),
                    generation: group.generation,
                    value: Arc::downgrade(&value.value),
                    metadata: value.metadata.clone(),
                    admission: group.admission.clone(),
                    active: Arc::clone(&group.batch.active),
                    cancellation: group.batch.cancellation.clone(),
                })
            })
            .collect();
        let snapshot = ExtensionSnapshot { revision: self.snapshot.revision.saturating_add(1), entries: entries.into() };
        self.retired.snapshots.push(std::mem::replace(&mut self.snapshot, snapshot));
    }
    fn purge_closed(&mut self) {
        let closed: Vec<_> = self.groups.iter().filter(|(_, group)| group.admission.is_closed()).map(|(owner, _)| owner.clone()).collect();
        let mut visible = false;
        for owner in closed {
            if let Some(group) = self.groups.remove(&owner) {
                visible |= group.published;
                group.batch.revoke();
                self.retired.groups.push(group);
            }
        }
        if visible {
            self.rebuild();
        }
    }
    fn remove(&mut self, owner: &PluginId, token: u64) -> bool {
        if self.groups.get(owner).is_none_or(|group| group.token != token) {
            return false;
        }
        if let Some(group) = self.groups.remove(owner) {
            let published = group.published;
            group.batch.revoke();
            self.retired.groups.push(group);
            if published {
                self.rebuild();
            }
            return true;
        }
        false
    }
}
fn detach_state<T, R>(state: &Mutex<State<T>>, operation: impl FnOnce(&mut State<T>) -> R) -> (R, Retired<T>) {
    let mut state = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let result = operation(&mut state);
    let retired = std::mem::take(&mut state.retired);
    drop(state);
    (result, retired)
}
fn with_state<T, R>(state: &Mutex<State<T>>, operation: impl FnOnce(&mut State<T>) -> R) -> R {
    let (result, retired) = detach_state(state, operation);
    drop(retired);
    result
}

/// One immutable registry view. Only metadata is retained strongly; callback
/// resources stay alive solely through a current batch or an admitted operation.
pub struct ExtensionSnapshot<T> {
    pub revision: u64,
    pub entries: Arc<[ExtensionEntry<T>]>,
}
impl<T> Clone for ExtensionSnapshot<T> {
    fn clone(&self) -> Self {
        Self { revision: self.revision, entries: Arc::clone(&self.entries) }
    }
}
impl<T> Default for ExtensionSnapshot<T> {
    fn default() -> Self {
        Self { revision: 0, entries: Arc::from([]) }
    }
}
impl<T> fmt::Debug for ExtensionSnapshot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionSnapshot").field("revision", &self.revision).field("entries", &self.entries.len()).finish()
    }
}

pub struct ExtensionEntry<T> {
    pub owner: PluginId,
    pub generation: Generation,
    value: Weak<T>,
    metadata: Option<Metadata>,
    admission: Admission,
    active: Arc<AtomicBool>,
    cancellation: CancellationToken,
}
impl<T> fmt::Debug for ExtensionEntry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionEntry").field("owner", &self.owner).field("generation", &self.generation).finish_non_exhaustive()
    }
}
impl<T> ExtensionEntry<T> {
    /// Immutable host-supplied selection metadata, paired atomically with this
    /// callback. Reading it grants no callback authority or mutable registry access.
    pub fn metadata<M: Any>(&self) -> Option<&M> {
        self.metadata.as_deref()?.downcast_ref()
    }
    /// Keep the resulting lease through the callback's complete future/body.
    pub fn admit(&self) -> Result<ExtensionLease<T>, AdmissionError> {
        let lease = self.admission.try_acquire(self.generation)?;
        if !self.active.load(Ordering::Acquire) {
            return Err(AdmissionError::Closed);
        }
        let value = self.value.upgrade().ok_or(AdmissionError::Closed)?;
        // Withdrawal may win between the first gate check and upgrading. Only an
        // admitted operation retains callback resources; stale metadata never does.
        if !self.active.load(Ordering::Acquire) {
            return Err(AdmissionError::Closed);
        }
        Ok(ExtensionLease { value, lease, cancellation: self.cancellation.child_token() })
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.child_token()
    }
}

/// Admitted callback data paired with its full operation lifetime.
pub struct ExtensionLease<T> {
    value: Arc<T>,
    lease: OperationLease,
    cancellation: CancellationToken,
}
impl<T> fmt::Debug for ExtensionLease<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionLease").field("lease", &self.lease).finish_non_exhaustive()
    }
}
impl<T> ExtensionLease<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.child_token()
    }
    /// Adapters transferring ownership to a streamed body must retain the returned
    /// lease until callback work and output end. Do not cache the value separately.
    pub fn into_parts(self) -> (Arc<T>, OperationLease, CancellationToken) {
        (self.value, self.lease, self.cancellation)
    }
}

/// Bounded generation registry. Validation belongs to the typed host adapter.
/// Native value destructors run after its mutex is released, never under it.
pub struct ScopedExtensionRegistry<T> {
    state: Arc<Mutex<State<T>>>,
}
impl<T> Clone for ScopedExtensionRegistry<T> {
    fn clone(&self) -> Self {
        Self { state: Arc::clone(&self.state) }
    }
}
impl<T> Default for ScopedExtensionRegistry<T> {
    fn default() -> Self {
        Self { state: Arc::new(Mutex::new(State::default())) }
    }
}
impl<T> fmt::Debug for ScopedExtensionRegistry<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f.debug_struct("ScopedExtensionRegistry")
            .field("owners", &state.groups.len())
            .field("revision", &state.snapshot.revision)
            .finish_non_exhaustive()
    }
}
impl<T: Send + Sync + 'static> ScopedExtensionRegistry<T> {
    /// Total entries includes staged and published batches. Per-generation batches
    /// remain limited to 32 even when the registry has a larger aggregate limit.
    pub fn with_capacity(capacity: usize) -> Result<Self, ExtensionRegistryError> {
        if capacity == 0 || capacity > crate::MAX_PLUGINS * MAX_SCOPED_EXTENSIONS {
            return Err(ExtensionRegistryError::Capacity);
        }
        Ok(Self { state: Arc::new(Mutex::new(State { capacity, ..State::default() })) })
    }
    pub fn stage(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        values: Vec<T>,
    ) -> Result<ExtensionRegistration<T>, ExtensionRegistryError> {
        self.stage_values(owner, scope, generation, values.into_iter().map(|value| (value, None)).collect())
    }
    /// Registers immutable metadata with each value. Hosts must validate metadata
    /// and values before staging; snapshots reveal only metadata until admission.
    pub fn stage_with_metadata<M: Send + Sync + 'static>(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        values: Vec<(T, M)>,
    ) -> Result<ExtensionRegistration<T>, ExtensionRegistryError> {
        self.stage_values(
            owner,
            scope,
            generation,
            values.into_iter().map(|(value, metadata)| (value, Some(Arc::new(metadata) as Metadata))).collect(),
        )
    }
    fn stage_values(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        values: Vec<(T, Option<Metadata>)>,
    ) -> Result<ExtensionRegistration<T>, ExtensionRegistryError> {
        check_count(values.len())?;
        let _lease = scope.try_acquire(generation)?;
        // On an early error, incoming T destructors must also run after unlocking.
        let mut incoming = Some(values);
        with_state(&self.state, |state| {
            if state.closed {
                return Err(ExtensionRegistryError::Closed);
            }
            state.purge_closed();
            if state.groups.contains_key(&owner) {
                return Err(ExtensionRegistryError::Conflict);
            }
            let count = incoming.as_ref().map_or(0, Vec::len);
            if state.groups.len() >= crate::MAX_PLUGINS || state.count() + count > state.capacity {
                return Err(ExtensionRegistryError::Capacity);
            }
            let token = state.next_token;
            state.next_token = token.checked_add(1).ok_or(ExtensionRegistryError::Capacity)?;
            let weak = Arc::downgrade(&self.state);
            let cleanup_owner = owner.clone();
            scope.register_cleanup(Box::new(move || {
                if let Some(state) = weak.upgrade() {
                    let (_, retired) = detach_state(&state, |state| state.remove(&cleanup_owner, token));
                    return Some(Box::new(retired));
                }
                None
            }))?;
            if scope.is_closed() {
                return Err(AdmissionError::Closed.into());
            }
            let admission = scope.admission();
            let cancellation = scope.cancellation_token();
            let batch = Batch::new(incoming.take().unwrap_or_default(), &cancellation);
            state
                .groups
                .insert(owner.clone(), Group { generation, token, admission: admission.clone(), cancellation, batch, published: false });
            Ok(ExtensionRegistration { state: Arc::downgrade(&self.state), owner, generation, token, admission })
        })
    }
    /// Permanently revokes every staged/published batch across all clones.
    /// Admitted operations receive cancellation and remain tracked by their scopes;
    /// this does not dispose unrelated tasks owned by those scopes.
    pub fn close(&self) {
        let (tokens, retired) = detach_state(&self.state, |state| {
            if state.closed {
                return state.closing_tokens.clone();
            }
            state.closed = true;
            for (_, group) in std::mem::take(&mut state.groups) {
                group.batch.revoke();
                state.closing_tokens.push(group.batch.cancellation.clone());
                state.retired.groups.push(group);
            }
            state.rebuild();
            state.closing_tokens.clone()
        });
        // Every concurrent close also signals the saved tokens before returning.
        // Wake every admitted operation before any native value destructor runs.
        for token in tokens {
            token.cancel();
        }
        drop(retired);
    }
    pub fn is_closed(&self) -> bool {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).closed
    }
    pub fn snapshot(&self) -> ExtensionSnapshot<T> {
        with_state(&self.state, |state| {
            state.purge_closed();
            state.snapshot.clone()
        })
    }
}

pub struct ExtensionRegistration<T> {
    state: Weak<Mutex<State<T>>>,
    owner: PluginId,
    generation: Generation,
    token: u64,
    admission: Admission,
}
impl<T> Clone for ExtensionRegistration<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            owner: self.owner.clone(),
            generation: self.generation,
            token: self.token,
            admission: self.admission.clone(),
        }
    }
}
impl<T> fmt::Debug for ExtensionRegistration<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtensionRegistration").field("owner", &self.owner).field("generation", &self.generation).finish_non_exhaustive()
    }
}
impl<T: Send + Sync + 'static> ExtensionRegistration<T> {
    pub fn owner(&self) -> &PluginId {
        &self.owner
    }
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    pub fn publish(&self) -> Result<(), ExtensionRegistryError> {
        if self.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        let state = self.state.upgrade().ok_or(ExtensionRegistryError::Stale)?;
        with_state(&state, |state| {
            if state.closed {
                return Err(ExtensionRegistryError::Closed);
            }
            state.purge_closed();
            let group = state
                .groups
                .get_mut(&self.owner)
                .filter(|group| group.token == self.token && group.generation == self.generation)
                .ok_or(ExtensionRegistryError::Stale)?;
            if group.admission.is_closed() {
                return Err(AdmissionError::Closed.into());
            }
            if !group.published {
                group.published = true;
                group.batch.active.store(true, Ordering::Release);
                state.rebuild();
            }
            Ok(())
        })
    }
    /// Revokes the old batch, including retained snapshots. Already admitted leases
    /// remain tracked and receive cancellation; callbacks must cooperate with it.
    pub fn replace(&self, values: Vec<T>) -> Result<(), ExtensionRegistryError> {
        self.replace_values(values.into_iter().map(|value| (value, None)).collect())
    }
    /// Metadata and callbacks change in the same atomic batch publication.
    pub fn replace_with_metadata<M: Send + Sync + 'static>(&self, values: Vec<(T, M)>) -> Result<(), ExtensionRegistryError> {
        self.replace_values(values.into_iter().map(|(value, metadata)| (value, Some(Arc::new(metadata) as Metadata))).collect())
    }
    fn replace_values(&self, values: Vec<(T, Option<Metadata>)>) -> Result<(), ExtensionRegistryError> {
        check_count(values.len())?;
        if self.admission.is_closed() {
            return Err(AdmissionError::Closed.into());
        }
        let state = self.state.upgrade().ok_or(ExtensionRegistryError::Stale)?;
        let mut incoming = Some(values);
        with_state(&state, |state| {
            if state.closed {
                return Err(ExtensionRegistryError::Closed);
            }
            state.purge_closed();
            let group = state
                .groups
                .get(&self.owner)
                .filter(|group| group.token == self.token && group.generation == self.generation)
                .ok_or(ExtensionRegistryError::Stale)?;
            if group.admission.is_closed() {
                return Err(AdmissionError::Closed.into());
            }
            if state.count() - group.batch.values.len() + incoming.as_ref().map_or(0, Vec::len) > state.capacity {
                return Err(ExtensionRegistryError::Capacity);
            }
            let group = state.groups.get_mut(&self.owner).ok_or(ExtensionRegistryError::Stale)?;
            let batch = Batch::new(incoming.take().unwrap_or_default(), &group.cancellation);
            let old = std::mem::replace(&mut group.batch, batch);
            old.revoke();
            state.retired.batches.push(old);
            if group.published {
                group.batch.active.store(true, Ordering::Release);
                state.rebuild();
            }
            Ok(())
        })
    }
    pub fn unpublish(&self) -> bool {
        self.state.upgrade().is_some_and(|state| with_state(&state, |state| state.remove(&self.owner, self.token)))
    }
}
fn check_count(count: usize) -> Result<(), ExtensionRegistryError> {
    if count == 0 || count > MAX_SCOPED_EXTENSIONS { Err(ExtensionRegistryError::Capacity) } else { Ok(()) }
}
