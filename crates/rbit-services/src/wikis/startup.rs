//! Real native-to-SQL scratch composition, still no integrity/durability readiness.
use rbit_git::wikis::{CatalogBoundWikiObjects, WikiLooseScan, WikiObjectError};
use rbit_storage::sqlite::wiki_mutations::full::{
    WikiCatalogHandle, WikiWriteError,
    fencing::FencedWikiRecovery,
    scratch::{WikiScratchEdgeBatch, WikiScratchEdgeCursor, WikiScratchLedger, WikiScratchScope},
};
use rbit_wikis::startup::{
    WikiStartupBatchSize, WikiStartupScope,
    objects::WikiScanBudget,
    scratch::{WikiScratchLimits, WikiScratchUsage},
};
use std::fmt;

pub type WikiNativeObjects = CatalogBoundWikiObjects<WikiCatalogHandle>;

#[derive(Debug, thiserror::Error)]
pub enum WikiStartupScanError {
    #[error("wiki startup SQL scratch failed")]
    Sql(#[from] WikiWriteError),
    #[error("wiki startup native scan failed")]
    Objects(#[from] WikiObjectError),
    #[error("wiki startup progress is uncertain; restart the owned scan pass")]
    Poisoned,
    #[error("wiki startup scope scan has already ended")]
    Ended,
}
/// Explicit stopped composition. Does not spawn a server task or declare ready.
pub struct WikiStartupCatalog {
    ledger: WikiScratchLedger<WikiNativeObjects>,
}
impl fmt::Debug for WikiStartupCatalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiStartupCatalog(..)")
    }
}
/// One native continuation plus SQL scope. No full-history DTO or body buffer.
pub struct WikiStartupScopeScan {
    scope: WikiScratchScope<WikiNativeObjects>,
    cursor: Option<WikiLooseScan<WikiNativeObjects>>,
    metadata: WikiStartupScope,
    poisoned: bool,
    ended: bool,
}
impl fmt::Debug for WikiStartupScopeScan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WikiStartupScopeScan(..)")
    }
}
#[derive(Debug, Clone, Copy)]
pub struct WikiStartupProgress {
    pub objects_hashed: u16,
    pub bytes_hashed: u32,
    pub records_applied: u16,
    pub scope_ended: bool,
    pub usage: Option<WikiScratchUsage>,
}
impl WikiStartupCatalog {
    /// The caller must complete bounded old-scratch cleanup on this exact recovery
    /// owner first. No persisted ledger phase is adopted as validation evidence.
    pub async fn from_recovery(
        recovery: FencedWikiRecovery<WikiNativeObjects>,
        limits: WikiScratchLimits,
    ) -> Result<Self, WikiStartupScanError> {
        Ok(Self { ledger: recovery.begin_scratch(limits).await? })
    }
    /// Metadata must originate from stopped enumeration. SQL rechecks the actual
    /// catalog/pending identity; this does not accept arbitrary roots or OIDs.
    pub async fn scope(&self, metadata: WikiStartupScope) -> Result<WikiStartupScopeScan, WikiStartupScanError> {
        let scope = self.ledger.scope(metadata.wiki, metadata.repository).await?;
        let cursor = self.ledger.objects().scan_loose(metadata.repository, metadata.wiki).await?;
        Ok(WikiStartupScopeScan { scope, cursor: Some(cursor), metadata, poisoned: false, ended: false })
    }
    pub async fn validate_edges(
        &self,
        cursor: Option<WikiScratchEdgeCursor<WikiNativeObjects>>,
        size: WikiStartupBatchSize,
    ) -> Result<WikiScratchEdgeBatch<WikiNativeObjects>, WikiStartupScanError> {
        Ok(self.ledger.validate_edges(cursor, size).await?)
    }
    /// SQL-only drain. Live native continuations/workers retain actual resources;
    /// a caller must not infer whole-root destruction from this method.
    pub async fn close(&self) -> Result<(), WikiStartupScanError> {
        Ok(self.ledger.close().await?)
    }
}
impl WikiStartupScopeScan {
    pub const fn metadata(&self) -> WikiStartupScope {
        self.metadata
    }
    /// Cancellation/error poisons this continuation BEFORE its first await. Never
    /// advance a native cursor after an uncertain SQL ACK or invent an EOF marker.
    pub async fn advance(&mut self, budget: WikiScanBudget) -> Result<WikiStartupProgress, WikiStartupScanError> {
        if self.poisoned {
            return Err(WikiStartupScanError::Poisoned);
        }
        if self.ended {
            return Err(WikiStartupScanError::Ended);
        }
        self.poisoned = true;
        let cursor = self.cursor.take().ok_or(WikiStartupScanError::Poisoned)?;
        let batch = cursor.next(budget).await?;
        let objects_hashed = batch.objects_hashed();
        let bytes_hashed = batch.bytes_hashed();
        let records_applied = u16::try_from(batch.records().len()).map_err(|_| WikiStartupScanError::Poisoned)?;
        let (records, next) = batch.into_parts();
        let mut usage = if records.is_empty() { None } else { Some(self.scope.record(records).await?) };
        let scope_ended = next.is_none();
        if scope_ended {
            usage = Some(self.scope.finish().await?);
        }
        self.cursor = next;
        self.ended = scope_ended;
        self.poisoned = false;
        Ok(WikiStartupProgress { objects_hashed, bytes_hashed, records_applied, scope_ended, usage })
    }
}
