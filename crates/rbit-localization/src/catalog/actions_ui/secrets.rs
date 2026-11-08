//! The secrets and variables sections of the Actions settings pages.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsSecretsMessage<'a> {
    SecretsTitle,
    /// `expression` is how a workflow reads a secret.
    SecretsIntro {
        expression: &'a str,
    },
    /// `problem` explains why the key is unusable.
    SecretsUnavailable {
        problem: &'a str,
    },
    NoSecrets,
    NoSecretsHint,
    CannotDecrypt,
    /// `when` is a relative time.
    Updated {
        when: &'a str,
    },
    AllRepositories,
    SelectedRepositories {
        count: u64,
    },
    DeleteSecret {
        name: &'a str,
    },
    Delete,
    ReplaceValue,
    NewValueOf {
        name: &'a str,
    },
    ReplaceSecret,
    ChooseRepositories,
    Recipients {
        name: &'a str,
    },
    SaveRepositories,
    NewSecret,
    Name,
    SecretNameHint,
    Value,
    SecretValueHint {
        kib: u64,
    },
    AddSecret,
    VariablesTitle,
    VariablesIntro {
        expression: &'a str,
    },
    NoVariables,
    NoVariablesHint,
    DeleteVariable {
        name: &'a str,
    },
    Edit,
    ValueOf {
        name: &'a str,
    },
    SaveVariable,
    NewVariable,
    VariableNameHint,
    VariableValueHint {
        kib: u64,
    },
    AddVariable,
}

impl MessageFamily for ActionsSecretsMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::SecretsTitle => "actions.secrets.title",
            Self::SecretsIntro { .. } => "actions.secrets.intro",
            Self::SecretsUnavailable { .. } => "actions.secrets.unavailable",
            Self::NoSecrets => "actions.secrets.empty",
            Self::NoSecretsHint => "actions.secrets.empty-hint",
            Self::CannotDecrypt => "actions.secrets.cannot-decrypt",
            Self::Updated { .. } => "actions.secrets.updated",
            Self::AllRepositories => "actions.secrets.all-repositories",
            Self::SelectedRepositories { .. } => "actions.secrets.selected-repositories",
            Self::DeleteSecret { .. } => "actions.secrets.delete-label",
            Self::Delete => "actions.secrets.delete",
            Self::ReplaceValue => "actions.secrets.replace-value",
            Self::NewValueOf { .. } => "actions.secrets.new-value-of",
            Self::ReplaceSecret => "actions.secrets.replace",
            Self::ChooseRepositories => "actions.secrets.choose-repositories",
            Self::Recipients { .. } => "actions.secrets.recipients",
            Self::SaveRepositories => "actions.secrets.save-repositories",
            Self::NewSecret => "actions.secrets.new",
            Self::Name => "actions.secrets.name",
            Self::SecretNameHint => "actions.secrets.name-hint",
            Self::Value => "actions.secrets.value",
            Self::SecretValueHint { .. } => "actions.secrets.value-hint",
            Self::AddSecret => "actions.secrets.add",
            Self::VariablesTitle => "actions.variables.title",
            Self::VariablesIntro { .. } => "actions.variables.intro",
            Self::NoVariables => "actions.variables.empty",
            Self::NoVariablesHint => "actions.variables.empty-hint",
            Self::DeleteVariable { .. } => "actions.variables.delete-label",
            Self::Edit => "actions.variables.edit",
            Self::ValueOf { .. } => "actions.variables.value-of",
            Self::SaveVariable => "actions.variables.save",
            Self::NewVariable => "actions.variables.new",
            Self::VariableNameHint => "actions.variables.name-hint",
            Self::VariableValueHint { .. } => "actions.variables.value-hint",
            Self::AddVariable => "actions.variables.add",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::SecretsIntro { expression } => {
                text.literal("Workflows read them as ");
                text.argument(expression);
                return text.literal(
                    ". Values are encrypted and can't be shown again, only replaced. Runs from forks and from authors \
                     without write access never receive them; anyone who can push can read them through a workflow.",
                );
            }
            Self::SecretsUnavailable { problem } => {
                text.literal("Secrets cannot be stored or used: ");
                return text.argument(problem);
            }
            Self::Updated { when } => {
                text.literal("Updated ");
                return text.argument(when);
            }
            Self::SelectedRepositories { count } => {
                text.number(count);
                return text.literal(if count == 1 { " selected repository" } else { " selected repositories" });
            }
            Self::DeleteSecret { name } => {
                text.literal("Delete secret ");
                return text.argument(name);
            }
            Self::NewValueOf { name } => {
                text.literal("New value of ");
                return text.argument(name);
            }
            Self::Recipients { name } => {
                text.literal("The repositories whose workflows receive ");
                text.argument(name);
                return text.literal(", without changing its value.");
            }
            Self::SecretValueHint { kib } => {
                text.literal("At most ");
                text.number(kib);
                return text.literal(" KiB. It can't be shown again.");
            }
            Self::VariablesIntro { expression } => {
                text.literal("Workflows read them as ");
                text.argument(expression);
                return text.literal(
                    ". They are not secret: anyone who can change a workflow can print them. Runs from forks and from \
                     authors without write access never receive them.",
                );
            }
            Self::DeleteVariable { name } => {
                text.literal("Delete variable ");
                return text.argument(name);
            }
            Self::ValueOf { name } => {
                text.literal("Value of ");
                return text.argument(name);
            }
            Self::VariableValueHint { kib } => {
                text.literal("At most ");
                text.number(kib);
                return text.literal(" KiB.");
            }
            Self::SecretsTitle => "Secrets",
            Self::NoSecrets => "No secrets",
            Self::NoSecretsHint => "Add a secret for the tokens and keys your workflows need.",
            Self::CannotDecrypt => "Cannot be decrypted — set it again",
            Self::AllRepositories => "All repositories",
            Self::Delete => "Delete",
            Self::ReplaceValue => "Replace the value",
            Self::ReplaceSecret => "Replace secret",
            Self::ChooseRepositories => "Choose repositories",
            Self::SaveRepositories => "Save repositories",
            Self::NewSecret => "New secret",
            Self::Name => "Name",
            Self::SecretNameHint => {
                "Letters, digits and underscores, not starting with a digit or with GITHUB_, GITEA_, FORGEJO_ or RBIT_. \
                 Names are stored in capitals; an existing name is replaced."
            }
            Self::Value => "Value",
            Self::AddSecret => "Add secret",
            Self::VariablesTitle => "Variables",
            Self::NoVariables => "No variables",
            Self::NoVariablesHint => "Add a variable for configuration your workflows share, such as a region or a version.",
            Self::Edit => "Edit",
            Self::SaveVariable => "Save variable",
            Self::NewVariable => "New variable",
            Self::VariableNameHint => "The same rules as secret names; an existing name is replaced.",
            Self::AddVariable => "Add variable",
        };
        text.literal(literal);
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
