//! Spanish (`es`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::TeamSentence;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};
use crate::plural::Plural;

entries! { Es:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("Nuevo equipo"),
        TeamMessage::NoTeams => text.literal("Aún no hay equipos"),
        TeamMessage::NoTeamsText => text.literal(
            "Los equipos agrupan a los miembros y dan a cada grupo su propio acceso a los repositorios.",
        ),
        TeamMessage::OneMember => text.literal("1 miembro"),
        TeamMessage::OneRepository => text.literal("1 repositorio"),
        TeamMessage::CreateTeam => text.literal("Crear equipo"),
        TeamMessage::TeamSettings => text.literal("Configuración del equipo"),
        TeamMessage::RenameHint => text.literal("Cambiar el nombre cambia la dirección del equipo."),
        TeamMessage::SaveChanges => text.literal("Guardar cambios"),
        TeamMessage::DeleteTeam => text.literal("Eliminar este equipo"),
        TeamMessage::DeleteTeamText => text.literal(
            "Sus miembros siguen en la organización, pero pierden el acceso que les daba este equipo.",
        ),
        TeamMessage::DeleteTeamButton => text.literal("Eliminar equipo"),
        TeamMessage::UsernamePlaceholder => text.literal("Nombre de usuario"),
        TeamMessage::OnlyMembers => {
            text.literal("Solo los miembros de la organización pueden unirse a sus equipos.");
        }
        TeamMessage::NoMembers => text.literal("No hay miembros"),
        TeamMessage::NoMembersText => text.literal("Los miembros obtienen el acceso de este equipo a sus repositorios."),
        TeamMessage::AddRepository => text.literal("Añadir un repositorio"),
        TeamMessage::RepositoryPlaceholder => text.literal("Nombre del repositorio"),
        TeamMessage::Permission => text.literal("Permiso"),
        TeamMessage::RepositoryHint => text.literal(
            "Cualquier repositorio de la organización. Los miembros del equipo obtienen este permiso en él.",
        ),
        TeamMessage::NoRepositories => text.literal("No hay repositorios"),
        TeamMessage::NoRepositoriesText => text.literal(
            "Este equipo no tiene acceso a ningún repositorio que puedas ver.",
        ),
        TeamMessage::Private => text.literal("Privado"),
        TeamMessage::Save => text.literal("Guardar"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " miembro",
                _ => " miembros",
            });
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositorio",
                _ => " repositorios",
            });
        }
        TeamText::PermissionOn(repository) => {
            text.literal("Permiso en ");
            text.argument(repository);
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("Guardar el permiso en ");
            text.argument(repository);
        }
        TeamText::TeamsTitle(org) => {
            text.literal("Equipos · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("Nuevo equipo · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("Configuración · ");
            text.argument(team);
        }
    }
    TeamSentence => RichWriter |text| {
        TeamSentence::NameHint => {
            text.literal("Letras, dígitos, ");
            text.slot(0);
            text.literal(", ");
            text.slot(1);
            text.literal(" y ");
            text.slot(2);
            text.literal("; se usa en la dirección del equipo.");
        }
    }
}
