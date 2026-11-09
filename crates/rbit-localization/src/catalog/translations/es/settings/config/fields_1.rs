//! Spanish (`es`) entries for the configuration fields of `catalog/settings/config/`, part 1.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

config_arms! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "updates", key: "check_for_updates" } => text.literal("Buscar actualizaciones de software"),
        ConfigText::Help { section: "updates", key: "check_for_updates" } => text.literal(
            "Consulta el canal público de versiones de GitHub como mucho una vez al día tras una consulta correcta. No \
             se envía la versión en ejecución ni la identidad de la instancia. Requiere reiniciar.",
        ),
        ConfigText::Label { section: "updates", key: "report_version" } => text.literal("Informar de la versión en ejecución"),
        ConfigText::Help { section: "updates", key: "report_version" } => text.literal(
            "Muestra la versión del software en el pie de página público, la API de versión, el banner SSH y los \
             User-Agent de las integraciones salientes. Desactívalo para ocultarla; los administradores conservan los \
             diagnósticos locales. Requiere reiniciar.",
        ),
        ConfigText::Label { section: "server", key: "bind" } => text.literal("Dirección de escucha"),
        ConfigText::Help { section: "server", key: "bind" } => text.literal("Dirección en la que escucha el servidor HTTP."),
        ConfigText::Label { section: "server", key: "public_url" } => text.literal("URL pública"),
        ConfigText::Help { section: "server", key: "public_url" } => text.literal(
            "URL base pública, usada en las URL de clonación y en la comprobación de origen CSRF.",
        ),
        ConfigText::Label { section: "server", key: "site_name" } => text.literal("Nombre predeterminado del sitio"),
        ConfigText::Help { section: "server", key: "site_name" } => text.literal(
            "Nombre predeterminado mientras no lo sustituya la marca en vivo. Cambia el nombre activo en Marca y políticas.",
        ),
        ConfigText::Label { section: "server", key: "open_registration" } => text.literal("Registro abierto por defecto"),
        ConfigText::Help { section: "server", key: "open_registration" } => text.literal(
            "Política de registro predeterminada. Prevalece la política en vivo de Marca y políticas.",
        ),
        ConfigText::Label { section: "storage", key: "data_dir" } => text.literal("Carpeta de datos"),
        ConfigText::Help { section: "storage", key: "data_dir" } => text.literal(
            "Aquí es de solo lectura. Detén el servidor, mueve los datos y actualiza el archivo de despliegue o el entorno.",
        ),
        ConfigText::Label { section: "storage", key: "secret_key_file" } => text.literal("Archivo de clave secreta"),
        ConfigText::Help { section: "storage", key: "secret_key_file" } => text.literal(
            "Aquí es de solo lectura. La clave que cifra las credenciales guardadas; vacío significa secret.key en la \
             carpeta de datos. Haz una copia de seguridad junto con la base de datos.",
        ),
        ConfigText::Label { section: "security", key: "session_lifetime_days" } => text.literal("Duración de la sesión (días)"),
        ConfigText::Help { section: "security", key: "session_lifetime_days" } => text.literal(
            "Cuánto tiempo sigue siendo válida una sesión sin actividad, en días.",
        ),
        ConfigText::Label { section: "security", key: "session_max_age_days" } => text.literal("Antigüedad máxima de la sesión (días)"),
        ConfigText::Help { section: "security", key: "session_max_age_days" } => text.literal(
            "Cuánto tiempo puede ser válida una sesión como máximo, por activa que esté, en días. 0 significa sin límite.",
        ),
        ConfigText::Label { section: "security", key: "min_password_length" } => text.literal("Longitud mínima de la contraseña"),
        ConfigText::Help { section: "security", key: "min_password_length" } => text.literal(
            "Longitud mínima de contraseña aceptada al registrarse.",
        ),
        ConfigText::Label { section: "security", key: "max_password_jobs" } => text.literal("Máximo de tareas de contraseña"),
        ConfigText::Help { section: "security", key: "max_password_jobs" } => text.literal(
            "Tareas simultáneas de cálculo o verificación de hashes dentro del presupuesto compartido de trabajo de \
             contraseñas (1–64; 2 por defecto). Requiere reiniciar.",
        ),
        ConfigText::Label { section: "security", key: "trusted_proxies" } => text.literal("Redes de proxies de confianza"),
        ConfigText::Help { section: "security", key: "trusted_proxies" } => text.literal(
            "Proxies inversos (redes CIDR, p. ej., 127.0.0.1/32, 10.0.0.0/8, ::1/128) cuyas cabeceras X-Forwarded-For, \
             Forwarded y X-Forwarded-Proto se aceptan. Vacío (el valor predeterminado) significa que el par TCP es \
             siempre el cliente.",
        ),
        ConfigText::Label { section: "security", key: "client_ip_header" } => text.literal("Cabecera de la dirección del cliente"),
        ConfigText::Help { section: "security", key: "client_ip_header" } => text.literal("Una cabecera en la que un proxy de confianza indica la dirección del cliente, como CF-Connecting-IP detrás de Cloudflare o Cloudflare Tunnel. Solo se acepta de las redes de proxies de confianza anteriores, antes que X-Forwarded-For. Vacío usa solo X-Forwarded-For."),
        ConfigText::Label { section: "security", key: "rate_limit" } => text.literal("Activar la limitación de frecuencia"),
        ConfigText::Help { section: "security", key: "rate_limit" } => text.literal(
            "Limita por cliente los inicios de sesión, los registros, los códigos de dos pasos, las credenciales de Git \
             y los envíos de formularios. Solo conviene desactivarla para pruebas de carga.",
        ),
        ConfigText::Label { section: "security", key: "hsts" } => text.literal("Enviar HSTS"),
        ConfigText::Help { section: "security", key: "hsts" } => text.literal(
            "Envía Strict-Transport-Security (un año) cuando la URL pública es https://, para que los navegadores \
             rechacen HTTP sin cifrar hacia este host. Desactívalo solo mientras abandonas HTTPS.",
        ),
        ConfigText::Label { section: "security", key: "hsts_include_subdomains" } => text.literal("HSTS incluye los subdominios"),
        ConfigText::Help { section: "security", key: "hsts_include_subdomains" } => text.literal(
            "Añade includeSubDomains a esa cabecera, de modo que todos los subdominios del host también queden ligados a HTTPS.",
        ),
        ConfigText::Label { section: "security", key: "external_images" } => text.literal("Permitir imágenes externas"),
        ConfigText::Help { section: "security", key: "external_images" } => text.literal(
            "Permite que las páginas muestren imágenes de otros sitios, lo que también les revela quién vio la página. \
             Desactivado, las imágenes se limitan a esta instancia y a las imágenes data: incrustadas.",
        ),
        ConfigText::Label { section: "repository", key: "default_branch" } => text.literal("Rama predeterminada"),
        ConfigText::Help { section: "repository", key: "default_branch" } => text.literal(
            "Rama a la que apunta HEAD en los repositorios nuevos.",
        ),
        ConfigText::Label { section: "repository", key: "git_binary" } => text.literal("Ejecutable de Git"),
        ConfigText::Help { section: "repository", key: "git_binary" } => text.literal(
            "Ruta del ejecutable git que usa el transporte smart HTTP.",
        ),
        ConfigText::Label { section: "repository", key: "max_render_bytes" } => text.literal("Renderizado máximo (bytes)"),
        ConfigText::Help { section: "repository", key: "max_render_bytes" } => text.literal(
            "Los archivos más grandes no se renderizan dentro del navegador.",
        ),
        ConfigText::Label { section: "repository", key: "max_git_processes" } => text.literal("Máximo de procesos de Git"),
        ConfigText::Help { section: "repository", key: "max_git_processes" } => text.literal(
            "Procesos de transporte de Git (clonaciones, obtenciones y envíos por HTTP y SSH) que se ejecutan a la vez; \
             las solicitudes adicionales se rechazan (503 por HTTP).",
        ),
        ConfigText::Label { section: "repository", key: "max_git_requests_per_ip" } => text.literal("Máximo de solicitudes de Git por IP"),
        ConfigText::Help { section: "repository", key: "max_git_requests_per_ip" } => text.literal(
            "Solicitudes smart HTTP de Git simultáneas desde una misma dirección IP (429 al superarlas). Detrás de un \
             proxy inverso hacen falta redes de proxies de confianza.",
        ),
        ConfigText::Label { section: "repository", key: "git_idle_timeout_seconds" } => text.literal(
            "Tiempo de inactividad de Git (segundos)",
        ),
        ConfigText::Help { section: "repository", key: "git_idle_timeout_seconds" } => text.literal(
            "Segundos que un intercambio smart HTTP de Git puede pasar sin tráfico en ninguna dirección.",
        ),
        ConfigText::Label { section: "repository", key: "git_max_seconds" } => text.literal("Duración máxima de Git (segundos)"),
        ConfigText::Help { section: "repository", key: "git_max_seconds" } => text.literal(
            "Límite superior, en segundos, de un único intercambio smart HTTP de Git (una clonación o un envío muy grandes).",
        ),
        ConfigText::Label { section: "repository", key: "max_push_bytes" } => text.literal("Envío máximo (bytes)"),
        ConfigText::Help { section: "repository", key: "max_push_bytes" } => text.literal(
            "Paquete más grande que puede mandar un envío, por HTTP y SSH. 0 significa sin límite.",
        ),
        ConfigText::Label { section: "repository", key: "max_archives_per_ip" } => text.literal("Máximo de archivos comprimidos por IP"),
        ConfigText::Help { section: "repository", key: "max_archives_per_ip" } => text.literal(
            "Descargas de archivos del código fuente que una dirección IP puede tener en curso a la vez.",
        ),
        ConfigText::Label { section: "repository", key: "archive_idle_timeout_seconds" } => text.literal(
            "Tiempo de inactividad de archivos comprimidos (segundos)",
        ),
        ConfigText::Help { section: "repository", key: "archive_idle_timeout_seconds" } => text.literal(
            "Segundos que una descarga de archivo comprimido puede quedarse detenida (el cliente no lee o git no escribe).",
        ),
        ConfigText::Label { section: "repository", key: "archive_max_seconds" } => text.literal(
            "Duración máxima de archivos comprimidos (segundos)",
        ),
        ConfigText::Help { section: "repository", key: "archive_max_seconds" } => text.literal(
            "Límite superior, en segundos, de una descarga de archivo comprimido.",
        ),
        ConfigText::Label { section: "mail", key: "transport" } => text.literal("Transporte"),
        ConfigText::Help { section: "mail", key: "transport" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "mail", key: "from_address" } => text.literal("Dirección del remitente"),
        ConfigText::Help { section: "mail", key: "from_address" } => text.literal("La dirección From: de todos los mensajes."),
        ConfigText::Label { section: "mail", key: "from_name" } => text.literal("Nombre visible del remitente"),
        ConfigText::Help { section: "mail", key: "from_name" } => text.literal(
            "El nombre visible en From:. Vacío significa el nombre del sitio.",
        ),
        ConfigText::Label { section: "mail", key: "smtp_host" } => text.literal("Servidor SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_host" } => text.literal("Nombre de host del relé SMTP (transport = \"smtp\")."),
        ConfigText::Label { section: "mail", key: "smtp_port" } => text.literal("Puerto SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_port" } => text.literal(
            "Déjalo vacío para usar el puerto 587 con STARTTLS o sin cifrado, o el puerto 465 con TLS implícito.",
        ),
        ConfigText::Label { section: "mail", key: "smtp_security" } => text.literal("Cifrado SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_security" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "mail", key: "smtp_username" } => text.literal("Usuario SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_username" } => text.literal(
            "Credenciales del relé. Vacío significa sin autenticación.",
        ),
        ConfigText::Label { section: "mail", key: "smtp_password" } => text.literal("Contraseña SMTP"),
        ConfigText::Help { section: "mail", key: "smtp_password" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "mail", key: "timeout_seconds" } => text.literal("Tiempo de espera (segundos)"),
        ConfigText::Help { section: "mail", key: "timeout_seconds" } => text.literal(
            "Renuncia a un mensaje que no se haya entregado tras este número de segundos.",
        ),
        ConfigText::Label { section: "ssh", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "ssh", key: "enabled" } => text.literal("Ejecuta el servidor SSH integrado."),
        ConfigText::Label { section: "ssh", key: "bind" } => text.literal("Dirección de escucha"),
        ConfigText::Help { section: "ssh", key: "bind" } => text.literal("Dirección en la que escucha el servidor SSH."),
        ConfigText::Label { section: "ssh", key: "public_host" } => text.literal("Host público"),
        ConfigText::Help { section: "ssh", key: "public_host" } => text.literal(
            "Nombre de host que se muestra en las URL de clonación SSH. Por defecto, el host de server.public_url.",
        ),
        ConfigText::Label { section: "ssh", key: "public_port" } => text.literal("Puerto público"),
        ConfigText::Help { section: "ssh", key: "public_port" } => text.literal(
            "Puerto que se muestra en las URL de clonación SSH, cuando un proxy o una asignación de puertos difiere de bind.",
        ),
        ConfigText::Label { section: "ssh", key: "host_key_path" } => text.literal("Ruta de la clave del host"),
        ConfigText::Help { section: "ssh", key: "host_key_path" } => text.literal(
            "Ruta del archivo de clave del host SSH. Vacío usa ssh/host_ed25519 dentro de la carpeta de datos; se \
             genera en el primer arranque si falta.",
        ),
        ConfigText::Label { section: "ssh", key: "max_connections" } => text.literal("Máximo de conexiones"),
        ConfigText::Help { section: "ssh", key: "max_connections" } => text.literal(
            "Conexiones simultáneas entre todos los clientes; las conexiones adicionales se cierran de inmediato.",
        ),
        ConfigText::Label { section: "ssh", key: "max_connections_per_ip" } => text.literal("Máximo de conexiones por IP"),
        ConfigText::Help { section: "ssh", key: "max_connections_per_ip" } => text.literal(
            "Conexiones simultáneas desde una misma dirección IP.",
        ),
        ConfigText::Label { section: "ssh", key: "auth_timeout_seconds" } => text.literal("Tiempo de autenticación (segundos)"),
        ConfigText::Help { section: "ssh", key: "auth_timeout_seconds" } => text.literal(
            "Segundos que tiene un cliente para autenticarse tras conectarse.",
        ),
        ConfigText::Label { section: "ssh", key: "idle_timeout_seconds" } => text.literal("Tiempo de inactividad (segundos)"),
        ConfigText::Help { section: "ssh", key: "idle_timeout_seconds" } => text.literal(
            "Segundos sin ningún tráfico antes de cerrar una conexión.",
        ),
        ConfigText::Label { section: "ssh", key: "max_session_seconds" } => text.literal("Sesión máxima (segundos)"),
        ConfigText::Help { section: "ssh", key: "max_session_seconds" } => text.literal(
            "Límite superior, en segundos, de una única operación de Git (una clonación o un envío muy grandes).",
        ),
        ConfigText::Label { section: "auth", key: "timeout_seconds" } => text.literal("Tiempo de espera (segundos)"),
        ConfigText::Help { section: "auth", key: "timeout_seconds" } => text.literal("Límite superior de una solicitud a un proveedor."),
        ConfigText::Label { section: "auth", key: "allow_private_addresses" } => text.literal("Permitir destinos de redes privadas"),
        ConfigText::Help { section: "auth", key: "allow_private_addresses" } => text.literal(
            "Permite proveedores cuyas direcciones son de loopback, privadas, de enlace local o locales únicas, y \
             emisores o URL base por http sin cifrar.",
        ),
        ConfigText::Label { section: "branding", key: "primary_color" } => text.literal("Color principal"),
        ConfigText::Help { section: "branding", key: "primary_color" } => text.literal(
            "Fondo del botón principal en formato #RRGGBB. Define ambos colores o deja ambos vacíos.",
        ),
        ConfigText::Label { section: "branding", key: "primary_text_color" } => text.literal("Color del texto principal"),
        ConfigText::Help { section: "branding", key: "primary_text_color" } => text.literal(
            "Texto del botón principal en formato #RRGGBB. Elige un contraste legible.",
        ),
        ConfigText::Label { section: "branding", key: "description" } => text.literal("Descripción"),
        ConfigText::Help { section: "branding", key: "description" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "welcome_title" } => text.literal("Título de bienvenida"),
        ConfigText::Help { section: "branding", key: "welcome_title" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "welcome_message" } => text.literal("Mensaje de bienvenida"),
        ConfigText::Help { section: "branding", key: "welcome_message" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "logo_url" } => text.literal("URL del logotipo"),
        ConfigText::Help { section: "branding", key: "logo_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "favicon_url" } => text.literal("URL del favicon"),
        ConfigText::Help { section: "branding", key: "favicon_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "app_icon_url" } => text.literal("URL del icono de la aplicación"),
        ConfigText::Help { section: "branding", key: "app_icon_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "footer_text" } => text.literal("Texto del pie de página"),
        ConfigText::Help { section: "branding", key: "footer_text" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "support_url" } => text.literal("URL de ayuda"),
        ConfigText::Help { section: "branding", key: "support_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "terms_url" } => text.literal("URL de las condiciones"),
        ConfigText::Help { section: "branding", key: "terms_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "privacy_url" } => text.literal("URL de privacidad"),
        ConfigText::Help { section: "branding", key: "privacy_url" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "show_powered_by" } => text.literal("Mostrar «Funciona con»"),
        ConfigText::Help { section: "branding", key: "show_powered_by" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "show_version" } => text.literal("Mostrar la versión"),
        ConfigText::Help { section: "branding", key: "show_version" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "branding", key: "show_features" } => text.literal("Mostrar las funciones"),
        ConfigText::Help { section: "branding", key: "show_features" } => text.literal("Se muestra a los visitantes de esta instancia."),
        ConfigText::Label { section: "notifications", key: "email" } => text.literal("Enviar correos de notificación"),
        ConfigText::Help { section: "notifications", key: "email" } => text.literal(
            "Si las notificaciones también se envían por correo electrónico.",
        ),
        ConfigText::Label { section: "notifications", key: "default_email" } => text.literal("Suscripción de correo predeterminada"),
        ConfigText::Help { section: "notifications", key: "default_email" } => text.literal(
            "La preferencia de los usuarios que nunca eligieron una.",
        ),
        ConfigText::Label { section: "notifications", key: "digest_hour" } => text.literal("Hora predeterminada de los resúmenes (UTC)"),
        ConfigText::Help { section: "notifications", key: "digest_hour" } => text.literal(
            "La hora, de 0 a 23 en UTC, a la que se envían los resúmenes diarios y semanales por correo a los usuarios \
             que no eligieron una. Administración del sitio → Configuración la sustituye en tiempo de ejecución.",
        ),
        ConfigText::Label { section: "operations", key: "metrics" } => text.literal("Activar el endpoint de métricas"),
        ConfigText::Help { section: "operations", key: "metrics" } => text.literal("Expone métricas de Prometheus en /metrics."),
        ConfigText::Label { section: "operations", key: "metrics_token" } => text.literal("Token de acceso a las métricas"),
        ConfigText::Help { section: "operations", key: "metrics_token" } => text.literal(
            "Si se define, /metrics exige Authorization: Bearer <token>. Déjalo sin definir solo si el endpoint no es \
             accesible desde redes que no son de confianza.",
        ),
        ConfigText::Label { section: "operations", key: "warm_caches_on_push" } => text.literal("Precalentar las cachés al enviar"),
        ConfigText::Help { section: "operations", key: "warm_caches_on_push" } => text.literal(
            "Precalienta las cachés de lenguajes y del último commit de la rama predeterminada tras cada envío, para \
             que el primer visitante después de un envío no pague el cálculo.",
        ),
        ConfigText::Label { section: "operations", key: "maintenance_interval_minutes" } => text.literal(
            "Intervalo de mantenimiento (minutos)",
        ),
        ConfigText::Help { section: "operations", key: "maintenance_interval_minutes" } => text.literal(
            "Minutos entre ejecuciones de mantenimiento (git gc --auto en los repositorios que recibieron envíos desde \
             la última ejecución y, después, limpieza de la caché de estadísticas). 0 desactiva el mantenimiento \
             periódico.",
        ),
        ConfigText::Label { section: "operations", key: "job_concurrency" } => text.literal("Concurrencia de trabajos"),
        ConfigText::Help { section: "operations", key: "job_concurrency" } => text.literal(
            "Límite superior de trabajos en segundo plano (precalentamiento de cachés, git gc) que se ejecutan a la vez.",
        ),
        ConfigText::Label { section: "operations", key: "insights_cache_max_age_days" } => text.literal(
            "Antigüedad máxima de la caché de estadísticas (días)",
        ),
        ConfigText::Help { section: "operations", key: "insights_cache_max_age_days" } => text.literal(
            "El mantenimiento elimina las entradas de la caché de estadísticas más antiguas que esto. 0 conserva las \
             entradas sea cual sea su antigüedad.",
        ),
        ConfigText::Label { section: "operations", key: "insights_cache_max_entries" } => text.literal(
            "Máximo de entradas de la caché de estadísticas",
        ),
        ConfigText::Help { section: "operations", key: "insights_cache_max_entries" } => text.literal(
            "Cuando la caché de estadísticas tiene más entradas que esto, el mantenimiento elimina las más antiguas. 0 \
             significa sin límite.",
        ),
        ConfigText::Label { section: "api", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "api", key: "enabled" } => text.literal(
            "Sirve /api/v1. Si se desactiva, todas las rutas de la API responden 404.",
        ),
        ConfigText::Label { section: "api", key: "default_page_size" } => text.literal("Tamaño de página predeterminado"),
        ConfigText::Help { section: "api", key: "default_page_size" } => text.literal(
            "Elementos por página cuando la solicitud no lo indica.",
        ),
        ConfigText::Label { section: "api", key: "max_page_size" } => text.literal("Tamaño de página máximo"),
        ConfigText::Help { section: "api", key: "max_page_size" } => text.literal("Límite superior de ?per_page=."),
        ConfigText::Label { section: "webhooks", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "webhooks", key: "enabled" } => text.literal(
            "Entregar webhooks. Si se desactiva, los webhooks se pueden seguir configurando, pero no se envía nada.",
        ),
        ConfigText::Label { section: "webhooks", key: "allow_private_addresses" } => text.literal("Permitir destinos de redes privadas"),
        ConfigText::Help { section: "webhooks", key: "allow_private_addresses" } => text.literal(
            "Permite destinos que se resuelven en direcciones de loopback, privadas, de enlace local o locales únicas.",
        ),
        ConfigText::Label { section: "webhooks", key: "timeout_seconds" } => text.literal("Tiempo de espera (segundos)"),
        ConfigText::Help { section: "webhooks", key: "timeout_seconds" } => text.literal(
            "Límite superior de un intento de entrega (conectar, enviar y leer el estado de la respuesta).",
        ),
        ConfigText::Label { section: "webhooks", key: "max_attempts" } => text.literal("Máximo de intentos"),
        ConfigText::Help { section: "webhooks", key: "max_attempts" } => text.literal(
            "Intentos por entrega, incluido el primero, antes de marcarla como fallida.",
        ),
        ConfigText::Label { section: "webhooks", key: "concurrency" } => text.literal("Concurrencia"),
        ConfigText::Help { section: "webhooks", key: "concurrency" } => text.literal("Entregas en curso a la vez en toda la instancia."),
        ConfigText::Label { section: "webhooks", key: "delivery_retention_days" } => text.literal("Conservación de las entregas (días)"),
        ConfigText::Help { section: "webhooks", key: "delivery_retention_days" } => text.literal(
            "Días que el registro de entregas conserva las solicitudes y las respuestas.",
        ),
        ConfigText::Label { section: "releases", key: "max_asset_bytes" } => text.literal("Archivo adjunto máximo (bytes)"),
        ConfigText::Help { section: "releases", key: "max_asset_bytes" } => text.literal("Archivo adjunto más grande, en bytes."),
        ConfigText::Label { section: "releases", key: "max_assets_per_release" } => text.literal("Máximo de archivos por versión"),
        ConfigText::Help { section: "releases", key: "max_assets_per_release" } => text.literal("Archivos adjuntos por versión."),
        ConfigText::Label { section: "releases", key: "upload_timeout_seconds" } => text.literal("Tiempo de subida (segundos)"),
        ConfigText::Help { section: "releases", key: "upload_timeout_seconds" } => text.literal(
            "Límite superior, en segundos, de una subida.",
        ),
        ConfigText::Label { section: "mirrors", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "mirrors", key: "enabled" } => text.literal(
            "Permite crear réplicas y ejecuta las obtenciones programadas.",
        ),
        ConfigText::Label { section: "mirrors", key: "default_interval_minutes" } => text.literal("Intervalo predeterminado (minutos)"),
        ConfigText::Help { section: "mirrors", key: "default_interval_minutes" } => text.literal(
            "Intervalo de obtención de las réplicas nuevas.",
        ),
        ConfigText::Label { section: "mirrors", key: "min_interval_minutes" } => text.literal("Intervalo mínimo (minutos)"),
        ConfigText::Help { section: "mirrors", key: "min_interval_minutes" } => text.literal(
            "El intervalo más corto que se puede configurar en una réplica.",
        ),
        ConfigText::Label { section: "mirrors", key: "fetch_timeout_seconds" } => text.literal("Tiempo de obtención (segundos)"),
        ConfigText::Help { section: "mirrors", key: "fetch_timeout_seconds" } => text.literal("Límite superior de una obtención."),
        ConfigText::Label { section: "mirrors", key: "allow_private_addresses" } => text.literal("Permitir destinos de redes privadas"),
        ConfigText::Help { section: "mirrors", key: "allow_private_addresses" } => text.literal(
            "Permite URL de origen que se resuelven en direcciones de loopback, privadas, de enlace local o locales únicas.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "push_mirrors", key: "enabled" } => text.literal(
            "Permite configurar réplicas de envío y enviarles cambios.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "interval_minutes" } => text.literal("Intervalo (minutos)"),
        ConfigText::Help { section: "push_mirrors", key: "interval_minutes" } => text.literal(
            "Envía a cada réplica al menos con esta frecuencia.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "debounce_seconds" } => text.literal("Espera tras un envío (segundos)"),
        ConfigText::Help { section: "push_mirrors", key: "debounce_seconds" } => text.literal(
            "Retraso tras un envío antes de enviar a las réplicas.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "push_timeout_seconds" } => text.literal("Tiempo de envío (segundos)"),
        ConfigText::Help { section: "push_mirrors", key: "push_timeout_seconds" } => text.literal("Límite superior de un git push."),
        ConfigText::Label { section: "push_mirrors", key: "allow_private_addresses" } => text.literal(
            "Permitir destinos de redes privadas",
        ),
        ConfigText::Help { section: "push_mirrors", key: "allow_private_addresses" } => text.literal(
            "Permite remotos que se resuelven en direcciones de loopback, privadas, de enlace local o locales únicas.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "allow_local_urls" } => text.literal(
            "Permitir remotos de archivos locales (solo pruebas)",
        ),
        ConfigText::Help { section: "push_mirrors", key: "allow_local_urls" } => text.literal(
            "Admite remotos file:// (solo para pruebas; consulta la documentación del módulo).",
        ),
        ConfigText::Label { section: "imports", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "imports", key: "enabled" } => text.literal("Ofrece la importación en la página «Nuevo repositorio»."),
    }
}
