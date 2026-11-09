//! Which pushed commits are new: what a branch update's commit walk excludes.

/// The commits to exclude when listing what an update to `after` brought: the previous
/// tip when the branch existed (`before`), otherwise every other ref tip of the
/// repository (`tips`, taken after the push), skipping one occurrence of `after`
/// itself. Without either, nothing is excluded.
pub fn push_exclusions<'a>(before: Option<&str>, tips: Option<impl Iterator<Item = &'a str>>, after: &str) -> Vec<String> {
    match (before, tips) {
        (Some(before), _) => vec![before.to_owned()],
        (None, Some(tips)) => {
            let mut skipped = false;
            tips.filter(|tip| {
                if !skipped && *tip == after {
                    skipped = true;
                    return false;
                }
                true
            })
            .map(str::to_owned)
            .collect()
        }
        (None, None) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::push_exclusions;

    #[test]
    fn new_branches_exclude_every_other_tip_once() {
        let none = None::<std::iter::Empty<&str>>;
        assert_eq!(push_exclusions(Some("b"), Some(["x"].into_iter()), "a"), ["b"], "an update walks from its previous tip");
        assert_eq!(push_exclusions(None, Some(["x", "a", "y", "a"].into_iter()), "a"), ["x", "y", "a"], "one occurrence of the new tip");
        assert_eq!(push_exclusions(None, none, "a"), Vec::<String>::new());
    }
}
