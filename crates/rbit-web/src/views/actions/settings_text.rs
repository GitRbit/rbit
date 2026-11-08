//! Messages of the Actions settings pages that take arguments: the page introductions,
//! trusted authors, and the secrets and variables sections.
use rbit_localization::{ActionsSecretsMessage as Secrets, ActionsSettingsMessage as Settings, Localized};
use rbit_services::actions::triggers::WORKFLOW_DIRS;

use super::ui_text::{Mark, Rich, SLOTS, UiText};
use crate::views::actions_settings::{SecretRow, TrustedRow};

/// How workflows read a secret and a variable, shown as code.
const SECRET_EXPRESSION: &str = "${{ secrets.NAME }}";
const VARIABLE_EXPRESSION: &str = "${{ vars.NAME }}";
/// The workflow key that lowers the token permission.
const PERMISSIONS_KEY: &str = "permissions:";

impl UiText {
    /// Where a repository's workflows live and which repository runs them.
    pub fn repository_intro(self, owner: &str, name: &str) -> Rich {
        let repository = format!("{owner}/{name}");
        self.rich(
            Settings::RepositoryIntro { directory: SLOTS[0], repository: SLOTS[1] },
            &[Mark::code(WORKFLOW_DIRS[0], "font-mono text-ink"), Mark::name(&repository, "text-ink")],
        )
    }

    pub fn organization_intro(self, organization: &str) -> Rich {
        self.rich(Settings::OrganizationIntro { organization: SLOTS[0] }, &[Mark::name(organization, "text-ink")])
    }

    pub fn permission_hint(self) -> Rich {
        self.rich(Settings::PermissionHint { key: SLOTS[0] }, &[Mark::code(PERMISSIONS_KEY, "font-mono")])
    }

    /// "Trusted {since} by {approver}", the time in a `<time>` element.
    pub fn trusted(self, author: &TrustedRow) -> Rich {
        let since = [Mark::time(&author.since, &author.since_iso)];
        match &author.approved_by {
            Some(approver) => self.rich(Settings::TrustedSinceBy { since: SLOTS[0], approver }, &since),
            None => self.rich(Settings::TrustedSince { since: SLOTS[0] }, &since),
        }
    }

    pub fn stop_trusting(self, name: &str) -> Localized {
        self.get(Settings::StopTrusting { name })
    }

    pub fn secrets_intro(self) -> Rich {
        self.rich(Secrets::SecretsIntro { expression: SLOTS[0] }, &[Mark::code(SECRET_EXPRESSION, "font-mono text-ink")])
    }

    pub fn secrets_unavailable(self, problem: &str) -> Localized {
        self.get(Secrets::SecretsUnavailable { problem })
    }

    /// "Updated {when}", the time in a `<time>` element.
    pub fn updated(self, when: &str, iso: &str) -> Rich {
        self.rich(Secrets::Updated { when: SLOTS[0] }, &[Mark::time(when, iso)])
    }

    /// Which of the owner's repositories receive an owner secret; nothing for a repository's.
    pub fn visibility(self, secret: &SecretRow) -> Option<Localized> {
        let count = secret.receivers?;
        Some(self.get(if secret.selected {
            Secrets::SelectedRepositories { count: u64::try_from(count).unwrap_or(u64::MAX) }
        } else {
            Secrets::AllRepositories
        }))
    }

    pub fn delete_secret(self, name: &str) -> Localized {
        self.get(Secrets::DeleteSecret { name })
    }

    pub fn new_value_of(self, name: &str) -> Rich {
        self.rich(Secrets::NewValueOf { name: SLOTS[0] }, &[Mark::code(name, "font-mono")])
    }

    /// The repositories that receive the secret `name`.
    pub fn recipients(self, name: &str) -> Rich {
        self.rich(Secrets::Recipients { name: SLOTS[0] }, &[Mark::code(name, "font-mono text-ink")])
    }

    pub fn secret_value_hint(self, kib: usize) -> Localized {
        self.get(Secrets::SecretValueHint { kib: u64::try_from(kib).unwrap_or(u64::MAX) })
    }

    pub fn variables_intro(self) -> Rich {
        self.rich(Secrets::VariablesIntro { expression: SLOTS[0] }, &[Mark::code(VARIABLE_EXPRESSION, "font-mono text-ink")])
    }

    pub fn delete_variable(self, name: &str) -> Localized {
        self.get(Secrets::DeleteVariable { name })
    }

    pub fn value_of(self, name: &str) -> Rich {
        self.rich(Secrets::ValueOf { name: SLOTS[0] }, &[Mark::code(name, "font-mono")])
    }

    pub fn variable_value_hint(self, kib: usize) -> Localized {
        self.get(Secrets::VariableValueHint { kib: u64::try_from(kib).unwrap_or(u64::MAX) })
    }
}
