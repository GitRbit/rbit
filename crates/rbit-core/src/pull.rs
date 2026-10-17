//! Pull requests: the branch data attached to an issue row, mergeability, reviews and
//! merge requirements.
//!
//! Milestone 2 workstream P (pull requests) owns this file.
//!
//! Everything here is pure: the stored shapes ([`PullRequest`], [`Review`],
//! [`ReviewComment`]), the approval tally ([`tally`]), the merge box's requirement list
//! ([`evaluate`]), default merge messages and the noreply identities Rbit writes into
//! the commits it makes. Services decide *when*; this module decides *what*.

use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    AccountId, IssueId, IssueNumber, RepoId, Timestamp,
    issue::{MergeStrategy, ReviewId, ReviewState},
    protection::MergeRules,
};

/// Conflicting paths stored (and shown) at most per pull request.
pub const MAX_CONFLICT_PATHS: usize = 100;
/// Reviewers that can be requested on one pull request.
pub const MAX_REQUESTED_REVIEWERS: usize = 15;

/// Database id of a review comment on a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewCommentId(pub i64);

/// Generates `as_str`, `parse` and `Display` for a field-less enum stored as text.
macro_rules! stored_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

stored_enum!(
    /// The result of the last trial merge of head into base.
    Mergeability {
        /// Not computed yet, or the head or base moved since.
        Unknown => "unknown",
        Clean => "clean",
        Conflict => "conflict",
        /// The trial merge failed for another reason (logged); retried after the next push.
        Error => "error",
    }
);

stored_enum!(
    /// Required status checks. Nothing produces anything but `NotConfigured` in this wave;
    /// the other states exist for a future commit status API.
    ChecksStatus {
        NotConfigured => "not_configured",
        Pending => "pending",
        Passing => "passing",
        Failing => "failing",
    }
);

stored_enum!(
    /// The side of a unified diff a review comment is anchored to: `Left` is the old file
    /// (a removed line), `Right` the new one (an added or context line).
    ReviewSide {
        Left => "left",
        Right => "right",
    }
);

/// A pull request's own row, keyed by its issue row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub issue_id: IssueId,
    /// The base repository (the issue row's repository).
    pub repo_id: RepoId,
    /// `None` once the head repository is deleted (`refs/pull/{n}/head` keeps the commits).
    pub head_repo_id: Option<RepoId>,
    pub head_branch: String,
    pub base_branch: String,
    /// The head commit as of the last push Rbit saw (what `refs/pull/{n}/head` points at).
    pub head_sha: String,
    pub is_draft: bool,
    pub mergeable: Mergeability,
    /// The base and head ids the stored mergeability was computed on.
    pub mergeable_base_sha: Option<String>,
    pub mergeable_head_sha: Option<String>,
    /// For [`Mergeability::Conflict`]: at most [`MAX_CONFLICT_PATHS`] paths.
    pub conflict_paths: Vec<String>,
    pub merged_at: Option<Timestamp>,
    pub merged_by: Option<AccountId>,
    /// `None` for a pull request merged outside Rbit.
    pub merge_strategy: Option<MergeStrategy>,
    pub merge_commit_sha: Option<String>,
    pub merged_base_sha: Option<String>,
    pub merged_head_sha: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl PullRequest {
    pub fn is_merged(&self) -> bool {
        self.merged_at.is_some()
    }

    /// The head lives in another repository than the base.
    pub fn is_cross_repository(&self) -> bool {
        self.head_repo_id != Some(self.repo_id)
    }

    /// GitHub's `mergeable_state` for an open pull request: `draft`, `dirty` (conflicts),
    /// `blocked` (requirements unmet), `clean` or `unknown`.
    pub fn mergeable_state(&self, blocked: bool) -> &'static str {
        match (self.is_draft, self.mergeable) {
            (true, _) => "draft",
            (false, Mergeability::Conflict) => "dirty",
            (false, Mergeability::Clean) if blocked => "blocked",
            (false, Mergeability::Clean) => "clean",
            (false, Mergeability::Unknown | Mergeability::Error) => "unknown",
        }
    }
}

