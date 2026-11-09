//! German (`de`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("Secrets"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("Workflows lesen sie als ");
            text.argument(expression);
            text.literal(
                ". Werte werden verschlüsselt und können nicht erneut angezeigt, nur ersetzt werden. Ausführungen aus \
                 Forks und von Autoren ohne Schreibzugriff erhalten sie nie; wer pushen kann, kann sie über einen \
                 Workflow auslesen.",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("Secrets können weder gespeichert noch verwendet werden: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("Keine Secrets"),
        ActionsSecretsMessage::NoSecretsHint => {
            text.literal("Fügen Sie ein Secret für die Tokens und Schlüssel hinzu, die Ihre Workflows benötigen.");
        }
        ActionsSecretsMessage::CannotDecrypt => {
            text.literal("Kann nicht entschlüsselt werden – legen Sie es erneut fest");
        }
        ActionsSecretsMessage::Updated { when } => {
            text.literal("Aktualisiert ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("Alle Repositorys"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " ausgewähltes Repository",
                _ => " ausgewählte Repositorys",
            });
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("Secret ");
            text.argument(name);
            text.literal(" löschen");
        }
        ActionsSecretsMessage::Delete => text.literal("Löschen"),
        ActionsSecretsMessage::ReplaceValue => text.literal("Wert ersetzen"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.literal("Neuer Wert von ");
            text.argument(name);
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("Secret ersetzen"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("Repositorys auswählen"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("Die Repositorys, deren Workflows ");
            text.argument(name);
            text.literal(" erhalten, ohne seinen Wert zu ändern.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("Repositorys speichern"),
        ActionsSecretsMessage::NewSecret => text.literal("Neues Secret"),
        ActionsSecretsMessage::Name => text.literal("Name"),
        ActionsSecretsMessage::SecretNameHint => text.literal(
            "Buchstaben, Ziffern und Unterstriche, nicht beginnend mit einer Ziffer oder mit GITHUB_, GITEA_, \
             FORGEJO_ oder RBIT_. Namen werden in Großbuchstaben gespeichert; ein vorhandener Name wird ersetzt.",
        ),
        ActionsSecretsMessage::Value => text.literal("Wert"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("Höchstens ");
            text.number(kib);
            text.literal(" KiB. Er kann nicht erneut angezeigt werden.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("Secret hinzufügen"),
        ActionsSecretsMessage::VariablesTitle => text.literal("Variablen"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("Workflows lesen sie als ");
            text.argument(expression);
            text.literal(
                ". Sie sind nicht geheim: Wer einen Workflow ändern kann, kann sie ausgeben. Ausführungen aus Forks \
                 und von Autoren ohne Schreibzugriff erhalten sie nie.",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("Keine Variablen"),
        ActionsSecretsMessage::NoVariablesHint => text.literal(
            "Fügen Sie eine Variable für Konfiguration hinzu, die Ihre Workflows gemeinsam nutzen, etwa eine Region \
             oder eine Version.",
        ),
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("Variable ");
            text.argument(name);
            text.literal(" löschen");
        }
        ActionsSecretsMessage::Edit => text.literal("Bearbeiten"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.literal("Wert von ");
            text.argument(name);
        }
        ActionsSecretsMessage::SaveVariable => text.literal("Variable speichern"),
        ActionsSecretsMessage::NewVariable => text.literal("Neue Variable"),
        ActionsSecretsMessage::VariableNameHint => {
            text.literal("Dieselben Regeln wie für Secret-Namen; ein vorhandener Name wird ersetzt.");
        }
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("Höchstens ");
            text.number(kib);
            text.literal(" KiB.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("Variable hinzufügen"),
    }
}
