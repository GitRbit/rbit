//! Account identity shared by feature ports and persisted references.

use serde::{Deserialize, Serialize};

/// Stable, never-reused primary key of an account. Used for on-disk paths so that
/// renaming an account never moves files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AccountId(pub i64);