/// A review of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    pub id: ReviewId,
    pub pull_id: IssueId,
    /// `None` once the reviewer's account is deleted.
    pub reviewer: Option<AccountId>,
    pub state: ReviewState,
    pub body: String,
    /// The head commit the review was made on.
    pub commit_sha: String,
    pub created_at: Timestamp,
    /// `None` while pending.
    pub submitted_at: Option<Timestamp>,
    pub dismissed_by: Option<AccountId>,
    pub dismissed_at: Option<Timestamp>,
    pub dismissal_message: Option<String>,
}

/// A comment on one diff line, part of a review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: ReviewCommentId,
    pub review_id: ReviewId,
    pub pull_id: IssueId,
    pub author: Option<AccountId>,
    pub path: String,
    pub side: ReviewSide,
    /// 1-based line number on `side`.
    pub line: u32,
    /// The head commit the comment was made on.
    pub commit_sha: String,
    /// Up to four lines of the diff ending at the commented line, for display.
    pub diff_hunk: String,
    pub in_reply_to: Option<ReviewCommentId>,
    pub body: String,
    /// The file changed since the comment's commit; shown only in the conversation.
    pub outdated: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

// ------------------------------------------------------------------ approvals

/// Where the reviews of one pull request stand, for the merge requirements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewTally {
    /// Writers whose latest decisive review approves (and counts: see [`tally`]).
    pub approvals: Vec<AccountId>,
    /// Writers whose latest decisive review requests changes.
    pub changes_requested: Vec<AccountId>,
}

/// Counts approvals and change requests.
///
/// For every reviewer, only their **latest decisive** review counts: the newest submitted
/// review that is `approved` or `changes_requested` (pending, comment-only and dismissed
/// reviews neither approve nor withdraw an approval, as on GitHub). Only reviewers for
/// whom `is_writer` holds count, never the pull request's `author`. With
/// `dismiss_stale_approvals`, an approval counts only if it was made on `head_sha`;
/// change requests count regardless of the commit.
pub fn tally(
    reviews: &[Review],
    author: Option<AccountId>,
    head_sha: &str,
    dismiss_stale_approvals: bool,
    is_writer: impl Fn(AccountId) -> bool,
) -> ReviewTally {
    let mut latest: HashMap<AccountId, &Review> = HashMap::new();
    for review in reviews {
        let Some(reviewer) = review.reviewer else { continue };
        if !matches!(review.state, ReviewState::Approved | ReviewState::ChangesRequested) || Some(reviewer) == author {
            continue;
        }
        let key = |r: &Review| (r.submitted_at.unwrap_or(r.created_at), r.id);
        match latest.get(&reviewer) {
            Some(known) if key(known) >= key(review) => {}
            _ => {
                latest.insert(reviewer, review);
            }
        }
    }
    let mut tally = ReviewTally::default();
    for (reviewer, review) in latest {
        if !is_writer(reviewer) {
            continue;
        }
        match review.state {
            ReviewState::Approved if !dismiss_stale_approvals || review.commit_sha == head_sha => tally.approvals.push(reviewer),
            ReviewState::ChangesRequested => tally.changes_requested.push(reviewer),
            _ => {}
        }
    }
    tally.approvals.sort();
    tally.changes_requested.sort();
    tally
}

// ------------------------------------------------------------------ requirements

/// One line of the merge box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    Open,
    NotDraft,
    HeadBranchExists,
    /// The trial merge is clean.
    NoConflicts(Mergeability),
    /// Approvals by writers, counted as in [`tally`].
    Approvals {
        have: usize,
        need: u8,
    },
    /// No writer's latest review requests changes.
    NoChangesRequested {
        count: usize,
    },
    /// The merger may push to the base branch (restricted pushers).
    MergerMayPush,
    Checks(ChecksStatus),
}

impl Requirement {
    /// Requirements a protection bypass may skip: the protection rules themselves.
    pub const fn is_protection(self) -> bool {
        matches!(self, Self::Approvals { .. } | Self::NoChangesRequested { .. } | Self::MergerMayPush)
    }

