//! Spanish (`es`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};
use crate::plural::Plural;

entries! { Es:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("Elige un permiso."),
        OrgMessage::Organization => text.literal("Organización"),
        OrgMessage::Repositories => text.literal("Repositorios"),
        OrgMessage::Members => text.literal("Miembros"),
        OrgMessage::Teams => text.literal("Equipos"),
        OrgMessage::Settings => text.literal("Configuración"),
        OrgMessage::NewOrganization => text.literal("Nueva organización"),
        OrgMessage::NewIntro => text.literal(
            "Una organización posee repositorios en nombre de un equipo. Serás su primer propietario.",
        ),
        OrgMessage::Name => text.literal("Nombre"),
        OrgMessage::NameHint => text.literal(
            "Se usa en las URL. Letras, dígitos, «-», «_» y «.». Los usuarios y las organizaciones comparten un \
             mismo espacio de nombres.",
        ),
        OrgMessage::DisplayName => text.literal("Nombre visible"),
        OrgMessage::Optional => text.literal("opcional"),
        OrgMessage::Description => text.literal("Descripción"),
        OrgMessage::Cancel => text.literal("Cancelar"),
        OrgMessage::CreateOrganization => text.literal("Crear organización"),
        OrgMessage::New => text.literal("Nuevo"),
        OrgMessage::NoRepositories => text.literal("Aún no hay repositorios"),
        OrgMessage::NoRepositoriesText => {
            text.literal("Aquí se mostrarán los repositorios de esta organización.");
        }
        OrgMessage::Profile => text.literal("Perfil"),
        OrgMessage::Website => text.literal("Sitio web"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("Guardar perfil"),
        OrgMessage::MemberPermission => text.literal("Permiso de los miembros"),
        OrgMessage::PermissionLegend => text.literal("Lo que obtiene cada miembro en cada repositorio"),
        OrgMessage::PermissionHint => text.literal(
            "Los equipos y los colaboradores pueden ampliarlo, nunca reducirlo. Los propietarios siempre administran \
             todos los repositorios.",
        ),
        OrgMessage::SavePermission => text.literal("Guardar permiso"),
        OrgMessage::NoPermission => text.literal("Sin permiso"),
        OrgMessage::NoPermissionText => text.literal(
            "Los miembros solo ven los repositorios públicos y aquellos a los que tienen acceso sus equipos.",
        ),
        OrgMessage::Read => text.literal("Lectura"),
        OrgMessage::ReadText => text.literal("Los miembros pueden explorar y clonar todos los repositorios."),
        OrgMessage::Write => text.literal("Escritura"),
        OrgMessage::WriteText => {
            text.literal("Los miembros pueden explorar, clonar y enviar a todos los repositorios.");
        }
        OrgMessage::DangerZone => text.literal("Zona de peligro"),
        OrgMessage::DeleteOrganization => text.literal("Eliminar esta organización"),
        OrgMessage::DeleteText => text.literal(
            "Esto elimina para siempre la organización y todas sus membresías. No se puede deshacer.",
        ),
        OrgMessage::TypeToConfirmLead => text.literal("Escribe"),
        OrgMessage::TypeToConfirmTail => text.literal("para confirmar"),
        OrgMessage::DeleteOrganizationButton => text.literal("Eliminar organización"),
        OrgMessage::AddMember => text.literal("Añadir un miembro"),
        OrgMessage::Username => text.literal("Nombre de usuario"),
        OrgMessage::Role => text.literal("Rol"),
        OrgMessage::Member => text.literal("Miembro"),
        OrgMessage::Owner => text.literal("Propietario"),
        OrgMessage::Add => text.literal("Añadir"),
        OrgMessage::MembersHint => text.literal(
            "Los miembros obtienen el permiso base de la organización en todos los repositorios, más lo que les \
             concedan sus equipos. Los propietarios también pueden gestionar miembros, equipos y configuración.",
        ),
        OrgMessage::You => text.literal("(tú)"),
        OrgMessage::Leave => text.literal("Abandonar"),
        OrgMessage::Remove => text.literal("Quitar"),
        OrgMessage::MakeOwner => text.literal("Hacer propietario"),
        OrgMessage::MakeMember => text.literal("Hacer miembro"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" todavía posee 1 repositorio. Elimínalo o transfiérelo primero.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" todavía posee ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositorio. Elimínalo o transfiérelo primero.",
                _ => " repositorios. Elimínalos o transfiérelos primero.",
            });
        }
        OrgText::RemoveLabel(name) => {
            text.literal("Quitar a ");
            text.argument(name);
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal(": ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("Miembros · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("Configuración · ");
            text.argument(org);
        }
    }
}
