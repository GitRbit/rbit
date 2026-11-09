//! Pull requests: the branch data attached to an issue row, mergeability, reviews and
//! merge requirements.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Everything here is pure: the stored shapes ([`PullRequest`], [`Review`],
//! [`ReviewComment`]), the approval tally ([`tally`]), the merge box's requirement list
//! ([`evaluate`]), default merge messages and the noreply identities rbit writes into
//! the commits it makes. Services decide *when*; this module decides *what*.

pub use rbit_collaboration::pulls::{
    ChecksStatus, CommitIdentity, HeadSpec, MAX_CONFLICT_PATHS, MAX_REQUESTED_REVIEWERS, MergeEvaluation, MergeFacts, MergeStrategy,
    Mergeability, PullRequest, Requirement, RequirementCheck, Review, ReviewComment, ReviewCommentId, ReviewId, ReviewSide, ReviewState,
    ReviewTally, commit_message, default_merge_title, default_squash_body, default_squash_title, evaluate, noreply_domain, noreply_email,
    tally,
};

#[cfg(test)]
use crate::{AccountId, IssueId, IssueNumber, RepoId, Timestamp, protection::MergeRules, status::RequiredChecks};

#[cfg(test)]
mod tests {
    use super::*;

    fn review(id: i64, reviewer: i64, state: ReviewState, commit: &str, at: i64) -> Review {
        Review {
            id: ReviewId(id),
            pull_id: IssueId(1),
            reviewer: Some(AccountId(reviewer)),
            state,
            body: String::new(),
            commit_sha: commit.to_owned(),
            created_at: Timestamp(at),
            submitted_at: (state != ReviewState::Pending).then_some(Timestamp(at)),
            dismissed_by: None,
            dismissed_at: None,
            dismissal_message: None,
        }
    }

    fn writers(id: AccountId) -> bool {
        id.0 < 100
    }

    #[test]
    fn the_latest_decisive_review_per_reviewer_counts() {
        use ReviewState::{Approved, ChangesRequested, Commented, Dismissed, Pending};
        let reviews = [
            // 1: changes requested, then approved: approves.
            review(1, 1, ChangesRequested, "a", 10),
            review(2, 1, Approved, "a", 20),
            // 2: approved, then changes requested: requests changes.
            review(3, 2, Approved, "a", 10),
            review(4, 2, ChangesRequested, "a", 20),
            // 3: approved, then only commented: still approves.
            review(5, 3, Approved, "b", 10),
            review(6, 3, Commented, "b", 30),
            // 4: approval dismissed: nothing.
            review(7, 4, Dismissed, "b", 10),
            // 5: pending approval-to-be: nothing.
            review(8, 5, Pending, "b", 10),
            // 200: not a writer.
            review(9, 200, Approved, "b", 10),
            // 9: the author.
            review(10, 9, Approved, "b", 10),
        ];
        let result = tally(&reviews, Some(AccountId(9)), "b", false, writers);
        assert_eq!(result.approvals, [AccountId(1), AccountId(3)]);
        assert_eq!(result.changes_requested, [AccountId(2)]);

        // Stale approvals (made on "a") drop out; change requests stay.
        let strict = tally(&reviews, Some(AccountId(9)), "b", true, writers);
        assert_eq!(strict.approvals, [AccountId(3)]);
        assert_eq!(strict.changes_requested, [AccountId(2)]);

        // Same second: the higher id is the later review.
        let same_time = [review(1, 1, Approved, "a", 10), review(2, 1, ChangesRequested, "a", 10)];
        assert_eq!(tally(&same_time, None, "a", false, writers).changes_requested, [AccountId(1)]);
    }

