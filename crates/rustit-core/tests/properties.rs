//! Property tests for validated names: whatever the input, a name that could escape a
//! directory, hide a file or masquerade as a Git directory is never accepted.

use proptest::prelude::*;
use rustit_core::{AccountName, RepoName};

/// Parses `value` as both kinds of name; returns which ones accepted it.
fn accepted(value: &str) -> (bool, bool) {
    (value.parse::<AccountName>().is_ok(), value.parse::<RepoName>().is_ok())
}

/// Whether `value` ends in a Git-meaningful suffix, ignoring case.
fn has_git_suffix(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".git", ".lock"].iter().any(|suffix| lower.ends_with(suffix))
}

/// Strings built from the characters names may contain plus the dangerous ones.
fn name_like() -> impl Strategy<Value = String> {
    "[A-Za-z0-9._/-]{0,24}"
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn slashes_are_never_accepted(a in name_like(), b in name_like()) {
        let value = format!("{a}/{b}");
        prop_assert_eq!(accepted(&value), (false, false));
        let backslash = format!("{a}\\{b}");
        prop_assert_eq!(accepted(&backslash), (false, false));
    }

    #[test]
    fn parent_references_are_never_accepted(a in name_like(), b in name_like()) {
        prop_assert_eq!(accepted(&format!("{a}..{b}")), (false, false));
    }

    #[test]
    fn leading_dots_are_never_accepted(rest in name_like()) {
        prop_assert_eq!(accepted(&format!(".{rest}")), (false, false));
    }

    #[test]
    fn git_suffixes_are_never_accepted(stem in name_like(), suffix in prop::sample::select(vec![".git", ".GIT", ".Git", ".lock"])) {
        prop_assert_eq!(accepted(&format!("{stem}{suffix}")), (false, false));
    }

    /// Whatever is accepted is a plain slug, kept verbatim.
    #[test]
    fn accepted_names_are_safe_slugs(value in prop_oneof![any::<String>(), "[A-Za-z0-9._-]{1,40}"]) {
        let parsed = [
            (value.parse::<AccountName>().map(String::from), AccountName::MAX_LEN),
            (value.parse::<RepoName>().map(String::from), RepoName::MAX_LEN),
        ];
        for (result, max) in parsed {
            if let Ok(name) = result {
                prop_assert_eq!(&name, &value);
                prop_assert!(!name.is_empty() && name.len() <= max);
                prop_assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')));
                prop_assert!(name.as_bytes()[0].is_ascii_alphanumeric());
                prop_assert!(!name.contains("..") && !has_git_suffix(&name));
            }
        }
    }

    /// Ordinary slugs (alphanumeric ends, single separators) are accepted. The digit
    /// keeps them clear of the reserved route names.
    #[test]
    fn ordinary_slugs_are_accepted(value in "[a-zA-Z]{0,6}[0-9]([a-zA-Z0-9]|[-_.][a-zA-Z0-9]){0,15}") {
        prop_assume!(!has_git_suffix(&value));
        prop_assert!(value.parse::<AccountName>().is_ok(), "{} should be a valid account name", value);
        prop_assert!(value.parse::<RepoName>().is_ok(), "{} should be a valid repository name", value);
    }
}
