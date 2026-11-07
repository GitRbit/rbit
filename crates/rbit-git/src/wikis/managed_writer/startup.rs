//! Once-per-loose-file validation with retained iterators and bounded record batches.
mod loose;
mod pending;
#[cfg(test)]
mod tests;
mod walk;
use super::{WikiObjectError, WikiObjectWriter, WikiScope};
use crate::wikis::layout;
use rbit_core::RepoId;
use rbit_wikis::{
    WikiId,
    ownership::WikiObjectOwnership,
    startup::objects::{MAX_SCAN_OBJECT_BYTES, WikiScanBudget, WikiScanRecord},
};
use std::fmt;

/// Private continuation retains the actual root/wiki gate and catalog owner.
/// It cannot be reconstructed from persistent IDs, serialized or cloned.
pub struct WikiLooseScan<K> {
    walk: walk::Walk,
    pending: Option<pending::Pending>,
    keepalive: K,
    scope: WikiScope,
}
impl<K> fmt::Debug for WikiLooseScan<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiLooseScan(..)")
    }
}
pub struct WikiScanBatch<K> {
    records: Vec<WikiScanRecord>,
    next: Option<WikiLooseScan<K>>,
    objects_hashed: u16,
    bytes_hashed: u32,
}
impl<K> fmt::Debug for WikiScanBatch<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiScanBatch(..)")
    }
}
impl<K> WikiScanBatch<K> {
    pub fn records(&self) -> &[WikiScanRecord] {
        &self.records
    }
    pub const fn objects_hashed(&self) -> u16 {
        self.objects_hashed
    }
    pub const fn bytes_hashed(&self) -> u32 {
        self.bytes_hashed
    }
    pub fn into_parts(self) -> (Vec<WikiScanRecord>, Option<WikiLooseScan<K>>) {
        (self.records, self.next)
    }
}
impl WikiObjectWriter {
    pub(in crate::wikis) async fn start_scan_retaining<K: WikiObjectOwnership>(
        &self,
        repository: RepoId,
        wiki: WikiId,
        keepalive: K,
    ) -> Result<WikiLooseScan<K>, WikiObjectError> {
        if keepalive.store_id() != self.store_id() {
            return Err(WikiObjectError::Binding);
        }
        let reservation = self.owner.objects.budget.try_reserve()?;
        let scope = self.reserve(wiki)?;
        reservation
            .spawn(move || {
                keepalive.check_objects_bound().map_err(|_| WikiObjectError::Binding)?;
                let objects = &scope.owner.objects;
                let repo = layout::repository(&objects.root, objects.identity, repository, wiki, false)?;
                if repo.object_hash() != gix::hash::Kind::Sha1 {
                    return Err(WikiObjectError::Format);
                }
                let walk = walk::Walk::open(&repo.path().join("objects"))?;
                keepalive.check_objects_bound().map_err(|_| WikiObjectError::Binding)?;
                Ok(WikiLooseScan { walk, pending: None, keepalive, scope })
            })
            .await?
    }
}
impl<K: WikiObjectOwnership> WikiLooseScan<K> {
    /// Every real worker retains the continuation and actual catalog/root guards.
    /// Drop of its awaiter cannot release them while the worker is queued/running.
    /// Empty progress batches are possible while advancing fixed fanout directories.
    pub async fn next(self, budget: WikiScanBudget) -> Result<WikiScanBatch<K>, WikiObjectError> {
        let reservation = self.scope.owner.objects.budget.try_reserve()?;
        reservation.spawn(move || self.advance(budget)).await?
    }
    fn advance(mut self, budget: WikiScanBudget) -> Result<WikiScanBatch<K>, WikiObjectError> {
        self.keepalive.check_objects_bound().map_err(|_| WikiObjectError::Binding)?;
        let mut records = Vec::with_capacity(usize::from(budget.records()));
        let mut objects_hashed = 0;
        let mut bytes_hashed = 0;
        let mut end = false;
        // Directory entries, object starts and edge emission each consume a step.
        // Reserve maximum decoded space before starting another loose object.
        for _ in 0..budget.records() {
            if let Some(pending) = &mut self.pending {
                if let Some(record) = pending.next()? {
                    records.push(record);
                } else {
                    self.pending = None;
                }
                continue;
            }
            if budget.bytes() - bytes_hashed < MAX_SCAN_OBJECT_BYTES {
                break;
            }
            match self.walk.next()? {
                walk::Step::Progress => {}
                walk::Step::End => {
                    end = true;
                    break;
                }
                walk::Step::Object(path, id) => {
                    let decoded = loose::read(&path, id)?;
                    bytes_hashed += u32::try_from(decoded.data.len() - decoded.offset).map_err(|_| WikiObjectError::Format)?;
                    objects_hashed += 1;
                    self.pending = Some(pending::Pending::new(id, decoded)?);
                }
            }
        }
        self.keepalive.check_objects_bound().map_err(|_| WikiObjectError::Binding)?;
        Ok(WikiScanBatch { records, next: if end { None } else { Some(self) }, objects_hashed, bytes_hashed })
    }
}
