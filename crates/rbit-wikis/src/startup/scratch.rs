//! Explicit scratch resource policy, not a readiness or historical-integrity proof.
#[derive(Debug, Clone, Copy)]
pub struct WikiScratchLimits {
    scopes: u32,
    objects: u32,
    edges: u32,
    logical_bytes: u64,
}
impl WikiScratchLimits {
    pub const fn new(scopes: u32, objects: u32, edges: u32, logical_bytes: u64) -> Option<Self> {
        if scopes > 0 && objects > 0 && edges > 0 && logical_bytes > 0 && logical_bytes <= i64::MAX as u64 {
            Some(Self { scopes, objects, edges, logical_bytes })
        } else {
            None
        }
    }
    pub const fn scopes(self) -> u32 {
        self.scopes
    }
    pub const fn objects(self) -> u32 {
        self.objects
    }
    pub const fn edges(self) -> u32 {
        self.edges
    }
    pub const fn logical_bytes(self) -> u64 {
        self.logical_bytes
    }
}

/// Fixed record charges bound stored payload, NOT physical SQLite page/WAL size.
/// Filesystem headroom is an additional mandatory coordinator admission gate.
pub const SCOPE_CHARGE: i64 = 64;
pub const OBJECT_CHARGE: i64 = 128;
pub const EDGE_CHARGE: i64 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WikiScratchUsage {
    pub scopes: u32,
    pub objects: u32,
    pub edges: u32,
    pub logical_bytes: u64,
}
