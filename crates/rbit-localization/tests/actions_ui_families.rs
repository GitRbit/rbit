//! Every key of the Actions settings families exists, renders in every catalog and falls
//! back to English in Spanish, which has not translated them yet.
use std::{collections::BTreeSet, fmt::Debug};

use rbit_localization::{
    ActionsEnvironmentMessage, ActionsRunnersMessage, ActionsSecretsMessage, ActionsSettingsMessage, Catalog, Locale, MessageFamily,
};

const DERIVED: [Locale; 3] = [Locale::English, Locale::PseudoExpanded, Locale::PseudoRtl];

/// Ids are distinct; English and the diagnostics render without fallback; Spanish falls
/// back to exactly the English text.
fn check_family<M: MessageFamily + Debug>(messages: &[M], distinct_ids: usize) {
    let ids: BTreeSet<_> = messages.iter().map(|message| message.id()).collect();
    assert_eq!(ids.len(), distinct_ids, "ids are unique and every variant is listed");
    for &message in messages {
        for locale in DERIVED {
            let text = Catalog::new(locale).localized(message);
            assert!(!text.as_str().is_empty(), "{message:?} in {locale:?}");
            assert!(!text.fallback(), "{message:?} derives from English in {locale:?}");
        }
        let english = Catalog::new(Locale::English).localized(message);
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert!(spanish.fallback(), "{message:?} is not translated yet");
        assert_eq!(spanish.as_str(), english.as_str(), "{message:?}");
    }
}

#[test]
fn settings_keys_render_and_fall_back() {
    use ActionsSettingsMessage as M;
    check_family(
        &[
            M::RepositoryIntro { directory: "dir", repository: "alice/rocket" },
            M::OrganizationIntro { organization: "acme" },
            M::AccountIntro,
            M::General,
            M::RunWorkflows,
            M::RunWorkflowsHint,
            M::DefaultPermission,
            M::PermissionHint { key: "permissions:" },
            M::PermissionRead,
            M::PermissionReadHint,
            M::PermissionWrite,
            M::PermissionWriteHint,
            M::SaveChanges,
            M::RepositoryRunnersHint,
            M::OrganizationRunnersHint,
            M::AccountRunnersHint,
            M::EnvironmentsHint,
            M::TrustedTitle,
            M::TrustedIntro,
            M::NoTrustedAuthors,
            M::NoTrustedAuthorsHint,
            M::TrustedSince { since: "2 days ago" },
            M::TrustedSinceBy { since: "2 days ago", approver: "bob" },
            M::StopTrusting { name: "carol" },
            M::Remove,
            M::Saved,
            M::SecretSaved,
            M::SecretDeleted,
            M::VariableSaved,
            M::VariableDeleted,
            M::Untrusted,
            M::RepositoryAccess,
            M::AllRepositories,
            M::AllRepositoriesHint,
            M::SelectedRepositories,
            M::SelectedRepositoriesHint,
            M::NoRepositoriesYet,
        ],
        37,
    );
}

#[test]
fn secrets_keys_render_and_fall_back() {
    use ActionsSecretsMessage as M;
    check_family(
        &[
            M::SecretsTitle,
            M::SecretsIntro { expression: "${{ secrets.NAME }}" },
            M::SecretsUnavailable { problem: "no key" },
            M::NoSecrets,
            M::NoSecretsHint,
            M::CannotDecrypt,
            M::Updated { when: "now" },
            M::AllRepositories,
            M::SelectedRepositories { count: 1 },
            M::SelectedRepositories { count: 2 },
            M::DeleteSecret { name: "TOKEN" },
            M::Delete,
            M::ReplaceValue,
            M::NewValueOf { name: "TOKEN" },
            M::ReplaceSecret,
            M::ChooseRepositories,
            M::Recipients { name: "TOKEN" },
            M::SaveRepositories,
            M::NewSecret,
            M::Name,
            M::SecretNameHint,
            M::Value,
            M::SecretValueHint { kib: 48 },
            M::AddSecret,
            M::VariablesTitle,
            M::VariablesIntro { expression: "${{ vars.NAME }}" },
            M::NoVariables,
            M::NoVariablesHint,
            M::DeleteVariable { name: "REGION" },
            M::Edit,
            M::ValueOf { name: "REGION" },
            M::SaveVariable,
            M::NewVariable,
            M::VariableNameHint,
            M::VariableValueHint { kib: 48 },
            M::AddVariable,
        ],
        35,
    );
}

