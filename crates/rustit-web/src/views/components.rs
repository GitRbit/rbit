//! Small, reusable view models shared across pages.

use std::collections::HashMap;

use rustit_core::{Account, OrgMembership, RepoId, repository::OwnedRepository};

/// Number of avatar colour variants defined in `styles/app.css` (`.avatar-0` … `.avatar-7`).
const AVATAR_PALETTE_SIZE: u64 = 8;

/// A generated avatar: initials on a stable gradient.
#[derive(Debug, Clone)]
pub struct Avatar {
    pub initials: String,
    pub palette: u64,
}

impl Avatar {
    pub fn for_account(account: &Account) -> Self {
        Self { initials: initials(account.label()), palette: account.id.0.unsigned_abs() % AVATAR_PALETTE_SIZE }
    }

    /// For identities without an account (commit authors), seeded by e.g. their email.
    pub fn for_identity(label: &str, seed: &str) -> Self {
        // FNV-1a: tiny, stable across releases and platforms.
        let hash = seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
        Self { initials: initials(label), palette: hash % AVATAR_PALETTE_SIZE }
    }

    pub fn class(&self) -> String {
        format!("avatar avatar-{}", self.palette)
    }
}

/// One or two upper-case initials from a display name or handle.
fn initials(label: &str) -> String {
    let mut words = label.split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.').filter(|w| !w.is_empty());
    let first = words.next().and_then(|w| w.chars().next());
    let second = words.next().and_then(|w| w.chars().next());
    let initials: String = first.into_iter().chain(second).flat_map(char::to_uppercase).collect();
    if initials.is_empty() { "?".to_owned() } else { initials }
}

/// A repository in a list (dashboard, explore, profile).
#[derive(Debug, Clone)]
pub struct RepoCard {
    pub owner: String,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub updated: String,
    pub updated_iso: String,
    pub owner_avatar: Avatar,
    pub star_count: i64,
}

impl RepoCard {
    /// Builds cards for a listing, attaching star counts from one batched lookup.
    pub fn list(repos: &[OwnedRepository], star_counts: &HashMap<RepoId, i64>) -> Vec<Self> {
        repos.iter().map(|repo| Self { star_count: star_counts.get(&repo.repo.id).copied().unwrap_or(0), ..Self::from(repo) }).collect()
    }
}

impl From<&OwnedRepository> for RepoCard {
    fn from(item: &OwnedRepository) -> Self {
        Self {
            owner: item.owner.name.to_string(),
            name: item.repo.name.to_string(),
            url: format!("/{}/{}", item.owner.name, item.repo.name),
            description: item.repo.description.clone(),
            is_private: !item.repo.visibility.is_public(),
            updated: item.repo.updated_at.relative(),
            updated_iso: item.repo.updated_at.iso8601(),
            owner_avatar: Avatar::for_account(&item.owner),
            star_count: 0,
        }
    }
}

/// An organisation the viewer belongs to (dashboard sidebar).
#[derive(Debug, Clone)]
pub struct OrgLink {
    pub name: String,
    pub label: String,
    pub url: String,
    pub avatar: Avatar,
    pub role_label: &'static str,
}

impl From<&OrgMembership> for OrgLink {
    fn from(membership: &OrgMembership) -> Self {
        Self {
            name: membership.org.name.to_string(),
            label: membership.org.label().to_owned(),
            url: format!("/{}", membership.org.name),
            avatar: Avatar::for_account(&membership.org),
            role_label: membership.role.label(),
        }
    }
}

/// "Previous / Page n of m / Next" links under a paged list. A missing URL renders as a
/// disabled control.
#[derive(Debug, Clone, Default)]
pub struct Pagination {
    pub page: u32,
    pub page_count: u32,
    pub previous_url: Option<String>,
    pub next_url: Option<String>,
}

impl Pagination {
    /// Whether there is more than one page (otherwise the control is not shown).
    pub fn is_needed(&self) -> bool {
        self.page_count > 1
    }
}

/// A form validation message shown above a form.
#[derive(Debug, Clone, Default)]
pub struct FormError(pub Option<String>);

impl FormError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(Some(message.into()))
    }

    /// Wraps a service message (a lower-case fragment such as "name is already taken")
    /// as a displayable sentence: "Name is already taken."
    pub fn from_message(message: &str) -> Self {
        let mut chars = message.chars();
        let sentence = chars.next().map(|first| first.to_uppercase().chain(chars).collect::<String>() + ".").unwrap_or_default();
        Self::new(sentence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_from_names() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("rust-lang"), "RL");
        assert_eq!(initials("alice"), "A");
        assert_eq!(initials(""), "?");
    }
}
