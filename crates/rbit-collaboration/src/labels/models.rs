//! Label records shared by issue and pull-request catalog workflows.

use rbit_contracts::{RepoId, Timestamp};
use serde::{Deserialize, Serialize};

/// Database id of a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LabelId(pub i64);

/// A label as it was when an event happened (labels can be renamed or deleted later).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabelSnapshot {
    pub id: LabelId,
    pub name: String,
    /// `#rrggbb`, lower case.
    pub color: String,
}

/// A repository's label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub id: LabelId,
    pub repo_id: RepoId,
    pub name: String,
    /// `#rrggbb`, lower case.
    pub color: String,
    pub description: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Label {
    /// The label as timeline events and payloads record it.
    pub fn snapshot(&self) -> LabelSnapshot {
        LabelSnapshot { id: self.id, name: self.name.clone(), color: self.color.clone() }
    }
}

/// A label's fields, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelFields {
    pub name: String,
    pub color: String,
    pub description: String,
}

/// A label with the number of open threads carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelUsage {
    pub label: Label,
    pub open_issues: u64,
}

/// A label as submitted (colour with or without `#`, any case).
#[derive(Debug, Clone, Default)]
pub struct LabelInput {
    pub name: String,
    pub color: String,
    pub description: String,
}
