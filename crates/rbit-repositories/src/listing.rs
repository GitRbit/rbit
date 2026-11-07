//! Stable repository listing selectors and legacy presentation labels.

use serde::{Deserialize, Serialize};

/// How a list of repositories is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RepoSort {
    /// Most recently updated (or pushed to) first.
    #[default]
    RecentlyUpdated,
    /// Alphabetical by name, case-insensitively.
    Name,
    /// Most recently created first.
    Newest,
}

impl RepoSort {
    /// Every order, in the order a sort picker lists them.
    pub const ALL: [Self; 3] = [Self::RecentlyUpdated, Self::Name, Self::Newest];

    /// The value used in query strings (`?sort=…`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RecentlyUpdated => "updated",
            Self::Name => "name",
            Self::Newest => "newest",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "updated" => Some(Self::RecentlyUpdated),
            "name" => Some(Self::Name),
            "newest" => Some(Self::Newest),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RecentlyUpdated => "Recently updated",
            Self::Name => "Name",
            Self::Newest => "Newest",
        }
    }
}

/// Which repositories a listing shows by archive state (`?archived=…`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArchivedFilter {
    /// Archived and active repositories alike.
    #[default]
    Include,
    /// Only repositories that are not archived (`archived=false`).
    Exclude,
    /// Only archived repositories (`archived=true`).
    Only,
}

impl ArchivedFilter {
    /// Every filter, in the order a filter picker lists them.
    pub const ALL: [Self; 3] = [Self::Include, Self::Exclude, Self::Only];

    /// The value used in query strings; `None` for the default, which has none.
    pub const fn query_value(self) -> Option<&'static str> {
        match self {
            Self::Include => None,
            Self::Exclude => Some("false"),
            Self::Only => Some("true"),
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "" => Some(Self::Include),
            "false" => Some(Self::Exclude),
            "true" => Some(Self::Only),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Include => "All",
            Self::Exclude => "Active",
            Self::Only => "Archived",
        }
    }

    /// Whether a repository that is `archived` (or not) is listed.
    pub const fn shows(self, archived: bool) -> bool {
        match self {
            Self::Include => true,
            Self::Exclude => !archived,
            Self::Only => archived,
        }
    }
}
