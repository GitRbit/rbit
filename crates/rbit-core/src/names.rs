//! Compatibility paths for feature-owned validated names.

pub use rbit_contracts::NameError;
pub use rbit_identity_models::{AccountName, Email};
pub use rbit_repositories::RepoName;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_names() {
        for name in ["alice", "a", "rust-lang", "my_repo.rs", "Repo2"] {
            assert!(name.parse::<RepoName>().is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_traversal_and_git_suffixes() {
        for name in ["", "..", "../etc", "a/b", ".hidden", "trailing-", "repo.git", "x..y", "has space", "é"] {
            assert!(name.parse::<RepoName>().is_err(), "{name:?} should be rejected");
        }
    }

    #[test]
    fn reserved_account_names_are_case_insensitive() {
        assert!(matches!("Settings".parse::<AccountName>(), Err(NameError::Reserved { .. })));
    }

    #[test]
    fn dashboards_are_reserved_account_names() {
        for name in ["issues", "Pulls", "notifications"] {
            assert!(matches!(name.parse::<AccountName>(), Err(NameError::Reserved { .. })), "{name}");
        }
    }

    #[test]
    fn organisation_pages_are_reserved_repository_names() {
        for name in ["members", "People", "settings", "teams"] {
            assert!(matches!(name.parse::<RepoName>(), Err(NameError::Reserved { .. })), "{name}");
        }
    }

    #[test]
    fn email_requires_domain_with_dot() {
        assert!("dev@example.com".parse::<Email>().is_ok());
        assert!("dev@localhost".parse::<Email>().is_err());
        assert!("@example.com".parse::<Email>().is_err());
    }
}
