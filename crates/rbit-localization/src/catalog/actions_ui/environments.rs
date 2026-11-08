//! A repository's deployment environments: the list, one environment's protection rules
//! and the summary of its protection in the list.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsEnvironmentMessage<'a> {
    Title,
    /// `key` is the workflow key that names an environment.
    Intro {
        key: &'a str,
    },
    NoEnvironments,
    NoEnvironmentsHint {
        key: &'a str,
    },
    NewEnvironment,
    Name,
    /// An example environment name.
    NamePlaceholder,
    AddEnvironment,
    Reviewers {
        count: u64,
    },
    ProtectedBranches,
    /// `patterns` is the list of allowed branch and tag patterns.
    Branches {
        patterns: &'a str,
    },
    NoProtection,
    ProtectionRules,
    RequiredReviewers,
    ReviewersPlaceholder,
    ReviewersHint,
    PreventSelfReview,
    PreventSelfReviewHint,
    WaitTimer,
    WaitTimerHint,
    DeploymentBranches,
    AnyBranch,
    ProtectedOnly,
    ProtectedOnlyHint,
    SelectedBranches,
    PatternsLabel,
    /// An example branch pattern.
    PatternsPlaceholder,
    /// Each argument is a pattern example shown as code.
    PatternsHint {
        branch: &'a str,
        tags: &'a str,
        tag: &'a str,
        exclude: &'a str,
    },
    SaveRules,
    DeleteTitle,
    DeleteHint,
    DeleteEnvironment,
}

impl MessageFamily for ActionsEnvironmentMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Title => "actions.environments.title",
            Self::Intro { .. } => "actions.environments.intro",
            Self::NoEnvironments => "actions.environments.empty",
            Self::NoEnvironmentsHint { .. } => "actions.environments.empty-hint",
            Self::NewEnvironment => "actions.environments.new",
            Self::Name => "actions.environments.name",
            Self::NamePlaceholder => "actions.environments.name-placeholder",
            Self::AddEnvironment => "actions.environments.add",
            Self::Reviewers { .. } => "actions.environments.summary.reviewers",
            Self::ProtectedBranches => "actions.environments.summary.protected",
            Self::Branches { .. } => "actions.environments.summary.branches",
            Self::NoProtection => "actions.environments.summary.none",
            Self::ProtectionRules => "actions.environment.rules",
            Self::RequiredReviewers => "actions.environment.reviewers",
            Self::ReviewersPlaceholder => "actions.environment.reviewers-placeholder",
            Self::ReviewersHint => "actions.environment.reviewers-hint",
            Self::PreventSelfReview => "actions.environment.prevent-self-review",
            Self::PreventSelfReviewHint => "actions.environment.prevent-self-review-hint",
            Self::WaitTimer => "actions.environment.wait-timer",
            Self::WaitTimerHint => "actions.environment.wait-timer-hint",
            Self::DeploymentBranches => "actions.environment.branches",
            Self::AnyBranch => "actions.environment.branches.all",
            Self::ProtectedOnly => "actions.environment.branches.protected",
            Self::ProtectedOnlyHint => "actions.environment.branches.protected-hint",
            Self::SelectedBranches => "actions.environment.branches.selected",
            Self::PatternsLabel => "actions.environment.branches.patterns",
            Self::PatternsPlaceholder => "actions.environment.branches.patterns-placeholder",
            Self::PatternsHint { .. } => "actions.environment.branches.patterns-hint",
            Self::SaveRules => "actions.environment.save",
            Self::DeleteTitle => "actions.environment.delete.title",
            Self::DeleteHint => "actions.environment.delete.hint",
            Self::DeleteEnvironment => "actions.environment.delete",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::Intro { key } => {
                text.literal("A job that names an environment with ");
                text.argument(key);
                return text.literal(
                    " gets its secrets and variables, waits for one of its reviewers when it has any, and runs only from \
                     the branches it allows. An environment appears here the first time a workflow names it.",
                );
            }
            Self::NoEnvironmentsHint { key } => {
                text.literal("Add one below, or run a workflow whose job names one with ");
                text.argument(key);
                return text.literal(".");
            }
            Self::Reviewers { count } => {
                text.number(count);
                return text.literal(if count == 1 { " reviewer" } else { " reviewers" });
            }
            Self::Branches { patterns } => {
                text.literal("branches ");
                return text.argument(patterns);
            }
            Self::PatternsHint { branch, tags, tag, exclude } => {
                text.literal("One pattern per line, as in workflow filters. Branches by name (");
                text.argument(branch);
                text.literal("), tags with ");
                text.argument(tags);
                text.literal(" (");
                text.argument(tag);
                text.literal("); ");
                text.argument(exclude);
                return text.literal(" excludes.");
            }
            Self::Title => "Environments",
            Self::NoEnvironments => "No environments",
            Self::NewEnvironment => "New environment",
            Self::Name => "Name",
            Self::NamePlaceholder => "production",
            Self::AddEnvironment => "Add environment",
            Self::ProtectedBranches => "protected branches",
            Self::NoProtection => "No protection",
            Self::ProtectionRules => "Protection rules",
            Self::RequiredReviewers => "Required reviewers",
            Self::ReviewersPlaceholder => "one username per line",
            Self::ReviewersHint => {
                "Up to six people who can read this repository. A job in this environment waits until one of them \
                 approves it; a rejected job fails. With none, jobs run without review."
            }
            Self::PreventSelfReview => "Prevent self-review",
            Self::PreventSelfReviewHint => "Whoever started a run cannot approve its deployments.",
            Self::WaitTimer => "Wait timer",
            Self::WaitTimerHint => {
                "Minutes a job in this environment waits, once it is ready and approved, before it runs. 0 for none; at \
                 most 43200 (30 days)."
            }
            Self::DeploymentBranches => "Deployment branches and tags",
            Self::AnyBranch => "Any branch or tag",
            Self::ProtectedOnly => "Protected branches only",
            Self::ProtectedOnlyHint => "Branches that have a branch protection rule.",
            Self::SelectedBranches => "Selected branches and tags",
            Self::PatternsLabel => "Branch and tag patterns",
            Self::PatternsPlaceholder => "main",
            Self::SaveRules => "Save protection rules",
            Self::DeleteTitle => "Delete this environment",
            Self::DeleteHint => "Its secrets, variables and rules go too. Jobs waiting for its reviewers run at once.",
            Self::DeleteEnvironment => "Delete environment",
        };
        text.literal(literal);
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
