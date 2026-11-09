//! Spanish (`es`) entries for `catalog/settings/protection.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::ProtectionSentence;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};
use crate::plural::Plural;

entries! { Es:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("Marca la casilla para confirmar."),
        ProtectionMessage::Settings => text.literal("Configuración"),
        ProtectionMessage::Title => text.literal("Protección de ramas"),
        ProtectionMessage::AddRule => text.literal("Añadir regla"),
        ProtectionMessage::Deleted => text.literal("Regla eliminada."),
        ProtectionMessage::Empty => text.literal("No hay ramas protegidas"),
        ProtectionMessage::EmptyText => text.literal(
            "Añade una regla para que una rama como main no reciba envíos forzados, no se elimine ni cambie sin una \
             solicitud de cambios.",
        ),
        ProtectionMessage::NoMatchYet => text.literal("Aún no coincide con ninguna rama."),
        ProtectionMessage::MatchesLabel => text.literal("Coincide con:"),
        ProtectionMessage::NewRule => text.literal("Nueva regla de protección de ramas"),
        ProtectionMessage::NewRuleIntro => text.literal(
            "La regla se aplica a todas las ramas que coinciden con su patrón, ahora y en el futuro.",
        ),
        ProtectionMessage::Saved => text.literal("Regla guardada."),
        ProtectionMessage::Rule => text.literal("Regla"),
        ProtectionMessage::Pattern => text.literal("Patrón de nombre de rama"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("Un nombre de rama o un patrón:"),
        ProtectionMessage::PatternOne => text.literal("coincide dentro de una parte del nombre,"),
        ProtectionMessage::PatternAcross => text.literal("a través de varias partes ("),
        ProtectionMessage::PatternChar => text.literal("), "),
        ProtectionMessage::PatternTail => text.literal("un carácter. Distingue mayúsculas de minúsculas."),
        ProtectionMessage::Protect => text.literal("Proteger las ramas coincidentes"),
        ProtectionMessage::BlockForcePush => text.literal("Bloquear los envíos forzados"),
        ProtectionMessage::BlockForcePushText => text.literal(
            "Los envíos solo pueden añadir commits; el historial no se puede reescribir.",
        ),
        ProtectionMessage::BlockDeletion => text.literal("Bloquear la eliminación"),
        ProtectionMessage::BlockDeletionText => text.literal("Las ramas coincidentes no se pueden eliminar."),
        ProtectionMessage::RequirePullRequest => text.literal("Exigir una solicitud de cambios"),
        ProtectionMessage::RequirePullRequestText => text.literal(
            "Los cambios solo llegan al fusionar una solicitud de cambios; se rechazan los envíos directos a una \
             rama existente.",
        ),
        ProtectionMessage::RequiredApprovals => text.literal("Aprobaciones obligatorias"),
        ProtectionMessage::DismissStale => text.literal("Descartar las aprobaciones cuando se envíen commits nuevos"),
        ProtectionMessage::BlockOnChanges => {
            text.literal("Bloquear la fusión mientras haya cambios solicitados");
        }
        ProtectionMessage::RestrictPushers => text.literal("Restringir quién puede enviar"),
        ProtectionMessage::RestrictPeople => text.literal(
            "Solo las personas de abajo pueden enviar o fusionar. Las claves de despliegue nunca pueden.",
        ),
        ProtectionMessage::RestrictPeopleTeams => text.literal(
            "Solo las personas y los equipos de abajo pueden enviar o fusionar. Las claves de despliegue nunca pueden.",
        ),
        ProtectionMessage::People => text.literal("Personas"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => text.literal(
            "Nombres de usuario separados por espacios o comas. Cada uno necesita acceso de escritura.",
        ),
        ProtectionMessage::Teams => text.literal("Equipos"),
        ProtectionMessage::NoTeams => text.literal("Esta organización aún no tiene equipos."),
        ProtectionMessage::RequireChecks => text.literal("Exigir que se superen las comprobaciones de estado"),
        ProtectionMessage::RequireChecksText => text.literal(
            "Las solicitudes de cambios solo se fusionan cuando el último estado de cada comprobación de abajo es \
             correcto en su commit de origen.",
        ),
        ProtectionMessage::RequiredChecks => text.literal("Comprobaciones obligatorias"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("Permitir que los administradores la omitan"),
        ProtectionMessage::AdminBypassText => text.literal(
            "Esta regla no se aplica a los administradores del repositorio. Cada omisión queda anotada en el \
             registro de auditoría.",
        ),
        ProtectionMessage::SaveRule => text.literal("Guardar regla"),
        ProtectionMessage::DangerZone => text.literal("Zona de peligro"),
        ProtectionMessage::DeleteTitle => text.literal("Eliminar esta regla"),
        ProtectionMessage::DeleteLead => text.literal("Las ramas que coinciden con"),
        ProtectionMessage::DeleteTail => text.literal("pierden su protección de inmediato."),
        ProtectionMessage::DeleteConfirm => text.literal("Entiendo que esta regla se eliminará"),
        ProtectionMessage::DeleteRule => text.literal("Eliminar regla"),
        ProtectionMessage::NoMatches => text.literal("Aún no coincide con ninguna rama"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("Este repositorio tiene el máximo de ");
            text.number(max);
            text.literal(" reglas. Elimina una para añadir otra.");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal(
                "Un contexto por línea, exactamente como lo informa tu CI (distingue mayúsculas de minúsculas), como \
                 máximo ",
            );
            text.number(max);
            text.literal(".");
        }
        ProtectionText::Matches(names) => {
            text.literal("Coincide con ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("Coincide con ");
            text.argument(names);
            text.literal(" y ");
            text.number(more);
            text.literal(" más");
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("Protección de ramas · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("Nueva regla de protección de ramas · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · Protección de ramas · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("Sin envíos forzados"),
        RuleBadge::NoDeletion => text.literal("Sin eliminación"),
        RuleBadge::PullRequest => text.literal("Solicitud de cambios"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aprobación",
                _ => " aprobaciones",
            });
        }
        RuleBadge::StatusChecks => text.literal("Comprobaciones de estado"),
        RuleBadge::Restricted => text.literal("Restringida"),
        RuleBadge::AdminsBypass => text.literal("Los administradores pueden omitirla"),
    }
    ProtectionSentence => RichWriter |text| {
        ProtectionSentence::Intro => {
            text.literal("Las reglas se aplican a cada envío a ");
            text.slot(0);
            text.literal(", por HTTP y SSH, también a los administradores salvo que una regla les permita omitirla.");
        }
    }
}
