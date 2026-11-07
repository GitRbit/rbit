//! Bounded native scan facts. None of these values is a durable head capability.
use crate::{WikiPageId, WikiRevisionId};
use rbit_contracts::{AccountId, Timestamp};

pub const MAX_SCAN_RECORDS: u16 = 256;
pub const MAX_SCAN_OBJECT_BYTES: u32 = 1024 * 1024;

/// Enough room for one maximum tree, with an explicit upper bound on each batch.
#[derive(Debug, Clone, Copy)]
pub struct WikiScanBudget {
    records: u16,
    bytes: u32,
}
impl WikiScanBudget {
    pub const fn new(records: u16, bytes: u32) -> Option<Self> {
        if records > 0 && records <= MAX_SCAN_RECORDS && bytes >= MAX_SCAN_OBJECT_BYTES && bytes <= 16 * MAX_SCAN_OBJECT_BYTES {
            Some(Self { records, bytes })
        } else {
            None
        }
    }
    pub const fn records(self) -> u16 {
        self.records
    }
    pub const fn bytes(self) -> u32 {
        self.bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiScannedKind {
    Blob,
    Tree,
    Commit,
}

/// The object row precedes its ordered edges; Complete follows full semantic
/// validation. A failed/interrupted scan cannot promote an incomplete row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiScanRecord {
    Object { id: WikiRevisionId, kind: WikiScannedKind, bytes: u32 },
    Commit { id: WikiRevisionId, actor: AccountId, incarnation: Option<i64>, at: Timestamp },
    Edge { source: WikiRevisionId, ordinal: u32, target: WikiRevisionId, kind: WikiScannedKind, page: Option<WikiPageId> },
    Complete { id: WikiRevisionId },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_scan_budgets_allow_a_maximum_tree_but_refuse_zero_and_unbounded_batches() {
        assert!(WikiScanBudget::new(0, MAX_SCAN_OBJECT_BYTES).is_none());
        assert!(WikiScanBudget::new(MAX_SCAN_RECORDS + 1, MAX_SCAN_OBJECT_BYTES).is_none());
        assert!(WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES - 1).is_none());
        assert!(WikiScanBudget::new(1, 16 * MAX_SCAN_OBJECT_BYTES + 1).is_none());
        let budget = WikiScanBudget::new(1, MAX_SCAN_OBJECT_BYTES).unwrap();
        assert_eq!((budget.records(), budget.bytes()), (1, MAX_SCAN_OBJECT_BYTES));
    }
}