#[test]
fn environment_keys_render_and_fall_back() {
    use ActionsEnvironmentMessage as M;
    check_family(
        &[
            M::Title,
            M::Intro { key: "environment:" },
            M::NoEnvironments,
            M::NoEnvironmentsHint { key: "environment:" },
            M::NewEnvironment,
            M::Name,
            M::NamePlaceholder,
            M::AddEnvironment,
            M::Reviewers { count: 1 },
            M::Reviewers { count: 2 },
            M::ProtectedBranches,
            M::Branches { patterns: "main, release/*" },
            M::NoProtection,
            M::ProtectionRules,
            M::RequiredReviewers,
            M::ReviewersPlaceholder,
            M::ReviewersHint,
            M::PreventSelfReview,
            M::PreventSelfReviewHint,
            M::WaitTimer,
            M::WaitTimerHint,
            M::DeploymentBranches,
            M::AnyBranch,
            M::ProtectedOnly,
            M::ProtectedOnlyHint,
            M::SelectedBranches,
            M::PatternsLabel,
            M::PatternsPlaceholder,
            M::PatternsHint { branch: "release/*", tags: "refs/tags/", tag: "refs/tags/v*", exclude: "!" },
            M::SaveRules,
            M::DeleteTitle,
            M::DeleteHint,
            M::DeleteEnvironment,
        ],
        32,
    );
}

#[test]
fn runner_keys_render_and_fall_back() {
    use ActionsRunnersMessage as M;
    check_family(
        &[
            M::Title,
            M::AdminIntro { setting: "instance_runners_for_all" },
            M::TokenCreated,
            M::TokenCreatedHint { command: "rbit-runner run" },
            M::Token,
            M::CopyToken,
            M::Copy,
            M::Command,
            M::CopyCommand,
            M::LabelsHint { container: "name:container:image", host: "name:host" },
            M::RepositoryIntro { program: "rbit-runner" },
            M::OwnerIntro { program: "rbit-runner", owner: "acme" },
            M::AccountIntro { program: "rbit-runner" },
            M::InstanceIntro { program: "rbit-runner" },
            M::NoRunners,
            M::NoRunnersHint,
            M::Online,
            M::Offline,
            M::Disabled,
            M::NeverSeen,
            M::OwnerScope,
            M::InstanceScope,
            M::Ephemeral,
            M::HostNoIsolation,
            M::HostWarning,
            M::Capacity { count: 2 },
            M::Seen { when: "now" },
            M::Enable,
            M::Disable,
            M::Remove,
            M::RemoveRunner { name: "build-1" },
            M::TokensTitle,
            M::TokensIntro,
            M::ExpiresAfter,
            M::OnlyTheseLabels,
            M::LabelsPlaceholder,
            M::Optional,
            M::Reusable,
            M::NewToken,
            M::Created { when: "now" },
            M::Expires { when: "in 1 hour" },
            M::NoLongerUsable,
            M::Revoked,
            M::Expired,
            M::UnlimitedUses,
            M::UsedUp,
            M::UsesLeft { count: 1 },
            M::Revoke,
            M::Hours { count: 1 },
            M::Days { count: 7 },
        ],
        50,
    );
}

fn english<M: MessageFamily>(message: M) -> String {
    Catalog::new(Locale::English).localized(message).as_str().to_owned()
}

#[test]
fn counts_choose_their_noun_and_arguments_stay_plain_text() {
    assert_eq!(english(ActionsRunnersMessage::UsesLeft { count: 1 }), "1 use left");
    assert_eq!(english(ActionsRunnersMessage::UsesLeft { count: 3 }), "3 uses left");
    assert_eq!(english(ActionsRunnersMessage::Days { count: 1 }), "1 day");
    assert_eq!(english(ActionsRunnersMessage::Days { count: 30 }), "30 days");
    assert_eq!(
        english(ActionsRunnersMessage::OwnerIntro { program: "rbit-runner", owner: "acme" }),
        "Machines running rbit-runner that take the jobs of acme."
    );
    assert_eq!(english(ActionsSecretsMessage::SelectedRepositories { count: 1 }), "1 selected repository");
    assert_eq!(english(ActionsSecretsMessage::SelectedRepositories { count: 4 }), "4 selected repositories");
    assert_eq!(english(ActionsEnvironmentMessage::Reviewers { count: 2 }), "2 reviewers");
    let attack = "<img src=x onerror=alert(1)>";
    let spanish = Catalog::new(Locale::Spanish).localized(ActionsSettingsMessage::StopTrusting { name: attack });
    assert_eq!(spanish.as_str(), format!("Stop trusting {attack}"));
    let rtl = Catalog::new(Locale::PseudoRtl).localized(ActionsSecretsMessage::DeleteSecret { name: attack });
    assert!(rtl.as_str().contains(&format!("\u{2068}{attack}\u{2069}")), "{rtl}");
}
