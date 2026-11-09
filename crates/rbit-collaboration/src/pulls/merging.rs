//! Merge decisions around [`evaluate`](super::evaluate): whether a stored mergeability
//! still holds, when it must be recomputed, whether a merge may go ahead (or bypass
//! protection), and the message of the commit it makes.

use super::{MergeEvaluation, MergeStrategy, Mergeability};

/// The stored trial-merge result of an open pull request holds only for the base tip and
/// head it was computed on; otherwise (or without a base tip) it is `Unknown`. Closed
/// pull requests keep what was stored.
pub fn current_mergeability(
    stored: Mergeability,
    computed_on: (Option<&str>, Option<&str>),
    base_tip: Option<&str>,
    head_sha: &str,
    is_open: bool,
) -> Mergeability {
    let stale = base_tip.is_none() || computed_on.0 != base_tip || computed_on.1 != Some(head_sha);
    if stale && is_open { Mergeability::Unknown } else { stored }
}

/// Whether the mergeability worker should compute `mergeable` now: unknown, on an open
/// pull request that is not a draft.
pub fn needs_trial_merge(mergeable: Mergeability, is_open: bool, is_draft: bool) -> bool {
    mergeable == Mergeability::Unknown && is_open && !is_draft
}

/// Who sees the merge button, and whose protection rules are evaluated: a signed-in
/// viewer with write access (the button also needs a repository that is not archived).
pub const fn merges_as_writer(signed_in: bool, can_write: bool) -> bool {
    signed_in && can_write
}

/// Whether the expected value a merge request carries (head commit or base branch)
/// still matches; an absent or empty expectation always does.
pub fn still_expected(expected: Option<&str>, actual: &str) -> bool {
    expected.filter(|expected| !expected.is_empty()).is_none_or(|expected| expected == actual)
}

/// Whether a merge may go ahead: `Ok(false)` when every requirement is met, `Ok(true)`
/// when the merger asked to `bypass` and may bypass the unmet protection requirements
/// (the caller audits it), otherwise the refusal naming the first unmet requirement.
pub fn merge_gate(evaluation: &MergeEvaluation, bypass: bool) -> Result<bool, String> {
    if evaluation.can_merge() {
        return Ok(false);
    }
    if bypass && evaluation.can_merge_with_bypass() {
        return Ok(true);
    }
    let unmet = evaluation.checks.iter().find(|check| !check.met).map_or_else(String::new, |check| check.requirement.describe(false));
    Err(format!("this pull request cannot be merged: {unmet}"))
}

/// The default title and body of the commit a `strategy` makes.
#[derive(Debug, Clone, Copy)]
pub struct MergeDefaults<'a> {
    pub merge: (&'a str, &'a str),
    pub squash: (&'a str, &'a str),
}

/// The commit message of a merge: the typed title and message, or the strategy's
/// defaults when no title is typed (the default body only when no message is typed
/// either).
pub fn merge_message(strategy: MergeStrategy, title: &str, message: &str, defaults: MergeDefaults<'_>) -> String {
    let (default_title, default_body) = match strategy {
        MergeStrategy::Merge => defaults.merge,
        MergeStrategy::Squash | MergeStrategy::Rebase => defaults.squash,
    };
    let typed_title = title.trim();
    let title = if typed_title.is_empty() { default_title } else { typed_title };
    let body = if typed_title.is_empty() && message.trim().is_empty() { default_body } else { message };
    super::commit_message(title, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulls::{Requirement, RequirementCheck};

    #[test]
    fn stored_mergeability_holds_only_where_it_was_computed() {
        let on = (Some("b"), Some("h"));
        assert_eq!(current_mergeability(Mergeability::Clean, on, Some("b"), "h", true), Mergeability::Clean);
        assert_eq!(current_mergeability(Mergeability::Clean, on, Some("b2"), "h", true), Mergeability::Unknown);
        assert_eq!(current_mergeability(Mergeability::Clean, on, Some("b"), "h2", true), Mergeability::Unknown);
        assert_eq!(current_mergeability(Mergeability::Conflict, on, None, "h", true), Mergeability::Unknown);
        assert_eq!(current_mergeability(Mergeability::Conflict, on, None, "h", false), Mergeability::Conflict, "closed keeps");
        assert!(needs_trial_merge(Mergeability::Unknown, true, false));
        assert!(!needs_trial_merge(Mergeability::Unknown, true, true) && !needs_trial_merge(Mergeability::Clean, true, false));
        assert!(still_expected(None, "x") && still_expected(Some(""), "x") && still_expected(Some("x"), "x"));
        assert!(!still_expected(Some("y"), "x"));
    }

    #[test]
    fn gates_and_messages() {
        let check = |requirement, met| RequirementCheck { requirement, met };
        let blocked = MergeEvaluation {
            checks: vec![check(Requirement::Open, true), check(Requirement::Approvals { have: 0, need: 1 }, false)],
            may_bypass: true,
        };
        assert_eq!(merge_gate(&blocked, true), Ok(true), "an allowed bypass");
        assert_eq!(merge_gate(&blocked, false), Err("this pull request cannot be merged: 0 of 1 required approval".to_owned()));
        let closed = MergeEvaluation { checks: vec![check(Requirement::Open, false)], may_bypass: true };
        assert_eq!(merge_gate(&closed, true), Err("this pull request cannot be merged: The pull request is closed".to_owned()));
        let defaults = MergeDefaults { merge: ("Merge #1", "Title"), squash: ("Title (#1)", "* a") };
        assert_eq!(merge_message(MergeStrategy::Merge, " ", "", defaults), super::super::commit_message("Merge #1", "Title"));
        assert_eq!(merge_message(MergeStrategy::Squash, "", "", defaults), super::super::commit_message("Title (#1)", "* a"));
        assert_eq!(merge_message(MergeStrategy::Rebase, "", "own", defaults), super::super::commit_message("Title (#1)", "own"));
        assert_eq!(merge_message(MergeStrategy::Merge, " Mine ", "", defaults), super::super::commit_message("Mine", ""));
    }
}
