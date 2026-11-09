//! Spanish (`es`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};
use crate::plural::Plural;

entries! { Es:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("Buscar repositorios públicos"),
        HomeMessage::Search => text.literal("Buscar"),
        HomeMessage::ExploreAll => text.literal("Explorar todos los repositorios"),
        HomeMessage::ThisInstance => text.literal("Esta instancia"),
        HomeMessage::RepositoryCount(count) => text.literal(match text.plural(count.unsigned_abs()) {
            Plural::One => "Repositorio",
            _ => "Repositorios",
        }),
        HomeMessage::UserCount(count) => text.literal(match text.plural(count.unsigned_abs()) {
            Plural::One => "Usuario",
            _ => "Usuarios",
        }),
        HomeMessage::OrganizationCount(count) => text.literal(match text.plural(count.unsigned_abs()) {
            Plural::One => "Organización",
            _ => "Organizaciones",
        }),
        HomeMessage::RecentlyUpdated => text.literal("Actualizados recientemente"),
        HomeMessage::ViewAll => text.literal("Ver todos"),
        HomeMessage::NoPublicRepositories => text.literal("Aún no hay repositorios públicos"),
        HomeMessage::NoPublicRepositoriesText => {
            text.literal("Los repositorios que se hagan públicos en este servidor aparecerán aquí.");
        }
        HomeMessage::FreshServer => text.literal(
            "Este servidor aún no tiene cuentas. La primera cuenta que se registre será su administradora.",
        ),
        HomeMessage::BuiltForSelfHosting => text.literal("Pensado para alojarlo tú mismo"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "Un único binario rápido",
            Feature::Protocols => "Git por HTTPS y SSH",
            Feature::Teams => "Organizaciones y equipos",
            Feature::Releases => "Versiones, réplicas y webhooks",
            Feature::Security => "Seguro por defecto",
            Feature::Screens => "En cualquier pantalla",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite y un almacén integrado. Ningún otro servicio que instalar o ejecutar.",
            Feature::Protocols => "Smart HTTP con el protocolo v2, un servidor SSH integrado y claves de despliegue.",
            Feature::Teams => "Comparte repositorios con equipos y colaboradores, rol a rol.",
            Feature::Releases => "Publica versiones, replica proyectos de origen y avisa a otros servicios.",
            Feature::Security => {
                "Inicio de sesión en dos pasos, tokens de acceso con alcance limitado y un registro de auditoría."
            }
            Feature::Screens => "Adaptable hasta el tamaño de un teléfono e instalable como aplicación.",
        }),
        HomeMessage::Dashboard => text.literal("Panel"),
        HomeMessage::ViewProfile => text.literal("Ver perfil"),
        HomeMessage::New => text.literal("Nuevo"),
        HomeMessage::NewRepository => text.literal("Nuevo repositorio"),
        HomeMessage::PersonalDashboards => text.literal("Paneles personales"),
        HomeMessage::Issues => text.literal("Incidencias"),
        HomeMessage::PullRequests => text.literal("Solicitudes de cambios"),
        HomeMessage::Repositories => text.literal("Repositorios"),
        HomeMessage::PrivateCount(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " privado",
                _ => " privados",
            });
        }
        HomeMessage::FindRepository => text.literal("Buscar un repositorio…"),
        HomeMessage::FilterRepositories => text.literal("Filtrar tus repositorios"),
        HomeMessage::NoRepositories => text.literal("Aún no hay repositorios"),
        HomeMessage::NoRepositoriesText => {
            text.literal("Crea un repositorio para empezar a alojar código en este servidor.");
        }
        HomeMessage::CreateRepository => text.literal("Crear repositorio"),
        HomeMessage::NoFilterMatch => text.literal("Ningún repositorio coincide con tu filtro."),
        HomeMessage::Organizations => text.literal("Organizaciones"),
        HomeMessage::NewOrganization => text.literal("Nueva organización"),
        HomeMessage::NotAMember => text.literal("No eres miembro de ninguna organización."),
        HomeMessage::CreateOne => text.literal("Crea una"),
        HomeMessage::ShareWithTeam => text.literal("para compartir repositorios con un equipo."),
        HomeMessage::Explore => text.literal("Explorar"),
        HomeMessage::NothingPublic => text.literal("Aún no hay nada público."),
        HomeMessage::YourInvolvement => text.literal("Tu participación"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "Creadas por mí",
            Involvement::Assigned => "Asignadas",
            Involvement::Mentioned => "Menciones",
            Involvement::ReviewRequested => "Revisión solicitada",
        }),
        HomeMessage::SearchIn(title) => {
            text.literal("Buscar en ");
            text.argument(title);
        }
        HomeMessage::SearchHelp => text.literal("Ayuda de búsqueda"),
        HomeMessage::SearchHelpText => text.literal(
            "Usa is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee o \
             sort:updated-desc. El texto busca en títulos y descripciones. Los resultados solo incluyen repositorios \
             que puedes leer ahora. Menciones incluye las menciones conocidas en descripciones, comentarios y \
             revisiones enviadas, aunque ya hayas leído las notificaciones.",
        ),
        HomeMessage::Results => text.literal("Resultados"),
        HomeMessage::OpenCount(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " abierta",
                _ => " abiertas",
            });
        }
        HomeMessage::ClosedCount(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " cerrada",
                _ => " cerradas",
            });
        }
        HomeMessage::NoMatchingThreads => text.literal("No hay conversaciones coincidentes"),
        HomeMessage::NoMatchingThreadsText => text.literal("Prueba otro filtro u otra pestaña de participación."),
        HomeMessage::CommentCount(count) => {
            let count = u64::from(count);
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentario",
                _ => " comentarios",
            });
        }
    }
}
