//! Spanish (`es`) entries for `catalog/overview_controls.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::overview_controls::OverviewControlMessage;

entries! { Es:
    OverviewControlMessage => MessageWriter |text| {
        OverviewControlMessage::Push => text.literal("envío"),
        OverviewControlMessage::PullRequest => text.literal("solicitud de incorporación"),
        OverviewControlMessage::Manual => text.literal("manual"),
        OverviewControlMessage::Schedule => text.literal("programación"),
        OverviewControlMessage::WorkflowRun => text.literal("tras un flujo de trabajo"),
        OverviewControlMessage::Dispatch => text.literal("despacho"),
        OverviewControlMessage::Success => text.literal("Correcto"),
        OverviewControlMessage::Failure => text.literal("Error"),
        OverviewControlMessage::Cancelled => text.literal("Cancelado"),
        OverviewControlMessage::Skipped => text.literal("Omitido"),
        OverviewControlMessage::Queued => text.literal("En cola"),
        OverviewControlMessage::Running => text.literal("En curso"),
        OverviewControlMessage::Waiting => text.literal("En espera"),
        OverviewControlMessage::Approval => text.literal("Pendiente de aprobación"),
        OverviewControlMessage::RunWorkflow => text.literal("Ejecutar flujo"),
        OverviewControlMessage::WorkflowFrom => text.literal("Usar el flujo de"),
        OverviewControlMessage::Change => text.literal("Cambiar"),
    }
}
