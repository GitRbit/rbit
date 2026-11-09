//! French (`fr`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { Fr:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("push"),
        OverviewControlMessage::PullRequest => text.literal("pull request"),
        OverviewControlMessage::Manual => text.literal("manuel"),
        OverviewControlMessage::Schedule => text.literal("planification"),
        OverviewControlMessage::WorkflowRun => text.literal("après un workflow"),
        OverviewControlMessage::Dispatch => text.literal("dispatch"),
        OverviewControlMessage::Success => text.literal("Réussite"),
        OverviewControlMessage::Failure => text.literal("Échec"),
        OverviewControlMessage::Cancelled => text.literal("Annulé"),
        OverviewControlMessage::Skipped => text.literal("Ignoré"),
        OverviewControlMessage::Queued => text.literal("En file d’attente"),
        OverviewControlMessage::Running => text.literal("En cours"),
        OverviewControlMessage::Waiting => text.literal("En attente"),
        OverviewControlMessage::Approval => text.literal("En attente d’approbation"),
        OverviewControlMessage::RunWorkflow => text.literal("Exécuter le workflow"),
        OverviewControlMessage::WorkflowFrom => text.literal("Utiliser le workflow de"),
        OverviewControlMessage::Change => text.literal("Modifier"),
    }
}
