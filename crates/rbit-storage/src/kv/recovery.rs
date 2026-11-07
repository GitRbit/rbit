//! Nonqueued recovery ownership shared by every composition of one KV backend.
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Clone, Debug, Default)]
pub struct CredentialRecoveryGate(Arc<Mutex<()>>);
/// A held operation, not permission to reconstruct an intent or ignore SQL fences.
#[derive(Debug)]
pub struct CredentialRecoveryPermit {
    _guard: OwnedMutexGuard<()>,
}
impl CredentialRecoveryGate {
    pub fn try_acquire(&self) -> Option<CredentialRecoveryPermit> {
        Arc::clone(&self.0).try_lock_owned().ok().map(|guard| CredentialRecoveryPermit { _guard: guard })
    }
}
