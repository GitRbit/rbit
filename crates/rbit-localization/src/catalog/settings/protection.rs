//! Branch protection settings: the rule list and the rule form.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed labels of the branch protection pages.
    ProtectionMessage, "settings.protection" {
        TickToConfirm = "tick-to-confirm" => "Tick the box to confirm.",
        Settings = "settings" => "Settings",
        Title = "title" => "Branch protection",
        IntroLead = "list.intro-lead" => "Rules apply to every push to",
        IntroTail = "list.intro-tail" => ", over HTTP and SSH, administrators included unless a rule lets them bypass it.",
        AddRule = "add-rule" => "Add rule",
        Deleted = "list.deleted" => "Rule deleted.",
        Empty = "list.empty-title" => "No protected branches",
        EmptyText = "list.empty-text" => "Add a rule to keep a branch such as main from being force-pushed, deleted or changed without a pull request.",
        NoMatchYet = "form.no-match" => "Matches no branch yet.",
        MatchesLabel = "form.matches" => "Matches:",
        NewRule = "form.new-title" => "New branch protection rule",
        NewRuleIntro = "form.new-intro" => "The rule applies to every branch its pattern matches, now and later.",
        Saved = "form.saved" => "Rule saved.",
        Rule = "form.rule" => "Rule",
        Pattern = "form.pattern" => "Branch name pattern",
        PatternPlaceholder = "form.pattern-placeholder" => "main",
        PatternLead = "form.pattern-lead" => "A branch name, or a pattern:",
        PatternOne = "form.pattern-one" => "matches within one part of the name,",
        PatternAcross = "form.pattern-across" => "across parts (",
        PatternChar = "form.pattern-char" => "), ",
        PatternTail = "form.pattern-tail" => "one character. Case-sensitive.",
        Protect = "form.protect" => "Protect matching branches",
        BlockForcePush = "form.block-force-push" => "Block force pushes",
        BlockForcePushText = "form.block-force-push-text" => "Pushes may only add commits; history cannot be rewritten.",
        BlockDeletion = "form.block-deletion" => "Block deletion",
        BlockDeletionText = "form.block-deletion-text" => "Matching branches cannot be deleted.",
        RequirePullRequest = "form.require-pull-request" => "Require a pull request",
        RequirePullRequestText = "form.require-pull-request-text" => "Changes arrive only by merging a pull request; direct pushes to an existing branch are refused.",
        RequiredApprovals = "form.required-approvals" => "Required approvals",
        DismissStale = "form.dismiss-stale" => "Dismiss approvals when new commits are pushed",
        BlockOnChanges = "form.block-on-changes" => "Block merging while changes are requested",
        RestrictPushers = "form.restrict-pushers" => "Restrict who can push",
        RestrictPeople = "form.restrict-people" => "Only the people below may push or merge. Deploy keys never may.",
        RestrictPeopleTeams = "form.restrict-people-teams" => "Only the people and teams below may push or merge. Deploy keys never may.",
        People = "form.people" => "People",
        PeoplePlaceholder = "form.people-placeholder" => "alice bob",
        PeopleHint = "form.people-hint" => "Usernames separated by spaces or commas. Each needs write access.",
        Teams = "form.teams" => "Teams",
        NoTeams = "form.no-teams" => "This organization has no teams yet.",
        RequireChecks = "form.require-checks" => "Require status checks to pass",
        RequireChecksText = "form.require-checks-text" => "Pull requests merge only when the latest status of every check below is a success on their head commit.",
        RequiredChecks = "form.required-checks" => "Required checks",
        ChecksPlaceholder = "form.checks-placeholder" => "ci/build",
        AdminBypass = "form.admin-bypass" => "Allow administrators to bypass",
        AdminBypassText = "form.admin-bypass-text" => "Repository administrators are not held to this rule. Every bypass is recorded in the audit log.",
        SaveRule = "form.save" => "Save rule",
        DangerZone = "form.danger-zone" => "Danger zone",
        DeleteTitle = "form.delete-title" => "Delete this rule",
        DeleteLead = "form.delete-lead" => "Branches matching",
        DeleteTail = "form.delete-tail" => "lose its protection immediately.",
        DeleteConfirm = "form.delete-confirm" => "I understand that this rule will be deleted",
        DeleteRule = "form.delete" => "Delete rule",
        NoMatches = "list.no-matches" => "Matches no branch yet",
    }
}

messages! {
    /// Branch protection text with arguments.
    ProtectionText<'a>, "settings.protection" {
        MaxRules(max: u64) = "list.max-rules" => ["This repository has the maximum of ", (max), " rules. Delete one to add another."],
        ChecksHint(max: u64) = "form.checks-hint" => [
            "One context per line, exactly as your CI reports it (case-sensitive), at most ",
            (max),
            "."
        ],
        Matches(names: &'a str) = "list.matches" => ["Matches ", {names}],
        MatchesMore(names: &'a str, more: u64) = "list.matches-more" => ["Matches ", {names}, " and ", (more), " more"],
        ListTitle(repository: &'a str) = "list.page-title" => ["Branch protection · ", {repository}],
        NewTitle(repository: &'a str) = "form.new-page-title" => ["New branch protection rule · ", {repository}],
        EditTitle(pattern: &'a str, repository: &'a str) = "form.edit-page-title" => [{pattern}, " · Branch protection · ", {repository}],
    }
}

/// A summary badge of a rule on the list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleBadge {
    NoForcePushes,
    NoDeletion,
    PullRequest,
    /// A pull request with this many approvals (at least one).
    Approvals(u64),
    StatusChecks,
    Restricted,
    AdminsBypass,
}

localizes!(RuleBadge);

impl MessageFamily for RuleBadge {
    fn id(self) -> &'static str {
        match self {
            Self::NoForcePushes => "settings.protection.badge.no-force-pushes",
            Self::NoDeletion => "settings.protection.badge.no-deletion",
            Self::PullRequest => "settings.protection.badge.pull-request",
            Self::Approvals(_) => "settings.protection.badge.approvals",
            Self::StatusChecks => "settings.protection.badge.status-checks",
            Self::Restricted => "settings.protection.badge.restricted",
            Self::AdminsBypass => "settings.protection.badge.admins-bypass",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::NoForcePushes => text.literal("No force pushes"),
            Self::NoDeletion => text.literal("No deletion"),
            Self::PullRequest => text.literal("Pull request"),
            Self::Approvals(1) => text.literal("1 approval"),
            Self::Approvals(count) => {
                text.number(count);
                text.literal(" approvals");
            }
            Self::StatusChecks => text.literal("Status checks"),
            Self::Restricted => text.literal("Restricted"),
            Self::AdminsBypass => text.literal("Admins bypass"),
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
