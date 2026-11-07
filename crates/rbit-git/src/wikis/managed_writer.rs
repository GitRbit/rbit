//! Managed, bounded preparation ownership. No ref, durability or completion receipt.
use super::{WikiObjectError, WikiObjects, layout, objects, writer_lock};
use rbit_kernel::AdmissionError;
use rbit_wikis::{WikiId, WikiRevisionId, commands::WikiIntent};
use std::{
    collections::HashSet,
    fmt,
    fs::File,
    sync::{Arc, Mutex},
};

struct Owner {
    objects: WikiObjects,
    active: Mutex<HashSet<WikiId>>,
    // Last: the lock outlives the owner's other state and all escaped scope leases.
    _root_lock: File,
}

/// Clones share the actual held root lock and nonqueued per-wiki gate. The bounded
/// set also counts prepared values waiting for the future SQL target-binding phase.
/// All old/direct preparation must honor this lock; external writers/GC are excluded.
#[derive(Clone)]
pub struct WikiObjectWriter {
    owner: Arc<Owner>,
}
impl fmt::Debug for WikiObjectWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WikiObjectWriter(..)")
    }
}

struct WikiScope {
    owner: Arc<Owner>,
    wiki: WikiId,
}
impl Drop for WikiScope {
    fn drop(&mut self) {
        // Poison leaves the owner fail-closed. Do not invent an idle/drained proof.
        if let Ok(mut active) = self.owner.active.lock() {
            active.remove(&self.wiki);
        }
    }
}

/// Exact unpublished preparation, retaining wiki/root ownership through SQL waits.
/// It is intentionally noncloneable and cannot authorize SQL visibility. Dropping
/// it releases ownership; the immutable objects may remain as bounded orphan work.
pub struct PreparedWikiObjects {
    intent: WikiIntent,
    revision: WikiRevisionId,
    _scope: WikiScope,
}
impl fmt::Debug for PreparedWikiObjects {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedWikiObjects(..)")
    }
}
impl PreparedWikiObjects {
    pub fn intent(&self) -> &WikiIntent {
        &self.intent
    }
    /// Syntactic target data, not a durable publication receipt.
    pub const fn revision(&self) -> WikiRevisionId {
        self.revision
    }
}

impl WikiObjectWriter {
    /// Startup only. Does not mint a validated prior head or enable network writes.
    /// Requires the declared owned local, stable topology/cooperating-writer model.
    pub fn open(objects: WikiObjects) -> Result<Self, WikiObjectError> {
        layout::verify_root(&objects.root, objects.identity)?;
        let root_lock = writer_lock::acquire(&objects.root)?;
        let capacity = objects.budget.capacity().get();
        Ok(Self { owner: Arc::new(Owner { objects, active: Mutex::new(HashSet::with_capacity(capacity)), _root_lock: root_lock }) })
    }

    fn reserve(&self, wiki: WikiId) -> Result<WikiScope, WikiObjectError> {
        let mut active = self.owner.active.lock().map_err(|_| WikiObjectError::Binding)?;
        if active.contains(&wiki) || active.len() >= self.owner.objects.budget.capacity().get() {
            return Err(AdmissionError::Busy.into());
        }
        active.insert(wiki);
        Ok(WikiScope { owner: Arc::clone(&self.owner), wiki })
    }

    /// Only the future private coordinator may treat this input as an accepted SQL
    /// fact. Constructing `WikiIntent` or preparing objects is never authentication.
    /// Cancellation keeps root/wiki/admission ownership in the real blocking worker.
    pub async fn prepare(&self, intent: WikiIntent) -> Result<PreparedWikiObjects, WikiObjectError> {
        if intent.store != self.owner.objects.identity {
            return Err(WikiObjectError::Binding);
        }
        let reservation = self.owner.objects.budget.try_reserve()?;
        let scope = self.reserve(intent.command.wiki())?;
        reservation
            .spawn(move || {
                let objects = &scope.owner.objects;
                layout::verify_root(&objects.root, objects.identity)?;
                let revision = objects::prepare_intent(&objects.root, &intent)?;
                Ok(PreparedWikiObjects { intent, revision, _scope: scope })
            })
            .await?
    }
}