    /// The sentence shown in the merge box.
    pub fn describe(self, met: bool) -> String {
        match (self, met) {
            (Self::Open, _) => if met { "The pull request is open" } else { "The pull request is closed" }.to_owned(),
            (Self::NotDraft, true) => "Ready for review".to_owned(),
            (Self::NotDraft, false) => "This pull request is still a draft".to_owned(),
            (Self::HeadBranchExists, true) => "The head branch exists".to_owned(),
            (Self::HeadBranchExists, false) => "The head branch was deleted".to_owned(),
            (Self::NoConflicts(state), _) => match state {
                Mergeability::Clean => "No conflicts with the base branch",
                Mergeability::Conflict => "This branch has conflicts that must be resolved",
                Mergeability::Unknown => "Checking for conflicts…",
                Mergeability::Error => "Mergeability could not be checked",
            }
            .to_owned(),
            (Self::Approvals { have, need }, _) => {
                let noun = if need == 1 { "approval" } else { "approvals" };
                format!("{have} of {need} required {noun}")
            }
            (Self::NoChangesRequested { .. }, true) => "No changes requested".to_owned(),
            (Self::NoChangesRequested { count }, false) => {
                format!("{count} {} requested changes", if count == 1 { "reviewer" } else { "reviewers" })
            }
            (Self::MergerMayPush, true) => "You may push to the base branch".to_owned(),
            (Self::MergerMayPush, false) => "You are not allowed to push to the base branch".to_owned(),
            (Self::Checks(_), _) => "No status checks — not supported yet".to_owned(),
        }
    }
}

/// A requirement and whether it is met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequirementCheck {
    pub requirement: Requirement,
    pub met: bool,
}

/// What [`evaluate`] needs to know about the pull request.
#[derive(Debug, Clone, Copy)]
pub struct MergeFacts<'a> {
    pub is_open: bool,
    pub is_draft: bool,
    pub head_branch_exists: bool,
    pub mergeable: Mergeability,
    pub tally: &'a ReviewTally,
    pub checks: ChecksStatus,
}

/// The merge box: every requirement with its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeEvaluation {
    pub checks: Vec<RequirementCheck>,
    /// The merger may merge despite unmet protection requirements (after confirming).
    pub may_bypass: bool,
}

impl MergeEvaluation {
    /// Every requirement is met.
    pub fn can_merge(&self) -> bool {
        self.checks.iter().all(|check| check.met)
    }

    /// Every requirement but protection ones is met, and the merger may bypass those.
    pub fn can_merge_with_bypass(&self) -> bool {
        self.may_bypass && self.checks.iter().all(|check| check.met || check.requirement.is_protection())
    }

    /// Some protection requirement is unmet (GitHub's `blocked`).
    pub fn is_blocked(&self) -> bool {
        self.checks.iter().any(|check| !check.met && check.requirement.is_protection())
    }
}

/// Evaluates the merge requirements of `facts` against `rules`.
pub fn evaluate(facts: &MergeFacts<'_>, rules: &MergeRules) -> MergeEvaluation {
    let check = |requirement, met| RequirementCheck { requirement, met };
    let mut checks = vec![
        check(Requirement::Open, facts.is_open),
        check(Requirement::NotDraft, !facts.is_draft),
        check(Requirement::HeadBranchExists, facts.head_branch_exists),
        check(Requirement::NoConflicts(facts.mergeable), facts.mergeable == Mergeability::Clean),
    ];
    if rules.required_approvals > 0 {
        let have = facts.tally.approvals.len();
        checks.push(check(Requirement::Approvals { have, need: rules.required_approvals }, have >= usize::from(rules.required_approvals)));
    }
    if rules.block_on_changes_requested {
        let count = facts.tally.changes_requested.len();
        checks.push(check(Requirement::NoChangesRequested { count }, count == 0));
    }
    if !rules.merger_may_push {
        checks.push(check(Requirement::MergerMayPush, false));
    }
    // Nothing reports statuses yet: the placeholder is informational and always met.
    checks.push(check(Requirement::Checks(facts.checks), facts.checks != ChecksStatus::Failing && facts.checks != ChecksStatus::Pending));
    MergeEvaluation { checks, may_bypass: rules.merger_may_bypass }
}

