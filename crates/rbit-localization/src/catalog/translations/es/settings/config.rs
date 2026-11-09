//! Spanish (`es`) entries for `catalog/settings/config.rs`. Section titles and help and
//! choice labels are here; field labels and help are in the `fields_*` modules.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares a module's `ConfigText` field entries as `write`, which returns `false` for a
/// key the module does not have.
macro_rules! config_arms {
    (ConfigText<'a> => MessageWriter |$text:ident| { $($arms:tt)* }) => {
        /// Writes this module's entry for `message`, or returns `false` when it has none.
        pub(super) fn write(message: ConfigText<'_>, $text: &mut MessageWriter) -> bool {
            #[allow(
                unreachable_patterns,
                clippy::match_same_arms,
                clippy::too_many_lines,
                reason = "a table lists only the messages translated so far"
            )]
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_1;
mod fields_2;

entries! { Es:
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::SectionTitle("updates") => text.literal("Actualizaciones de software y privacidad"),
        ConfigText::SectionHelp("updates") => text.literal(
            "Búsqueda independiente de versiones en segundo plano y controles de divulgación de la versión en \
             ejecución. Los cambios se aplican tras reiniciar.",
        ),
        ConfigText::SectionTitle("server") => text.literal("Servidor"),
        ConfigText::SectionHelp("server") => text.literal("Dirección pública, escucha y valores predeterminados de la instancia."),
        ConfigText::SectionTitle("storage") => text.literal("Almacenamiento"),
        ConfigText::SectionHelp("storage") => text.literal(
            "Consulta la carpeta de datos. Para reubicar el almacenamiento, hazlo sin conexión con la configuración de despliegue.",
        ),
        ConfigText::SectionTitle("security") => text.literal("Seguridad"),
        ConfigText::SectionHelp("security") => text.literal(
            "Sesiones, contraseñas, limitación de frecuencia y proxies inversos de confianza.",
        ),
        ConfigText::SectionTitle("repository") => text.literal("Repositorios"),
        ConfigText::SectionHelp("repository") => text.literal("Valores predeterminados y límites de los repositorios Git."),
        ConfigText::SectionTitle("mail") => text.literal("Envío de correo"),
        ConfigText::SectionHelp("mail") => text.literal("Configura la identidad del remitente, la conexión SMTP y los tiempos de entrega."),
        ConfigText::SectionTitle("ssh") => text.literal("SSH"),
        ConfigText::SectionHelp("ssh") => text.literal(
            "Servidor SSH de Git integrado, dirección pública de clonación y límites de conexión.",
        ),
        ConfigText::SectionTitle("auth") => text.literal("Proveedores de inicio de sesión"),
        ConfigText::SectionHelp("auth") => text.literal("Conecta proveedores de identidad OIDC y GitHub."),
        ConfigText::SectionTitle("branding") => text.literal("Marca predeterminada"),
        ConfigText::SectionHelp("branding") => text.literal(
            "Valores predeterminados del despliegue. Usa Marca y políticas para cambios inmediatos en toda la instancia.",
        ),
        ConfigText::SectionTitle("notifications") => text.literal("Notificaciones por correo"),
        ConfigText::SectionHelp("notifications") => text.literal(
            "Activa el correo de notificaciones y elige la política de suscripción predeterminada.",
        ),
        ConfigText::SectionTitle("operations") => text.literal("Operaciones"),
        ConfigText::SectionHelp("operations") => text.literal("Métricas, mantenimiento, trabajos en segundo plano y cachés."),
        ConfigText::SectionTitle("api") => text.literal("API REST"),
        ConfigText::SectionHelp("api") => text.literal("Disponibilidad de la API y límites de paginación."),
        ConfigText::SectionTitle("webhooks") => text.literal("Webhooks"),
        ConfigText::SectionHelp("webhooks") => text.literal("Configuración de entrega saliente, reintentos y conservación."),
        ConfigText::SectionTitle("releases") => text.literal("Versiones"),
        ConfigText::SectionHelp("releases") => text.literal("Tamaños, cantidades y tiempos de subida de los archivos de las versiones."),
        ConfigText::SectionTitle("mirrors") => text.literal("Réplicas de extracción"),
        ConfigText::SectionHelp("mirrors") => text.literal("Programación de obtenciones, tiempos de espera y acceso a la red."),
        ConfigText::SectionTitle("push_mirrors") => text.literal("Réplicas de envío"),
        ConfigText::SectionHelp("push_mirrors") => text.literal("Programación de envíos, tiempos de espera y acceso a la red."),
        ConfigText::SectionTitle("imports") => text.literal("Importaciones"),
        ConfigText::SectionHelp("imports") => text.literal("Importación de repositorios, concurrencia y asociación de autores."),
        ConfigText::SectionTitle("issues") => text.literal("Incidencias y bandeja de entrada"),
        ConfigText::SectionHelp("issues") => text.literal("Análisis de commits y conservación de las notificaciones de la aplicación."),
        ConfigText::SectionTitle("pulls") => text.literal("Solicitudes de cambios"),
        ConfigText::SectionHelp("pulls") => text.literal(
            "Límites de fusión, comprobaciones en segundo plano y direcciones de los autores.",
        ),
        ConfigText::SectionTitle("editor") => text.literal("Editor del navegador"),
        ConfigText::SectionHelp("editor") => text.literal("Límites de tamaño, cantidad y tiempo de las subidas desde el navegador."),
        ConfigText::SectionTitle("lfs") => text.literal("Git LFS"),
        ConfigText::SectionHelp("lfs") => text.literal("Almacenamiento de archivos grandes, tamaños de objeto y duración de los tokens."),
        ConfigText::SectionTitle("actions") => text.literal("Acciones"),
        ConfigText::SectionHelp("actions") => text.literal("CI/CD en agentes externos, conservación, límites y tiempos de espera."),
        ConfigText::Choice { section: "mail", key: "transport", value: "disabled" } => text.literal("Desactivado"),
        ConfigText::Choice { section: "mail", key: "transport", value: "log" } => text.literal("Registro de desarrollo"),
        ConfigText::Choice { section: "mail", key: "transport", value: "smtp" } => text.literal("Servidor SMTP"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "starttls" } => text.literal("STARTTLS (recomendado)"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "tls" } => text.literal("TLS implícito"),
        ConfigText::Choice { section: "mail", key: "smtp_security", value: "none" } => text.literal("Sin cifrado"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "off" } => text.literal("Desactivado"),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "participating" } => text.literal(
            "Conversaciones en las que participa el usuario",
        ),
        ConfigText::Choice { section: "notifications", key: "default_email", value: "all" } => text.literal(
            "Todas las conversaciones suscritas",
        ),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "none" } => text.literal(
            "Conservar los autores originales como texto",
        ),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email" } => text.literal("Asociar por correo verificado"),
        ConfigText::Choice { section: "imports", key: "author_mapping", value: "email_or_username" } => text.literal(
            "Asociar por correo verificado o nombre de usuario",
        ),
        ConfigText::Choice { section: "provider", key: "kind", value: "oidc" } => text.literal("OpenID Connect"),
        ConfigText::Choice { section: "provider", key: "kind", value: "github" } => text.literal("GitHub / GitHub Enterprise"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "true" } => text.literal(
            "Seguir la política de registro de la instancia",
        ),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "false" } => text.literal("Solo cuentas existentes"),
        ConfigText::Choice { section: "provider", key: "allow_registration", value: "always" } => text.literal(
            "Permitir cuentas aunque el registro esté cerrado",
        ),
        message => return fields_1::write(message, text) || fields_2::write(message, text),
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("Configuración avanzada"),
        ConfigMessage::Intro => text.literal(
            "Gestiona tu instancia autoalojada con una configuración organizada por funciones.",
        ),
        ConfigMessage::RestartServer => text.literal("Reiniciar el servidor"),
        ConfigMessage::Saved => text.literal(
            "Configuración guardada. Reinicia el servidor para aplicar los cambios de despliegue.",
        ),
        ConfigMessage::RestartRequired => text.literal("Requiere reiniciar"),
        ConfigMessage::EnvironmentWins => text.literal("Las variables de entorno tienen prioridad."),
        ConfigMessage::LivePolicies => text.literal("Marca y políticas: se aplican de inmediato"),
        ConfigMessage::SettingsSection => text.literal("Sección de configuración"),
        ConfigMessage::OpenSection => text.literal("Abrir sección"),
        ConfigMessage::Sections => text.literal("Secciones de configuración"),
        ConfigMessage::LiveOverrides => text.literal(
            "La marca y las políticas de registro en vivo guardadas sustituyen estos valores predeterminados del \
             despliegue.",
        ),
        ConfigMessage::IdentityProviders => text.literal("Proveedores de identidad"),
        ConfigMessage::ProvidersHint => text.literal(
            "Añade y quita primero las tarjetas de proveedores; después introduce las credenciales nuevas y guarda. \
             Los cambios de proveedores se quedan en este formulario hasta que guardes la sección.",
        ),
        ConfigMessage::CallbackLead => text.literal(
            "Registra una URL de retorno en cada proveedor: tu URL pública seguida de",
        ),
        ConfigMessage::CallbackTail => text.literal(
            ", sustituyendo PROVIDER_ID por el ID de abajo. Para OpenID Connect, define la URL del emisor; para \
             GitHub Enterprise, define la URL de Enterprise.",
        ),
        ConfigMessage::NoProviders => text.literal(
            "No hay proveedores externos de inicio de sesión configurados. El inicio de sesión local está disponible.",
        ),
        ConfigMessage::RemoveProvider => text.literal("Quitar proveedor"),
        ConfigMessage::AddProvider => text.literal("Añadir proveedor de identidad"),
        ConfigMessage::NewProvider => text.literal("Nuevo proveedor de inicio de sesión"),
        ConfigMessage::OnlySection => text.literal("Solo se actualizará esta sección."),
        ConfigMessage::Recovery => text.literal("Recuperación y valores predeterminados del despliegue"),
        ConfigMessage::RecoveryLead => text.literal(
            "Si la configuración guardada impide el arranque, define",
        ),
        ConfigMessage::RecoveryTail => text.literal(
            "en el entorno del servidor, reinicia y después corrige o elimina aquí las sustituciones.",
        ),
        ConfigMessage::RemovingOverrides => text.literal(
            "Al eliminar las sustituciones se restaura la configuración del archivo y del entorno en el próximo \
             reinicio. La marca y las políticas en vivo se conservan.",
        ),
        ConfigMessage::RemoveOverrides => text.literal("Eliminar todas las sustituciones del despliegue"),
        ConfigMessage::Optional => text.literal("(opcional)"),
        ConfigMessage::OnePerLine => text.literal("Una entrada por línea."),
        ConfigMessage::ProviderDefaults => text.literal("Usar los valores predeterminados del proveedor"),
        ConfigMessage::Configured => text.literal("Configurado: déjalo vacío para conservarlo"),
        ConfigMessage::NotConfigured => text.literal("Sin configurar"),
        ConfigMessage::SecretHint => text.literal(
            "Déjalo vacío para conservar la credencial actual. Las credenciales nuevas nunca se vuelven a mostrar \
             tras un error; vuelve a introducirlas al corregir el formulario. Protege la base de datos de la \
             instancia y sus copias de seguridad.",
        ),
        ConfigMessage::ClearSecret => text.literal("Borrar esta credencial"),
        ConfigMessage::BlankDefault => text.literal("Déjalo vacío para usar el valor predeterminado."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("Guardar la configuración de ");
            text.argument(section);
        }
    }
}
