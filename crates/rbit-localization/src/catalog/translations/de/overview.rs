//! German (`de`) entries for `catalog/overview.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview::ActionsOverviewMessage;
use crate::plural::Plural;

entries! { De:
    ActionsOverviewMessage => MessageWriter |text| {
        ActionsOverviewMessage::Workflows => text.literal("Workflows"),
        ActionsOverviewMessage::AllWorkflows => text.literal("Alle Workflows"),
        ActionsOverviewMessage::AllRuns => text.literal("Alle Workflow-Ausführungen"),
        ActionsOverviewMessage::Filters => text.literal("Filter für Workflow-Ausführungen"),
        ActionsOverviewMessage::Branch => text.literal("Branch"),
        ActionsOverviewMessage::Actor => text.literal("Auslöser"),
        ActionsOverviewMessage::Event => text.literal("Ereignis"),
        ActionsOverviewMessage::Status => text.literal("Status"),
        ActionsOverviewMessage::AnyEvent => text.literal("Alle Ereignisse"),
        ActionsOverviewMessage::AnyStatus => text.literal("Alle Status"),
        ActionsOverviewMessage::Filter => text.literal("Filtern"),
        ActionsOverviewMessage::Clear => text.literal("Zurücksetzen"),
        ActionsOverviewMessage::EnableWorkflow => text.literal("Workflow aktivieren"),
        ActionsOverviewMessage::DisableWorkflow => text.literal("Workflow deaktivieren"),
        ActionsOverviewMessage::WorkflowDisabled => text.literal(
            "Dieser Workflow ist deaktiviert: Er startet bei keinem Ereignis eine Ausführung, bis er aktiviert wird.",
        ),
        ActionsOverviewMessage::NoMatchingRuns => text.literal("Keine passenden Ausführungen"),
        ActionsOverviewMessage::NoMatchingRunsDescription => {
            text.literal("Keine Workflow-Ausführung passt zu diesen Filtern.");
        }
        ActionsOverviewMessage::NoRunsYet => text.literal("Noch keine Ausführungen"),
        ActionsOverviewMessage::DispatchEmptyDescription => text.literal(
            "Dieser Workflow läuft, wenn er gestartet wird: Verwenden Sie oben „Workflow ausführen“.",
        ),
        ActionsOverviewMessage::NoWorkflowRunsYet => text.literal("Noch keine Workflow-Ausführungen"),
        ActionsOverviewMessage::WorkflowEmptyDescription => text.literal(
            "Fügen Sie unter .rbit/workflows eine Workflow-Datei hinzu und pushen Sie sie: Ausführungen erscheinen \
             hier, sobald Runner sie übernehmen.",
        ),
        ActionsOverviewMessage::Pagination => text.literal("Seitennavigation"),
        ActionsOverviewMessage::OlderRuns => text.literal("Ältere Ausführungen"),
        ActionsOverviewMessage::RunsOnPage { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Ausführung auf dieser Seite",
                _ => " Ausführungen auf dieser Seite",
            });
        }
        ActionsOverviewMessage::JobsFinished { done, total } => {
            text.number(done);
            text.literal(" von ");
            text.number(total);
            text.literal(" Jobs abgeschlossen");
        }
        ActionsOverviewMessage::JobsRunning { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " Job läuft",
                _ => " Jobs laufen",
            });
        }
        ActionsOverviewMessage::WaitingForRunner => text.literal("Wartet auf einen Runner"),
    }
}
