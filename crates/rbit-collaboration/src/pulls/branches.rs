//! Branch names a pull request may name.

/// `refs/heads/{branch}` for a plausible branch name; `None` for an empty name, one
/// starting with `-`, or one with a space, control or revision-syntax character or `..`.
pub fn branch_ref(branch: &str) -> Option<String> {
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.contains(['\0', '\n', ' ', ':', '~', '^', '?', '*', '[', '\\'])
        || branch.contains("..")
    {
        return None;
    }
    Some(format!("refs/heads/{branch}"))
}

#[cfg(test)]
mod tests {
    use super::branch_ref;

    #[test]
    fn plausible_branch_names_only() {
        assert_eq!(branch_ref("feature/x").as_deref(), Some("refs/heads/feature/x"));
        for bad in ["", "-x", "a b", "a:b", "a~1", "a^", "a?", "a*", "a[", "a\\b", "a..b", "a\nb", "a\0"] {
            assert_eq!(branch_ref(bad), None, "{bad:?}");
        }
    }
}
