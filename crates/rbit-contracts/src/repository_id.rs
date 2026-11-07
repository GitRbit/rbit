//! Repository identity shared by feature ports and persisted references.

use serde::{Deserialize, Serialize};

/// Repository primary key, retaining the existing public signed-integer constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RepoId(pub i64);
