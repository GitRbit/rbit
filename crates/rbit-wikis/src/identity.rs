//! Stable page identity is separate from a wiki's mutable visible generation.

/// Positive SQLite page identity, never derived from title/path or reused on rename.
/// The catalog adapter owns allocation/nonreuse; construction grants no authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WikiPageId(i64);

impl WikiPageId {
    pub const fn new(value: i64) -> Option<Self> {
        if value > 0 { Some(Self(value)) } else { None }
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

/// Repository-scoped visible wiki generation, shared by all page projections.
/// Every accepted mutation advances it, including rename/restore/tombstone changes.
/// A prospective next value is data, not proof that a transaction has committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WikiGeneration(i64);

impl WikiGeneration {
    pub const INITIAL: Self = Self(1);

    pub const fn new(value: i64) -> Option<Self> {
        if value > 0 { Some(Self(value)) } else { None }
    }

    pub const fn get(self) -> i64 {
        self.0
    }

    /// Exhaustion is a refusal, never a wrap/reset that revives an old edit token.
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}
