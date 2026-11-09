//! Spanish (`es`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};
use crate::plural::Plural;

entries! { Es:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("Administración del sitio"),
        AdminMessage::Administration => text.literal("Administración"),
        AdminMessage::Dashboard => text.literal("Panel"),
        AdminMessage::Users => text.literal("Usuarios"),
        AdminMessage::Organizations => text.literal("Organizaciones"),
        AdminMessage::Repositories => text.literal("Repositorios"),
        AdminMessage::Settings => text.literal("Configuración"),
        AdminMessage::AuditLog => text.literal("Registro de auditoría"),
        AdminMessage::Configuration => text.literal("Configuración avanzada"),
        AdminMessage::Runners => text.literal("Agentes"),
        AdminMessage::Search => text.literal("Buscar"),
        AdminMessage::Pagination => text.literal("Paginación"),
        AdminMessage::Previous => text.literal("Anterior"),
        AdminMessage::Next => text.literal("Siguiente"),
        AdminMessage::NoResults => text.literal("Sin resultados"),
        AdminMessage::Cancel => text.literal("Cancelar"),
        AdminMessage::Actions => text.literal("Acciones"),
        AdminMessage::Private => text.literal("Privado"),
        AdminMessage::Archived => text.literal("Archivado"),
        AdminMessage::Delete => text.literal("Eliminar"),
        AdminMessage::TypeToConfirmLead => text.literal("Escribe"),
        AdminMessage::TypeToConfirmTail => text.literal("para confirmar"),
        AdminMessage::NewPassword => text.literal("Contraseña nueva"),
        AdminMessage::ConfirmPassword => text.literal("Confirmar contraseña nueva"),
        AdminMessage::Unavailable => text.literal("no disponible"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("Un resumen de esta instancia."),
        DashboardMessage::RestartServer => text.literal("Reiniciar el servidor"),
        DashboardMessage::RestartUnavailableHint => text.literal("El reinicio no está disponible en este host"),
        DashboardMessage::SoftwareUpdates => text.literal("Actualizaciones de software"),
        DashboardMessage::ConfigureUpdates => {
            text.literal("Configurar la búsqueda de actualizaciones y la privacidad de la versión");
        }
        DashboardMessage::LiveMonitoring => text.literal("Supervisión en directo"),
        DashboardMessage::MonitoringIntro => text.literal(
            "Tráfico, tiempos de respuesta y actividad de Git de los últimos cinco minutos.",
        ),
        DashboardMessage::Snapshot => text.literal("Instantánea"),
        DashboardMessage::Pause => text.literal("Pausar"),
        DashboardMessage::NeedsJavaScript => text.literal(
            "Las actualizaciones en directo necesitan JavaScript. Recarga esta página para obtener una instantánea \
             nueva.",
        ),
        DashboardMessage::System => text.literal("Sistema"),
        DashboardMessage::InstanceSettings => text.literal("Configuración de la instancia"),
        DashboardMessage::Edit => text.literal("Editar"),
        DashboardMessage::Users => text.literal("Usuarios"),
        DashboardMessage::Organizations => text.literal("Organizaciones"),
        DashboardMessage::SharedNamespaces => text.literal("Espacios de nombres compartidos"),
        DashboardMessage::Repositories => text.literal("Repositorios"),
        DashboardMessage::AccessTokens => text.literal("Tokens de acceso"),
        DashboardMessage::AcrossAllUsers => text.literal("Entre todos los usuarios"),
        DashboardMessage::Uptime => text.literal("Tiempo de actividad"),
        DashboardMessage::DataDirectory => text.literal("Carpeta de datos"),
        DashboardMessage::DiskUsage => text.literal("Uso del disco"),
        DashboardMessage::SiteName => text.literal("Nombre del sitio"),
        DashboardMessage::Registration => text.literal("Registro"),
        DashboardMessage::RegistrationOpen => text.literal("Abierto a todos"),
        DashboardMessage::RegistrationClosed => text.literal("Cerrado (los administradores crean las cuentas)"),
        DashboardMessage::OrgPolicy => text.literal("Organizaciones"),
        DashboardMessage::AnyUserCreates => text.literal("Cualquier usuario puede crearlas"),
        DashboardMessage::AdminsCreate => text.literal("Solo los administradores pueden crearlas"),
        DashboardMessage::NewRepositories => text.literal("Repositorios nuevos"),
        DashboardMessage::PublicByDefault => text.literal("Públicos por defecto"),
        DashboardMessage::PrivateByDefault => text.literal("Privados por defecto"),
        DashboardMessage::EmailDigests => text.literal("Resúmenes por correo"),
        DashboardMessage::Restarting => text.literal("Reiniciando el servidor"),
        DashboardMessage::RestartRequested => text.literal("Reinicio solicitado"),
        DashboardMessage::RestartingText => text.literal(
            "El servidor está terminando las solicitudes HTTP en curso, deteniendo SSH y los trabajos en segundo \
             plano y cerrando el almacenamiento antes de reiniciarse. Las transferencias largas pueden tardar en \
             terminar.",
        ),
        DashboardMessage::Waiting => text.literal("Esperando a que vuelva el servidor…"),
        DashboardMessage::ReturnToDashboard => text.literal("Volver al panel"),
        DashboardMessage::WaitNoScript => text.literal(
            "Espera un momento y vuelve al panel para reconectarte.",
        ),
        DashboardMessage::RestartQuestion => text.literal("¿Reiniciar esta aplicación rbit?"),
        DashboardMessage::RestartText => text.literal(
            "El sitio no estará disponible durante unos instantes mientras la aplicación se detiene y vuelve a \
             arrancar con su configuración guardada. Las solicitudes HTTP pueden terminar; las conexiones SSH se \
             cerrarán. Los usuarios conservarán sus sesiones guardadas.",
        ),
        DashboardMessage::RestartAudited => {
            text.literal("Esta acción queda anotada en el registro de auditoría de la administración.");
        }
        DashboardMessage::RestartUnavailable => text.literal("El reinicio no está disponible"),
        DashboardMessage::RestartUnsupported => text.literal(
            "Este host no admite reiniciar la aplicación. Reinicia rbit con tu gestor de servicios.",
        ),
        DashboardMessage::BackToDashboard => text.literal("Volver al panel"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("Solicitudes en el intervalo"),
        MonitoringMessage::ErrorRate => text.literal("Tasa de errores del servidor"),
        MonitoringMessage::ProcessUptime => text.literal("Tiempo de actividad del proceso"),
        MonitoringMessage::Samples => text.literal("Muestras de 5 segundos"),
        MonitoringMessage::FiveMinutesAgo => text.literal("Hace 5 minutos"),
        MonitoringMessage::LatestSample => text.literal("Última muestra"),
        MonitoringMessage::Collecting => text.literal(
            "Tomando la primera muestra. Los gráficos empiezan al cabo de cinco segundos.",
        ),
        MonitoringMessage::HttpResponses => text.literal("Respuestas HTTP"),
        MonitoringMessage::CurrentWindow => text.literal("Intervalo actual"),
        MonitoringMessage::Status => text.literal("Estado"),
        MonitoringMessage::RequestsColumn => text.literal("Solicitudes"),
        MonitoringMessage::Share => text.literal("Proporción"),
        MonitoringMessage::GitActivity => text.literal("Actividad de Git"),
        MonitoringMessage::Operation => text.literal("Operación"),
        MonitoringMessage::Exchanges => text.literal("Intercambios"),
        MonitoringMessage::CloneFetch => text.literal("Clonar / obtener"),
        MonitoringMessage::Push => text.literal("Enviar"),
        MonitoringMessage::GitNote => text.literal(
            "Intercambios de Smart HTTP, incluidos los fallidos. Se excluyen SSH y los anuncios de referencias. El \
             tiempo de respuesta mide el tiempo hasta las cabeceras; las transferencias en streaming pueden \
             continuar.",
        ),
        MonitoringMessage::Footnote => text.literal(
            "Solo este proceso; el historial se reinicia al reiniciar. Se excluyen las solicitudes de supervisión.",
        ),
        MonitoringMessage::Informational => text.literal("Informativas"),
        MonitoringMessage::Successful => text.literal("Correctas"),
        MonitoringMessage::Redirects => text.literal("Redirecciones"),
        MonitoringMessage::ClientErrors => text.literal("Errores del cliente"),
        MonitoringMessage::ServerErrors => text.literal("Errores del servidor"),
        MonitoringMessage::RequestTraffic => text.literal("Tráfico de solicitudes"),
        MonitoringMessage::ResponseTime => text.literal("Tiempo de respuesta"),
        MonitoringMessage::RequestsPerSecond => text.literal("sol./s"),
        MonitoringMessage::MillisecondsAverage => text.literal("ms de media"),
        MonitoringMessage::WaitingFirst => text.literal("Esperando la primera muestra"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("Ver las notas de la versión ");
            text.argument(version);
            text.literal(" de rbit");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.argument(admins);
            text.literal(" administradores · ");
            text.argument(suspended);
            text.literal(" suspendidos");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("1 administrador · ");
            text.argument(suspended);
            text.literal(" suspendidos");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.argument(public);
            text.literal(" públicos · ");
            text.argument(private);
            text.literal(" privados");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" de ");
            text.argument(total);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("Se envían a las ");
            text.argument(hour);
            text.literal(" salvo que los usuarios elijan otra hora");
        }
        AdminText::DeleteLabel(name) => {
            text.literal("Eliminar ");
            text.argument(name);
        }
        AdminText::ActionsFor(name) => {
            text.literal("Acciones para ");
            text.argument(name);
        }
        AdminText::PasswordLength(length) => {
            text.literal("Al menos ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " carácter.",
                _ => " caracteres.",
            });
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.argument(title);
            text.literal(" en los últimos cinco minutos. Último valor: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". Sin solicitudes, el tiempo de respuesta queda en blanco.");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("Actualizado a las ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" de ");
            text.number(limit);
            text.literal(" muestras");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · Administración");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("Definir la contraseña de ");
            text.argument(name);
            text.literal(" · Administración");
        }
        AdminText::DeletePageTitle(name) => {
            text.literal("Eliminar ");
            text.argument(name);
            text.literal(" · Administración");
        }
    }
}
