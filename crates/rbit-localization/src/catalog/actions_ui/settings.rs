//! The Actions settings of a repository, an organization and an account: the switch,
//! the default token permission, trusted authors, notices and repository access.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsSettingsMessage<'a> {
    /// `directory` is the workflow directory; `repository` is `owner/name`.
    RepositoryIntro {
        directory: &'a str,
        repository: &'a str,
    },
    OrganizationIntro {
        organization: &'a str,
    },
    AccountIntro,
    General,
    RunWorkflows,
    RunWorkflowsHint,
    DefaultPermission,
    /// `key` is the workflow key that lowers the permission.
    PermissionHint {
        key: &'a str,
    },
    PermissionRead,
    PermissionReadHint,
    PermissionWrite,
    PermissionWriteHint,
    SaveChanges,
    RepositoryRunnersHint,
    OrganizationRunnersHint,
    AccountRunnersHint,
    EnvironmentsHint,
    TrustedTitle,
    TrustedIntro,
    NoTrustedAuthors,
    NoTrustedAuthorsHint,
    /// `since` is a relative time.
    TrustedSince {
        since: &'a str,
    },
    TrustedSinceBy {
        since: &'a str,
        approver: &'a str,
    },
    StopTrusting {
        name: &'a str,
    },
    Remove,
    Saved,
    SecretSaved,
    SecretDeleted,
    VariableSaved,
    VariableDeleted,
    Untrusted,
    RepositoryAccess,
    AllRepositories,
    AllRepositoriesHint,
    SelectedRepositories,
    SelectedRepositoriesHint,
    NoRepositoriesYet,
}

impl MessageFamily for ActionsSettingsMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::RepositoryIntro { .. } => "actions.settings.repository-intro",
            Self::OrganizationIntro { .. } => "actions.settings.organization-intro",
            Self::AccountIntro => "actions.settings.account-intro",
            Self::General => "actions.settings.general",
            Self::RunWorkflows => "actions.settings.run-workflows",
            Self::RunWorkflowsHint => "actions.settings.run-workflows-hint",
            Self::DefaultPermission => "actions.settings.default-permission",
            Self::PermissionHint { .. } => "actions.settings.permission-hint",
            Self::PermissionRead => "actions.settings.permission-read",
            Self::PermissionReadHint => "actions.settings.permission-read-hint",
            Self::PermissionWrite => "actions.settings.permission-write",
            Self::PermissionWriteHint => "actions.settings.permission-write-hint",
            Self::SaveChanges => "actions.settings.save",
            Self::RepositoryRunnersHint => "actions.settings.runners-hint.repository",
            Self::OrganizationRunnersHint => "actions.settings.runners-hint.organization",
            Self::AccountRunnersHint => "actions.settings.runners-hint.account",
            Self::EnvironmentsHint => "actions.settings.environments-hint",
            Self::TrustedTitle => "actions.settings.trusted.title",
            Self::TrustedIntro => "actions.settings.trusted.intro",
            Self::NoTrustedAuthors => "actions.settings.trusted.empty",
            Self::NoTrustedAuthorsHint => "actions.settings.trusted.empty-hint",
            Self::TrustedSince { .. } => "actions.settings.trusted.since",
            Self::TrustedSinceBy { .. } => "actions.settings.trusted.since-by",
            Self::StopTrusting { .. } => "actions.settings.trusted.stop",
            Self::Remove => "actions.settings.trusted.remove",
            Self::Saved => "actions.settings.notice.saved",
            Self::SecretSaved => "actions.settings.notice.secret-saved",
            Self::SecretDeleted => "actions.settings.notice.secret-deleted",
            Self::VariableSaved => "actions.settings.notice.variable-saved",
            Self::VariableDeleted => "actions.settings.notice.variable-deleted",
            Self::Untrusted => "actions.settings.notice.untrusted",
            Self::RepositoryAccess => "actions.settings.access.title",
            Self::AllRepositories => "actions.settings.access.all",
            Self::AllRepositoriesHint => "actions.settings.access.all-hint",
            Self::SelectedRepositories => "actions.settings.access.selected",
            Self::SelectedRepositoriesHint => "actions.settings.access.selected-hint",
            Self::NoRepositoriesYet => "actions.settings.access.none",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::RepositoryIntro { directory, repository } => {
                text.literal("Workflows under ");
                text.argument(directory);
                text.literal(" run on runners when their events happen in ");
                text.argument(repository);
                return text.literal(".");
            }
            Self::OrganizationIntro { organization } => {
                text.literal("Secrets and variables for the workflows of ");
                text.argument(organization);
                return text.literal("'s repositories. A repository's own secret or variable of the same name wins.");
            }
            Self::PermissionHint { key } => {
                text.literal("A workflow's ");
                text.argument(key);
                return text.literal(" can only lower it. Runs from forks and outside authors always get a read-only token.");
            }
            Self::TrustedSince { since } => {
                text.literal("Trusted ");
                return text.argument(since);
            }
            Self::TrustedSinceBy { since, approver } => {
                text.literal("Trusted ");
                text.argument(since);
                text.literal(" by ");
                return text.argument(approver);
            }
            Self::StopTrusting { name } => {
                text.literal("Stop trusting ");
                return text.argument(name);
            }
            Self::AccountIntro => {
                "Secrets and variables for the workflows of your own repositories. A repository's own secret or variable of \
                 the same name wins."
            }
            Self::General => "General",
            Self::RunWorkflows => "Run workflows in this repository",
            Self::RunWorkflowsHint => "Off: no new runs start and the Actions tab is hidden. Earlier runs and their logs are kept.",
            Self::DefaultPermission => "Default workflow token permission",
            Self::PermissionRead => "Read",
            Self::PermissionReadHint => "Workflows can read the repository's contents, issues and pull requests.",
            Self::PermissionWrite => "Read and write",
            Self::PermissionWriteHint => {
                "Workflows can also push, and comment on issues and pull requests. Branch protection still applies."
            }
            Self::SaveChanges => "Save changes",
            Self::RepositoryRunnersHint => "This repository's runners, those it inherits, and registration tokens.",
            Self::OrganizationRunnersHint => "Runners for every repository of the organization, and registration tokens.",
            Self::AccountRunnersHint => "Runners for every repository of your account, and registration tokens.",
            Self::EnvironmentsHint => "Deployment environments: required reviewers, allowed branches, and their own secrets and variables.",
            Self::TrustedTitle => "Trusted pull request authors",
            Self::TrustedIntro => {
                "Their pull requests from forks run without waiting for approval, still with a read-only token and without \
                 secrets or variables. Authors are added from a run's approval with “always run this author's pull \
                 requests”."
            }
            Self::NoTrustedAuthors => "No trusted authors",
            Self::NoTrustedAuthorsHint => {
                "Pull requests from forks and from people without write access wait for a writer to approve each run."
            }
            Self::Remove => "Remove",
            Self::Saved => "Settings saved.",
            Self::SecretSaved => "Secret saved.",
            Self::SecretDeleted => "Secret deleted.",
            Self::VariableSaved => "Variable saved.",
            Self::VariableDeleted => "Variable deleted.",
            Self::Untrusted => "The author's pull requests need approval again.",
            Self::RepositoryAccess => "Repository access",
            Self::AllRepositories => "All repositories",
            Self::AllRepositoriesHint => "Every repository of this owner, including ones created later.",
            Self::SelectedRepositories => "Selected repositories",
            Self::SelectedRepositoriesHint => "Only the repositories ticked below.",
            Self::NoRepositoriesYet => "This owner has no repositories yet.",
        };
        text.literal(literal);
    }
}
