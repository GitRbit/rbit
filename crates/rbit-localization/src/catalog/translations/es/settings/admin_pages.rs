//! Spanish (`es`) entries for `catalog/settings/admin_pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin_pages::{AdminPageMessage, AdminPageText, InstanceSettingsMessage};
use crate::plural::Plural;

entries! { Es:
    AdminPageMessage => MessageWriter |text| {
        AdminPageMessage::UsersIntro => text.literal("Todas las personas con cuenta en esta instancia."),
        AdminPageMessage::UsersSearch => text.literal("Buscar por nombre de usuario, nombre o correo electrónico"),
        AdminPageMessage::NewUser => text.literal("Nuevo usuario"),
        AdminPageMessage::NoUsers => text.literal("No se encontraron usuarios"),
        AdminPageMessage::NoUsersText => text.literal("Prueba otra búsqueda."),
        AdminPageMessage::User => text.literal("Usuario"),
        AdminPageMessage::Joined => text.literal("Se unió"),
        AdminPageMessage::Admin => text.literal("Administrador"),
        AdminPageMessage::Suspended => text.literal("Suspendido"),
        AdminPageMessage::EmailVerified => text.literal("Dirección de correo electrónico verificada"),
        AdminPageMessage::Verified => text.literal("Verificada"),
        AdminPageMessage::You => text.literal("Tú"),
        AdminPageMessage::RemoveAdmin => text.literal("Quitar como administrador"),
        AdminPageMessage::MakeAdmin => text.literal("Hacer administrador"),
        AdminPageMessage::LiftSuspension => text.literal("Levantar la suspensión"),
        AdminPageMessage::Suspend => text.literal("Suspender"),
        AdminPageMessage::SendReset => text.literal("Enviar restablecimiento de contraseña"),
        AdminPageMessage::SetPasswordMenu => text.literal("Definir contraseña…"),
        AdminPageMessage::DeleteMenu => text.literal("Eliminar…"),
        AdminPageMessage::ResetSent => text.literal("Se envió el correo de restablecimiento de contraseña."),
        AdminPageMessage::PasswordSet => text.literal("Contraseña actualizada."),
        AdminPageMessage::NewUserIntro => text.literal(
            "Los administradores pueden crear cuentas aunque el registro esté cerrado.",
        ),
        AdminPageMessage::Username => text.literal("Nombre de usuario"),
        AdminPageMessage::UsernameHint => text.literal("Letras, dígitos, «-», «_» y «.»"),
        AdminPageMessage::Email => text.literal("Correo electrónico"),
        AdminPageMessage::InitialPassword => text.literal("Contraseña inicial"),
        AdminPageMessage::SiteAdministrator => text.literal("Administrador del sitio"),
        AdminPageMessage::SiteAdministratorText => {
            text.literal("Acceso completo a todos los repositorios y a este panel.");
        }
        AdminPageMessage::CreateUser => text.literal("Crear usuario"),
        AdminPageMessage::RegistrationClosed => text.literal(
            "El registro público está cerrado; esta es la única forma de añadir usuarios.",
        ),
        AdminPageMessage::SetPassword => text.literal("Definir contraseña"),
        AdminPageMessage::ResetHint => text.literal(
            "Para que elija la suya, envíale un restablecimiento de contraseña desde la lista de usuarios.",
        ),
        AdminPageMessage::DeleteUser => text.literal("Eliminar usuario"),
        AdminPageMessage::DeleteUserLead => text.literal("Esto elimina para siempre a"),
        AdminPageMessage::OrganizationsIntro => text.literal("Espacios de nombres compartidos y sus miembros."),
        AdminPageMessage::SearchByName => text.literal("Buscar por nombre"),
        AdminPageMessage::NoOrganizations => text.literal("No se encontraron organizaciones"),
        AdminPageMessage::NoOrganizationsText => {
            text.literal("Las organizaciones aparecerán aquí cuando existan.");
        }
        AdminPageMessage::Organization => text.literal("Organización"),
        AdminPageMessage::Members => text.literal("Miembros"),
        AdminPageMessage::Created => text.literal("Creada"),
        AdminPageMessage::RepositoriesIntro => text.literal(
            "Todos los repositorios, públicos y privados, con la actividad más reciente primero.",
        ),
        AdminPageMessage::RepositoriesSearch => text.literal("Buscar por propietario/nombre o descripción"),
        AdminPageMessage::NoRepositories => text.literal("No se encontraron repositorios"),
        AdminPageMessage::NoRepositoriesText => text.literal("Los repositorios aparecerán aquí cuando existan."),
        AdminPageMessage::Repository => text.literal("Repositorio"),
        AdminPageMessage::Updated => text.literal("Actualizado"),
        AdminPageMessage::DeleteRepository => text.literal("Eliminar repositorio"),
        AdminPageMessage::DeleteRepositoryText => text.literal(
            "Esto elimina para siempre el repositorio, con todos sus datos de Git. No se puede deshacer.",
        ),
        AdminPageMessage::AuditIntro => text.literal(
            "Inicios de sesión, cambios en la autenticación en dos pasos, sesiones, tokens de acceso y acciones de \
             los administradores, de más reciente a más antiguo.",
        ),
        AdminPageMessage::AuditSearch => text.literal("Usuario, destino o dirección IP"),
        AdminPageMessage::Category => text.literal("Categoría"),
        AdminPageMessage::Filter => text.literal("Filtrar"),
        AdminPageMessage::AllEvents => text.literal("Todos los eventos"),
        AdminPageMessage::Clear => text.literal("Limpiar"),
        AdminPageMessage::NoEvents => text.literal("No hay eventos"),
        AdminPageMessage::NoEventsText => text.literal("Aún no hay nada que coincida con estos filtros."),
        AdminPageMessage::As => text.literal("como"),
        AdminPageMessage::By => text.literal("por"),
        AdminPageMessage::On => text.literal("en"),
    }
    InstanceSettingsMessage => MessageWriter |text| {
        InstanceSettingsMessage::Intro => text.literal("Los cambios se aplican de inmediato a toda la instancia."),
        InstanceSettingsMessage::Saved => text.literal("Configuración guardada."),
        InstanceSettingsMessage::SiteName => text.literal("Nombre del sitio"),
        InstanceSettingsMessage::SiteNameHint => text.literal(
            "Se muestra en la cabecera, en los títulos de las páginas y al instalarlo como aplicación.",
        ),
        InstanceSettingsMessage::Branding => text.literal("Marca e identidad"),
        InstanceSettingsMessage::BrandingHint => text.literal(
            "El texto es texto sin formato. Las imágenes y los enlaces aceptan URL HTTPS o /rutas locales servidas \
             por tu proxy inverso. Los campos de imagen vacíos usan los valores incluidos por defecto.",
        ),
        InstanceSettingsMessage::PrimaryColor => {
            text.literal("Color del botón principal (vacío para usar el del tema)");
        }
        InstanceSettingsMessage::PrimaryTextColor => text.literal("Color del texto del botón principal"),
        InstanceSettingsMessage::ColorExample => text.literal("#2563eb"),
        InstanceSettingsMessage::TextColorExample => text.literal("#ffffff"),
        InstanceSettingsMessage::ColorHint => text.literal(
            "Elige colores con suficiente contraste para que las etiquetas de los botones se lean bien. Define ambos \
             colores o deja ambos vacíos.",
        ),
        InstanceSettingsMessage::Description => text.literal("Descripción del sitio"),
        InstanceSettingsMessage::WelcomeTitle => {
            text.literal("Título de bienvenida (vacío para usar el nombre del sitio)");
        }
        InstanceSettingsMessage::WelcomeMessage => text.literal("Mensaje de bienvenida"),
        InstanceSettingsMessage::LogoUrl => text.literal("URL del logotipo"),
        InstanceSettingsMessage::FaviconUrl => text.literal("URL del favicon"),
        InstanceSettingsMessage::AppIconUrl => {
            text.literal("URL del icono de la aplicación (cuadrado, de al menos 512 píxeles)");
        }
        InstanceSettingsMessage::FooterText => text.literal("Texto del pie de página"),
        InstanceSettingsMessage::SupportUrl => text.literal("URL de ayuda"),
        InstanceSettingsMessage::TermsUrl => text.literal("URL de las condiciones"),
        InstanceSettingsMessage::PrivacyUrl => text.literal("URL de privacidad"),
        InstanceSettingsMessage::ShowPoweredBy => text.literal("Mostrar «Funciona con rbit»"),
        InstanceSettingsMessage::ShowVersion => text.literal("Mostrar la versión del software"),
        InstanceSettingsMessage::ShowFeatures => {
            text.literal("Mostrar las funciones de autoalojamiento en la página de bienvenida");
        }
        InstanceSettingsMessage::Explore => text.literal("Página Explorar"),
        InstanceSettingsMessage::ExploreEnabled => text.literal("Activada"),
        InstanceSettingsMessage::ExploreEnabledText => text.literal("Cualquiera que pueda navegar por el sitio puede buscar y listar todos los repositorios públicos."),
        InstanceSettingsMessage::ExploreDisabled => text.literal("Desactivada"),
        InstanceSettingsMessage::ExploreDisabledText => text.literal("La página Explorar no se encuentra y sus enlaces se ocultan, también para los administradores. Los repositorios públicos siguen siendo públicos."),
        InstanceSettingsMessage::Browsing => text.literal("Quién puede navegar"),
        InstanceSettingsMessage::BrowsingOpen => text.literal("Cualquiera"),
        InstanceSettingsMessage::BrowsingOpenText => text.literal("Los visitantes pueden leer repositorios, perfiles y organizaciones públicos sin una cuenta."),
        InstanceSettingsMessage::BrowsingSignedIn => text.literal("Solo usuarios con sesión iniciada"),
        InstanceSettingsMessage::BrowsingSignedInText => text.literal("Los visitantes se envían a la página de inicio de sesión. Git, LFS y la API rechazan las lecturas sin credenciales."),
        InstanceSettingsMessage::Registration => text.literal("Registro"),
        InstanceSettingsMessage::Open => text.literal("Abierto"),
        InstanceSettingsMessage::OpenText => {
            text.literal("Cualquiera puede crear una cuenta desde la página de registro.");
        }
        InstanceSettingsMessage::Closed => text.literal("Cerrado"),
        InstanceSettingsMessage::ClosedText => text.literal("Solo los administradores pueden crear cuentas."),
        InstanceSettingsMessage::OrgCreation => text.literal("Quién puede crear organizaciones"),
        InstanceSettingsMessage::AnyUser => text.literal("Cualquier usuario"),
        InstanceSettingsMessage::AnyUserText => {
            text.literal("Cualquier usuario con sesión iniciada puede crear organizaciones.");
        }
        InstanceSettingsMessage::AdminsOnly => text.literal("Solo administradores"),
        InstanceSettingsMessage::AdminsOnlyText => {
            text.literal("Se puede seguir añadiendo usuarios a las organizaciones existentes.");
        }
        InstanceSettingsMessage::DefaultVisibility => text.literal("Visibilidad predeterminada de los repositorios"),
        InstanceSettingsMessage::Public => text.literal("Público"),
        InstanceSettingsMessage::PublicText => text.literal(
            "Los repositorios nuevos son visibles para todos salvo que se cambie.",
        ),
        InstanceSettingsMessage::Private => text.literal("Privado"),
        InstanceSettingsMessage::PrivateText => text.literal(
            "Los repositorios nuevos empiezan siendo privados; sus propietarios pueden hacerlos públicos.",
        ),
        InstanceSettingsMessage::VisibilityHint => {
            text.literal("Viene preseleccionada en el formulario de nuevo repositorio.");
        }
        InstanceSettingsMessage::DigestTime => text.literal("Hora de los resúmenes por correo"),
        InstanceSettingsMessage::DigestHint => text.literal(
            "Cuándo se envían los resúmenes diarios y semanales de notificaciones a los usuarios que no han elegido \
             una hora. Los resúmenes semanales se envían los lunes.",
        ),
        InstanceSettingsMessage::Save => text.literal("Guardar configuración"),
    }
    AdminPageText<'a> => MessageWriter |text| {
        AdminPageText::ShareSecurely(length) => {
            text.literal("Al menos ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " carácter.",
                _ => " caracteres.",
            });
            text.literal(
                " Compártela con el usuario de forma segura. Deberá reemplazarla la primera vez que inicie sesión.",
            );
        }
        AdminPageText::ReplacesPassword(name) => {
            text.literal(
                "La contraseña nueva reemplaza de inmediato a la actual y los enlaces de restablecimiento de \
                 contraseña dejan de funcionar. Compártela con ",
            );
            text.argument(name);
            text.literal(" de forma segura.");
        }
        AdminPageText::DeleteUserTail(repositories) => {
            text.literal(", sus repositorios (");
            text.argument(repositories);
            text.literal(
                ", con todos sus datos de Git), sus tokens de acceso y sus membresías en organizaciones. No se puede \
                 deshacer.",
            );
        }
    }
}
