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
    fn sort_round_trips() {
        for sort in RepoSort::ALL {
            assert_eq!(RepoSort::parse(sort.as_str()), Some(sort));
        }
        assert_eq!(RepoSort::parse("stars"), None);
        assert_eq!(RepoSort::default(), RepoSort::RecentlyUpdated);
    }
}