// ------------------------------------------------------------------ messages and identities

/// The default title of a merge commit: `Merge pull request #n from owner/branch`.
pub fn default_merge_title(number: IssueNumber, head_label: &str) -> String {
    format!("Merge pull request #{} from {head_label}", number.get())
}

/// The default title of a squash commit: `title (#n)`.
pub fn default_squash_title(title: &str, number: IssueNumber) -> String {
    format!("{title} (#{})", number.get())
}

/// The default message body of a squash commit: the squashed commits' summaries, oldest first.
pub fn default_squash_body<'a>(summaries: impl IntoIterator<Item = &'a str>) -> String {
    summaries.into_iter().map(|summary| format!("* {summary}")).collect::<Vec<_>>().join("\n")
}

/// A full commit message: the title, then (if any) a blank line and the body.
pub fn commit_message(title: &str, body: &str) -> String {
    let (title, body) = (title.trim(), body.trim());
    if body.is_empty() { format!("{title}\n") } else { format!("{title}\n\n{body}\n") }
}

/// The domain of the addresses in commits Rbit makes: the configured one, else
/// `noreply.<host of public_url>`.
pub fn noreply_domain(configured: &str, public_url: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    let rest = public_url.split_once("://").map_or(public_url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(inner, _)| inner.trim_start_matches('['))
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let host = if host.is_empty() { "localhost" } else { host };
    format!("noreply.{host}")
}

/// `username@domain`: Rbit never writes a real email address into a commit.
pub fn noreply_email(username: &str, domain: &str) -> String {
    format!("{username}@{domain}")
}

/// Who appears in a commit Rbit makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitIdentity {
    pub name: String,
    pub email: String,
}

impl CommitIdentity {
    /// The display name (or the username) with the noreply address.
    pub fn noreply(username: &str, display_name: Option<&str>, domain: &str) -> Self {
        let name = display_name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(username);
        // Git refuses `<`, `>` and newlines in identities.
        let name: String = name.chars().filter(|c| !matches!(c, '<' | '>' | '\n' | '\r' | '\0')).collect();
        Self { name: if name.trim().is_empty() { username.to_owned() } else { name }, email: noreply_email(username, domain) }
    }
}

// ------------------------------------------------------------------ compare specs

/// The head side of a compare URL or a new pull request: `branch`, `owner:branch` or
/// `owner/name:branch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadSpec {
    /// `None`: the base repository itself.
    pub owner: Option<String>,
    /// `None`: the owner's repository in the fork network.
    pub repo: Option<String>,
    pub branch: String,
}

impl HeadSpec {
    /// Parses a head spec. Branch names cannot contain `:` (Git forbids it), so the first
    /// `:` separates the repository part.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        let (repository, branch) = match spec.split_once(':') {
            Some((repository, branch)) => (Some(repository), branch),
            None => (None, spec),
        };
        if branch.is_empty() {
            return None;
        }
        let (owner, repo) = match repository {
            None => (None, None),
            Some(repository) => match repository.split_once('/') {
                Some((owner, repo)) if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') => {
                    (Some(owner.to_owned()), Some(repo.to_owned()))
                }
                Some(_) => return None,
                None if repository.is_empty() => return None,
                None => (Some(repository.to_owned()), None),
            },
        };
        Some(Self { owner, repo, branch: branch.to_owned() })
    }
}

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
        }
    }

    #[test]
    fn requirements_follow_the_rules() {
        let none = ReviewTally::default();
        let open = evaluate(&facts(&none), &MergeRules::unprotected());
        assert!(open.can_merge() && !open.is_blocked());
        assert_eq!(open.checks.len(), 5);

        let rules = MergeRules {
            patterns: vec!["main".to_owned()],
            require_pull_request: true,
            required_approvals: 2,
            dismiss_stale_approvals: false,
            block_on_changes_requested: true,
            merger_may_push: false,
            merger_may_bypass: false,
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
