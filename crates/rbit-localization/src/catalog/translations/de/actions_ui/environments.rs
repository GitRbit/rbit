//! German (`de`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;

entries! { De:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("Umgebungen"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("Ein Job, der mit ");
            text.argument(key);
            text.literal(
                " eine Umgebung nennt, erhält deren Secrets und Variablen, wartet auf einen ihrer Reviewer, sofern sie \
                 welche hat, und läuft nur von den Branches aus, die sie zulässt. Eine Umgebung erscheint hier, sobald \
                 ein Workflow sie zum ersten Mal nennt.",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("Keine Umgebungen"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("Fügen Sie unten eine hinzu oder führen Sie einen Workflow aus, dessen Job mit ");
            text.argument(key);
            text.literal(" eine Umgebung nennt.");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("Neue Umgebung"),
        ActionsEnvironmentMessage::Name => text.literal("Name"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("Umgebung hinzufügen"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(" Reviewer");
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("geschützte Branches"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("Branches ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("Kein Schutz"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("Schutzregeln"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("Erforderliche Reviewer"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("ein Benutzername pro Zeile"),
        ActionsEnvironmentMessage::ReviewersHint => text.literal(
            "Bis zu sechs Personen, die dieses Repository lesen können. Ein Job in dieser Umgebung wartet, bis eine \
             von ihnen ihn genehmigt; ein abgelehnter Job schlägt fehl. Ohne Reviewer laufen Jobs ohne Review.",
        ),
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("Eigenes Review verhindern"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => {
            text.literal("Wer eine Ausführung gestartet hat, kann deren Deployments nicht genehmigen.");
        }
        ActionsEnvironmentMessage::WaitTimer => text.literal("Wartezeit"),
        ActionsEnvironmentMessage::WaitTimerHint => text.literal(
            "Minuten, die ein Job in dieser Umgebung wartet, nachdem er bereit und genehmigt ist, bevor er läuft. 0 \
             für keine; höchstens 43200 (30 Tage).",
        ),
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("Branches und Tags für Deployments"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("Beliebiger Branch oder Tag"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("Nur geschützte Branches"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => text.literal("Branches mit einer Branch-Schutzregel."),
        ActionsEnvironmentMessage::SelectedBranches => text.literal("Ausgewählte Branches und Tags"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("Muster für Branches und Tags"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("Ein Muster pro Zeile, wie in Workflow-Filtern. Branches nach Name (");
            text.argument(branch);
            text.literal("), Tags mit ");
            text.argument(tags);
            text.literal(" (");
            text.argument(tag);
            text.literal("); ");
            text.argument(exclude);
            text.literal(" schließt aus.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("Schutzregeln speichern"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("Diese Umgebung löschen"),
        ActionsEnvironmentMessage::DeleteHint => text.literal(
            "Ihre Secrets, Variablen und Regeln werden ebenfalls gelöscht. Jobs, die auf ihre Reviewer warten, laufen \
             sofort.",
        ),
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("Umgebung löschen"),
    }
}
