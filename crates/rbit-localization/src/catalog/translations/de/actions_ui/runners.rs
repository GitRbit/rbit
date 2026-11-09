//! German (`de`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { De:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("Runner"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal(
                "Instanz-Runner übernehmen Jobs der Inhaber, die ein Administrator zulässt, oder mit ",
            );
            text.argument(setting);
            text.literal(" die Jobs aller Inhaber.");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("Registrierungstoken erstellt"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal(
                "Kopieren Sie es jetzt: Es wird nicht erneut angezeigt. Führen Sie auf dem Rechner, der Jobs ausführen \
                 soll, den Befehl unten aus, fügen Sie das Token ein, wenn er auf eine Eingabe wartet, und starten Sie \
                 den Runner dann mit ",
            );
            text.argument(command);
            text.literal(".");
        }
        ActionsRunnersMessage::Token => text.literal("Token"),
        ActionsRunnersMessage::CopyToken => text.literal("Token kopieren"),
        ActionsRunnersMessage::Copy => text.literal("Kopieren"),
        ActionsRunnersMessage::Command => text.literal("Befehl"),
        ActionsRunnersMessage::CopyCommand => text.literal("Befehl kopieren"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("Labels legen fest, wie Jobs laufen: ");
            text.argument(container);
            text.literal(" läuft in Docker oder Podman, ");
            text.argument(host);
            text.literal(" läuft direkt auf dem Rechner ohne Isolierung – nur für vertrauenswürdige Repositorys.");
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "diesem Repository"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "Ihren Repositorys"),
        ActionsRunnersMessage::InstanceIntro { program } => {
            intro(text, program, None, "allen Inhabern, denen Instanz-Runner erlaubt sind");
        }
        ActionsRunnersMessage::NoRunners => text.literal("Noch keine Runner"),
        ActionsRunnersMessage::NoRunnersHint => text.literal(
            "Erstellen Sie unten ein Registrierungstoken und registrieren Sie damit einen Runner. Jobs warten, bis \
             ein Runner mit ihren Labels online ist.",
        ),
        ActionsRunnersMessage::Online => text.literal("online"),
        ActionsRunnersMessage::Offline => text.literal("offline"),
        ActionsRunnersMessage::Disabled => text.literal("deaktiviert"),
        ActionsRunnersMessage::NeverSeen => text.literal("nie gesehen"),
        ActionsRunnersMessage::OwnerScope => text.literal("Inhaber"),
        ActionsRunnersMessage::InstanceScope => text.literal("Instanz"),
        ActionsRunnersMessage::Ephemeral => text.literal("kurzlebig"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("Host – keine Isolierung"),
        ActionsRunnersMessage::HostWarning => {
            text.literal("Jobs mit seinen Host-Labels laufen als Benutzer des Runners, ohne Isolierung");
        }
        ActionsRunnersMessage::Capacity { count } => {
            text.number(count);
            text.literal(" gleichzeitig");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("zuletzt gesehen ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("Aktivieren"),
        ActionsRunnersMessage::Disable => text.literal("Deaktivieren"),
        ActionsRunnersMessage::Remove => text.literal("Entfernen"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.argument(name);
            text.literal(" entfernen");
        }
        ActionsRunnersMessage::TokensTitle => text.literal("Registrierungstokens"),
        ActionsRunnersMessage::TokensIntro => text.literal(
            "Ein Runner tauscht ein Registrierungstoken einmalig gegen eigene Zugangsdaten. Tokens sind nur einmal \
             verwendbar und laufen ab, sofern Sie nichts anderes wählen.",
        ),
        ActionsRunnersMessage::ExpiresAfter => text.literal("Läuft ab nach"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("Nur diese Labels"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(optional)"),
        ActionsRunnersMessage::Reusable => {
            text.literal("Beliebig vielen Runnern erlauben, sich damit bis zum Ablauf zu registrieren");
        }
        ActionsRunnersMessage::NewToken => text.literal("Neues Registrierungstoken"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("Erstellt ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("läuft ab ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("nicht mehr verwendbar"),
        ActionsRunnersMessage::Revoked => text.literal("widerrufen"),
        ActionsRunnersMessage::Expired => text.literal("abgelaufen"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("unbegrenzt verwendbar"),
        ActionsRunnersMessage::UsedUp => text.literal("aufgebraucht"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.literal("noch ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Verwendung",
                _ => " Verwendungen",
            });
        }
        ActionsRunnersMessage::Revoke => text.literal("Widerrufen"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Stunde",
                _ => " Stunden",
            });
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Tag",
                _ => " Tage",
            });
        }
    }
}

/// „Rechner mit {program}, die die Jobs von {owner or subject} übernehmen.“
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("Rechner mit ");
    text.argument(program);
    text.literal(", die die Jobs von ");
    match owner {
        Some(owner) => text.argument(owner),
        None => text.literal(subject),
    }
    text.literal(" übernehmen.");
}