    fn facts(tally: &ReviewTally) -> MergeFacts<'_> {
        MergeFacts {
            is_open: true,
            is_draft: false,
            head_branch_exists: true,
            mergeable: Mergeability::Clean,
            tally,
            checks: ChecksStatus::NotConfigured,
            required_checks: RequiredChecks::default(),
        }
    }

    #[test]
    fn requirements_follow_the_rules() {
        let none = ReviewTally::default();
        let open = evaluate(&facts(&none), &MergeRules::unprotected());
        assert!(open.can_merge() && !open.is_blocked());
        assert_eq!(open.checks.len(), 4, "no status line without statuses");

        let rules = MergeRules {
            patterns: vec!["main".to_owned()],
            require_pull_request: true,
            required_approvals: 2,
            dismiss_stale_approvals: false,
            block_on_changes_requested: true,
            merger_may_push: false,
            merger_may_bypass: false,
            require_status_checks: false,
            required_contexts: Vec::new(),
        };
        let one = ReviewTally { approvals: vec![AccountId(1)], changes_requested: vec![AccountId(2)] };
        let blocked = evaluate(&facts(&one), &rules);
        assert!(!blocked.can_merge() && blocked.is_blocked() && !blocked.can_merge_with_bypass());
        let unmet: Vec<_> = blocked.checks.iter().filter(|c| !c.met).map(|c| c.requirement).collect();
        assert_eq!(
            unmet,
            [Requirement::Approvals { have: 1, need: 2 }, Requirement::NoChangesRequested { count: 1 }, Requirement::MergerMayPush]
        );

        let bypass = evaluate(&facts(&one), &MergeRules { merger_may_bypass: true, ..rules.clone() });
        assert!(!bypass.can_merge() && bypass.can_merge_with_bypass());

        // A bypass never skips structural requirements.
        let conflicted = MergeFacts { mergeable: Mergeability::Conflict, ..facts(&one) };
        assert!(!evaluate(&conflicted, &MergeRules { merger_may_bypass: true, ..rules.clone() }).can_merge_with_bypass());
        for broken in [
            MergeFacts { is_open: false, ..facts(&none) },
            MergeFacts { is_draft: true, ..facts(&none) },
            MergeFacts { head_branch_exists: false, ..facts(&none) },
            MergeFacts { mergeable: Mergeability::Unknown, ..facts(&none) },
        ] {
            assert!(!evaluate(&broken, &MergeRules::unprotected()).can_merge());
        }

        let two = ReviewTally { approvals: vec![AccountId(1), AccountId(3)], changes_requested: Vec::new() };
        assert!(evaluate(&facts(&two), &MergeRules { merger_may_push: true, ..rules }).can_merge());
    }

    #[test]
    fn required_checks_block_and_other_statuses_inform() {
        let none = ReviewTally::default();
        // Unrequired statuses never block, whatever their state.
        for checks in [ChecksStatus::Pending, ChecksStatus::Failing, ChecksStatus::Passing] {
            let evaluation = evaluate(&MergeFacts { checks, ..facts(&none) }, &MergeRules::unprotected());
            assert!(evaluation.can_merge(), "{checks:?}");
            assert_eq!(evaluation.checks.last().map(|c| c.requirement), Some(Requirement::Checks(checks)));
        }

        let rules = MergeRules {
            patterns: vec!["main".to_owned()],
            require_status_checks: true,
            required_contexts: vec![crate::status::StatusContext::parse("ci").unwrap()],
            ..MergeRules::unprotected()
        };
        let waiting = RequiredChecks { required: 2, passing: 1, failing: 0 };
        let failing = RequiredChecks { required: 2, passing: 0, failing: 1 };
        let passing = RequiredChecks { required: 2, passing: 2, failing: 0 };
        for (required_checks, can_merge, sentence) in [
            (waiting, false, "Waiting for 1 of 2 required checks"),
            (failing, false, "1 of 2 required checks failed"),
            (passing, true, "2 required checks passed"),
        ] {
            let evaluation = evaluate(&MergeFacts { required_checks, checks: ChecksStatus::Pending, ..facts(&none) }, &rules);
            assert_eq!(evaluation.can_merge(), can_merge, "{required_checks:?}");
            assert_eq!(evaluation.is_blocked(), !can_merge, "required checks are a protection requirement");
            let line = evaluation.checks.iter().find(|c| matches!(c.requirement, Requirement::RequiredChecks(_))).unwrap();
            assert_eq!(line.requirement.describe(line.met), sentence);
        }
        let one = RequiredChecks { required: 1, passing: 1, failing: 0 };
        assert_eq!(Requirement::RequiredChecks(one).describe(true), "1 required check passed");
        // An administrator allowed to bypass may merge while checks fail; structure still counts.
        let bypass = MergeRules { merger_may_bypass: true, ..rules.clone() };
        assert!(evaluate(&MergeFacts { required_checks: failing, ..facts(&none) }, &bypass).can_merge_with_bypass());
        assert_eq!(Requirement::Checks(ChecksStatus::Failing).describe(true), "Some checks were not successful");
    }

    #[test]
    fn mergeable_state_is_githubs() {
        let mut pull = PullRequest {
            issue_id: IssueId(1),
            repo_id: RepoId(1),
            head_repo_id: Some(RepoId(1)),
            head_branch: "feature".to_owned(),
            base_branch: "main".to_owned(),
            head_sha: "a".repeat(40),
            is_draft: false,
            mergeable: Mergeability::Clean,
            mergeable_base_sha: None,
            mergeable_head_sha: None,
            conflict_paths: Vec::new(),
            merged_at: None,
            merged_by: None,
            merge_strategy: None,
            merge_commit_sha: None,
            merged_base_sha: None,
            merged_head_sha: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        };
        assert!(!pull.is_cross_repository());
        assert_eq!((pull.mergeable_state(false), pull.mergeable_state(true)), ("clean", "blocked"));
        pull.mergeable = Mergeability::Conflict;
        assert_eq!(pull.mergeable_state(false), "dirty");
        pull.mergeable = Mergeability::Unknown;
        assert_eq!(pull.mergeable_state(false), "unknown");
        pull.is_draft = true;
        assert_eq!(pull.mergeable_state(false), "draft");
        pull.head_repo_id = None;
        assert!(pull.is_cross_repository());
    }

    #[test]
    fn messages_and_identities() {
        let n = IssueNumber::new(7).unwrap();
        assert_eq!(default_merge_title(n, "alice/feature"), "Merge pull request #7 from alice/feature");
        assert_eq!(default_squash_title("Fix it", n), "Fix it (#7)");
        assert_eq!(default_squash_body(["one", "two"]), "* one\n* two");
        assert_eq!(commit_message(" Title ", ""), "Title\n");
        assert_eq!(commit_message("Title", "Body\n"), "Title\n\nBody\n");

        assert_eq!(noreply_domain("", "https://git.example.com:8443/base"), "noreply.git.example.com");
        assert_eq!(noreply_domain("", "http://user@[::1]:3000"), "noreply.::1");
        assert_eq!(noreply_domain("", "localhost"), "noreply.localhost");
        assert_eq!(noreply_domain(" mail.example ", "http://x"), "mail.example");
        let identity = CommitIdentity::noreply("alice", Some("Alice <A>"), "noreply.example");
        assert_eq!((identity.name.as_str(), identity.email.as_str()), ("Alice A", "alice@noreply.example"));
        assert_eq!(CommitIdentity::noreply("bob", Some("  "), "d").name, "bob");
    }

    #[test]
    fn head_specs() {
        let spec = |owner: Option<&str>, repo: Option<&str>, branch: &str| HeadSpec {
            owner: owner.map(str::to_owned),
            repo: repo.map(str::to_owned),
            branch: branch.to_owned(),
        };
        assert_eq!(HeadSpec::parse("feature/x"), Some(spec(None, None, "feature/x")));
        assert_eq!(HeadSpec::parse("bob:feature"), Some(spec(Some("bob"), None, "feature")));
        assert_eq!(HeadSpec::parse("bob/fork:feature"), Some(spec(Some("bob"), Some("fork"), "feature")));
        for bad in ["", ":x", "bob:", "/fork:x", "bob/:x", "a/b/c:x"] {
            assert_eq!(HeadSpec::parse(bad), None, "{bad:?}");
        }
        assert_eq!(Mergeability::parse("conflict"), Some(Mergeability::Conflict));
        assert_eq!(ReviewSide::parse("left"), Some(ReviewSide::Left));
        assert_eq!(ChecksStatus::NotConfigured.as_str(), "not_configured");
    }
}
