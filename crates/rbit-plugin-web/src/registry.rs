use std::{collections::BTreeSet, sync::Arc};
use tokio::sync::Semaphore;

use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::{ExtensionRegistration, ExtensionSnapshot, PluginScope, ScopedExtensionRegistry};

use crate::{HookRegistration, HookRegistrationError, MAX_CONCURRENT_PIPELINES, MAX_HOOKS};

#[derive(Debug, Clone)]
pub struct RequestHookRegistry {
    inner: ScopedExtensionRegistry<HookRegistration>,
    pipelines: Arc<Semaphore>,
}
impl Default for RequestHookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl RequestHookRegistry {
    pub fn new() -> Self {
        Self {
            inner: ScopedExtensionRegistry::with_capacity(MAX_HOOKS).expect("host hook capacity is valid"),
            pipelines: Arc::new(Semaphore::new(MAX_CONCURRENT_PIPELINES)),
        }
    }
    pub fn stage(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        hooks: Vec<HookRegistration>,
    ) -> Result<RequestHookRegistration, HookRegistrationError> {
        validate(&hooks)?;
        Ok(RequestHookRegistration { inner: self.inner.stage(owner, scope, generation, hooks)? })
    }
    /// Terminally withdraws all callbacks and cancels admitted work across clones.
    /// The host still owns disposal of any independent plugin scope tasks.
    pub fn close(&self) {
        self.inner.close();
    }
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
    pub fn revision(&self) -> u64 {
        self.inner.snapshot().revision
    }
    pub fn snapshot(&self) -> RequestHookSnapshot {
        RequestHookSnapshot { inner: self.inner.snapshot(), pipelines: Arc::clone(&self.pipelines) }
    }
}

#[derive(Debug, Clone)]
pub struct RequestHookRegistration {
    inner: ExtensionRegistration<HookRegistration>,
}
impl RequestHookRegistration {
    pub fn publish(&self) -> Result<(), HookRegistrationError> {
        Ok(self.inner.publish()?)
    }
    pub fn replace(&self, hooks: Vec<HookRegistration>) -> Result<(), HookRegistrationError> {
        validate(&hooks)?;
        Ok(self.inner.replace(hooks)?)
    }
    pub fn unpublish(&self) -> bool {
        self.inner.unpublish()
    }
}

#[derive(Debug, Clone)]
pub struct RequestHookSnapshot {
    pub(crate) inner: ExtensionSnapshot<HookRegistration>,
    pub(crate) pipelines: Arc<Semaphore>,
}
impl RequestHookSnapshot {
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }
    pub const fn revision(&self) -> u64 {
        self.inner.revision
    }
}
fn validate(hooks: &[HookRegistration]) -> Result<(), HookRegistrationError> {
    let mut ids = BTreeSet::new();
    if hooks.is_empty() || hooks.len() > MAX_HOOKS || hooks.iter().any(|hook| !ids.insert(hook.id())) {
        return Err(HookRegistrationError::Invalid("invalid or duplicate hook batch"));
    }
    Ok(())
}
