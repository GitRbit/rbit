//! Repository metadata as stored in the relational database. The Git data itself lives
//! in a bare repository on disk, addressed by [`RepoId`].

use serde::{Deserialize, Serialize};

use crate::{Account, AccountId, RepoName, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RepoId(pub i64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Visibility {
    #[default]
    Public,
    Private,
}

impl Visibility {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }

    pub const fn is_public(self) -> bool {
        matches!(self, Self::Public)
    }
}

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

#[derive(Debug, Clone)]
pub struct Repository {
    pub id: RepoId,
    pub owner_id: AccountId,
    pub name: RepoName,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub pushed_at: Option<Timestamp>,
    /// When the repository was archived; `None` while it accepts changes.
    pub archived_at: Option<Timestamp>,
}

/// Why an archived repository refuses a change, shown to people and Git clients alike.
pub const ARCHIVED_READ_ONLY: &str = "This repository is archived and read-only; an administrator can unarchive it in its settings";

impl Repository {
    /// Archived repositories are read-only: they can be read, cloned, forked and starred,
    /// but nothing in them changes until they are unarchived.
    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// A repository together with its owner — the shape almost every page needs.
#[derive(Debug, Clone)]
pub struct OwnedRepository {
    pub owner: Account,
    pub repo: Repository,
}

impl OwnedRepository {
    /// `owner/name`, as shown in headings and used in URLs.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner.name, self.repo.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archived_filters_round_trip() {
        for filter in ArchivedFilter::ALL {
            assert_eq!(ArchivedFilter::parse(filter.query_value().unwrap_or_default()), Some(filter));
        }
        assert_eq!(ArchivedFilter::parse("maybe"), None);
        assert!(ArchivedFilter::Include.shows(true) && ArchivedFilter::Include.shows(false));
        assert!(!ArchivedFilter::Exclude.shows(true) && ArchivedFilter::Only.shows(true) && !ArchivedFilter::Only.shows(false));
    }

    #[test]
    fn sort_round_trips() {
        for sort in RepoSort::ALL {
            assert_eq!(RepoSort::parse(sort.as_str()), Some(sort));
        }
        assert_eq!(RepoSort::parse("stars"), None);
        assert_eq!(RepoSort::default(), RepoSort::RecentlyUpdated);
    }
}
