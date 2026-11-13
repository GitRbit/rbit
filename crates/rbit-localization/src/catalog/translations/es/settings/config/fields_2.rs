//! Spanish (`es`) entries for the configuration fields of `catalog/settings/config/`, part 2.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

config_arms! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "imports", key: "allow_private_addresses" } => text.literal("Permitir destinos de redes privadas"),
        ConfigText::Help { section: "imports", key: "allow_private_addresses" } => text.literal(
            "Permite orígenes que se resuelven en direcciones de loopback, privadas, de enlace local o locales únicas.",
        ),
        ConfigText::Label { section: "imports", key: "max_concurrent" } => text.literal("Máximo simultáneo"),
        ConfigText::Help { section: "imports", key: "max_concurrent" } => text.literal("Importaciones que se ejecutan a la vez."),
        ConfigText::Label { section: "imports", key: "git_timeout_seconds" } => text.literal("Tiempo de Git (segundos)"),
        ConfigText::Help { section: "imports", key: "git_timeout_seconds" } => text.literal("Límite superior de la copia de Git."),
        ConfigText::Label { section: "imports", key: "api_timeout_seconds" } => text.literal("Tiempo de la API (segundos)"),
        ConfigText::Help { section: "imports", key: "api_timeout_seconds" } => text.literal("Límite superior de una solicitud a la API."),
        ConfigText::Label { section: "imports", key: "rate_limit_max_wait_seconds" } => text.literal(
            "Espera máxima por límite de frecuencia (segundos)",
        ),
        ConfigText::Help { section: "imports", key: "rate_limit_max_wait_seconds" } => text.literal(
            "Espera más larga hasta que se restablezca el límite de frecuencia del origen.",
        ),
        ConfigText::Label { section: "imports", key: "author_mapping" } => text.literal("Asociar los autores importados"),
        ConfigText::Help { section: "imports", key: "author_mapping" } => text.literal(
            "Cómo se convierten en cuentas locales los autores del origen.",
        ),
        ConfigText::Label { section: "issues", key: "scan_commit_limit" } => text.literal("Límite de commits analizados"),
        ConfigText::Help { section: "issues", key: "scan_commit_limit" } => text.literal(
            "Commits analizados por cada actualización de referencia enviada, en busca de referencias y palabras clave de cierre.",
        ),
        ConfigText::Label { section: "issues", key: "notifications" } => text.literal("Notificaciones"),
        ConfigText::Help { section: "issues", key: "notifications" } => text.literal(
            "Si se generan y se muestran notificaciones dentro de la aplicación.",
        ),
        ConfigText::Label { section: "issues", key: "notification_retention_days" } => text.literal(
            "Conservación de las notificaciones (días)",
        ),
        ConfigText::Help { section: "issues", key: "notification_retention_days" } => text.literal(
            "Días que se conserva una notificación leída.",
        ),
        ConfigText::Label { section: "pulls", key: "merge_timeout_seconds" } => text.literal("Tiempo de fusión (segundos)"),
        ConfigText::Help { section: "pulls", key: "merge_timeout_seconds" } => text.literal(
            "Límite superior, en segundos, de una fusión o fusión de prueba: todo su trabajo de git, desde crear el \
             repositorio temporal hasta la obtención y la actualización de referencias del repositorio base.",
        ),
        ConfigText::Label { section: "pulls", key: "max_concurrent_merges" } => text.literal("Máximo de fusiones simultáneas"),
        ConfigText::Help { section: "pulls", key: "max_concurrent_merges" } => text.literal(
            "Fusiones en curso a la vez en toda la instancia (siempre una por repositorio base).",
        ),
        ConfigText::Label { section: "pulls", key: "mergeability_concurrency" } => text.literal(
            "Concurrencia de las comprobaciones de fusión",
        ),
        ConfigText::Help { section: "pulls", key: "mergeability_concurrency" } => text.literal(
            "Fusiones de prueba para comprobar si se puede fusionar que se ejecutan a la vez.",
        ),
        ConfigText::Label { section: "pulls", key: "max_commits" } => text.literal("Máximo de commits"),
        ConfigText::Help { section: "pulls", key: "max_commits" } => text.literal("Commits mostrados por solicitud de cambios."),
        ConfigText::Label { section: "pulls", key: "noreply_domain" } => text.literal("Dominio de correo sin respuesta"),
        ConfigText::Help { section: "pulls", key: "noreply_domain" } => text.literal(
            "Dominio usado para los autores sin respuesta en los commits generados. Vacío usa noreply seguido del \
             nombre de tu host público.",
        ),
        ConfigText::Label { section: "editor", key: "max_upload_bytes" } => text.literal("Subida máxima (bytes)"),
        ConfigText::Help { section: "editor", key: "max_upload_bytes" } => text.literal("Archivo subido más grande, en bytes."),
        ConfigText::Label { section: "editor", key: "max_upload_files" } => text.literal("Máximo de archivos por subida"),
        ConfigText::Help { section: "editor", key: "max_upload_files" } => text.literal("Archivos por subida."),
        ConfigText::Label { section: "editor", key: "upload_timeout_seconds" } => text.literal("Tiempo de subida (segundos)"),
        ConfigText::Help { section: "editor", key: "upload_timeout_seconds" } => text.literal(
            "Límite superior, en segundos, de una solicitud de subida.",
        ),
        ConfigText::Label { section: "lfs", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "lfs", key: "enabled" } => text.literal("Sirve la API de LFS y el comando SSH git-lfs-authenticate."),
        ConfigText::Label { section: "lfs", key: "max_object_bytes" } => text.literal("Objeto máximo (bytes)"),
        ConfigText::Help { section: "lfs", key: "max_object_bytes" } => text.literal("Objeto más grande, en bytes."),
        ConfigText::Label { section: "lfs", key: "token_lifetime_seconds" } => text.literal("Duración del token (segundos)"),
        ConfigText::Help { section: "lfs", key: "token_lifetime_seconds" } => text.literal(
            "Duración de las cabeceras de transferencia firmadas, en segundos.",
        ),
        ConfigText::Label { section: "actions", key: "enabled" } => text.literal("Activar esta función"),
        ConfigText::Help { section: "actions", key: "enabled" } => text.literal(
            "Ejecuta flujos de trabajo en rbit-runner. Si se desactiva, Acciones desaparece por completo: sin páginas, \
             sin endpoint para agentes, sin trabajo en segundo plano, y los archivos de flujos de trabajo se ignoran. \
             Requiere reiniciar.",
        ),
        ConfigText::Label { section: "actions", key: "enable_for_new_repositories" } => text.literal("Activar en los repositorios nuevos"),
        ConfigText::Help { section: "actions", key: "enable_for_new_repositories" } => text.literal(
            "Activa Acciones en los repositorios que se creen a partir de ahora. Los repositorios existentes conservan \
             su propia configuración.",
        ),
        ConfigText::Label { section: "actions", key: "secrets_key_file" } => text.literal("Archivo de clave de los secretos"),
        ConfigText::Help { section: "actions", key: "secrets_key_file" } => text.literal(
            "Clave de cifrado de los secretos de Acciones. Vacío usa actions/secrets.key en la carpeta de datos. Haz \
             una copia de seguridad aparte.",
        ),
        ConfigText::Label { section: "actions", key: "log_retention_days" } => text.literal("Conservación de los registros (días)"),
        ConfigText::Help { section: "actions", key: "log_retention_days" } => text.literal(
            "Días que se conservan los registros de los trabajos.",
        ),
        ConfigText::Label { section: "actions", key: "run_retention_days" } => text.literal("Conservación de las ejecuciones (días)"),
        ConfigText::Help { section: "actions", key: "run_retention_days" } => text.literal(
            "Días que se conservan las ejecuciones terminadas.",
        ),
        ConfigText::Label { section: "actions", key: "max_log_bytes" } => text.literal("Registro máximo (bytes)"),
        ConfigText::Help { section: "actions", key: "max_log_bytes" } => text.literal(
            "Bytes de registro conservados por tarea; las líneas posteriores se descartan.",
        ),
        ConfigText::Label { section: "actions", key: "max_workflow_bytes" } => text.literal("Archivo de flujo de trabajo máximo (bytes)"),
        ConfigText::Help { section: "actions", key: "max_workflow_bytes" } => text.literal(
            "Archivo de flujo de trabajo más grande que se lee, en bytes.",
        ),
        ConfigText::Label { section: "actions", key: "zombie_timeout_seconds" } => text.literal("Tiempo sin latido (segundos)"),
        ConfigText::Help { section: "actions", key: "zombie_timeout_seconds" } => text.literal(
            "Una tarea en curso que pasa este tiempo sin latido falla.",
        ),
        ConfigText::Label { section: "actions", key: "endless_timeout_seconds" } => text.literal(
            "Límite de tiempo de una tarea (segundos)",
        ),
        ConfigText::Help { section: "actions", key: "endless_timeout_seconds" } => text.literal(
            "Una tarea que se ejecuta durante este tiempo falla.",
        ),
        ConfigText::Label { section: "actions", key: "abandoned_timeout_seconds" } => text.literal("Límite de tiempo en cola (segundos)"),
        ConfigText::Help { section: "actions", key: "abandoned_timeout_seconds" } => text.literal(
            "Un trabajo en espera que ningún agente toma durante este tiempo se cancela.",
        ),
        ConfigText::Label { section: "actions", key: "evaluation_concurrency" } => text.literal("Concurrencia de evaluación"),
        ConfigText::Help { section: "actions", key: "evaluation_concurrency" } => text.literal(
            "Evaluaciones de flujos de trabajo (lecturas de Git y análisis de YAML) que se ejecutan a la vez, de 1 a 16.",
        ),
        ConfigText::Label { section: "actions", key: "instance_runners_for_all" } => text.literal(
            "Los agentes de la instancia sirven a todos",
        ),
        ConfigText::Help { section: "actions", key: "instance_runners_for_all" } => text.literal(
            "Permite que los agentes de la instancia ejecuten trabajos de cualquier propietario. Si se desactiva, solo \
             sirven a los propietarios que autorizó un administrador.",
        ),
        ConfigText::Label { section: "actions", key: "max_log_storage_bytes" } => text.literal(
            "Presupuesto de almacenamiento de registros (bytes)",
        ),
        ConfigText::Help { section: "actions", key: "max_log_storage_bytes" } => text.literal(
            "Bytes de registro conservados en toda la instancia; al superarlos, las ejecuciones terminadas más antiguas \
             pierden primero sus registros.",
        ),
        ConfigText::Label { section: "actions", key: "max_waiting_jobs_per_repository" } => text.literal(
            "Máximo de trabajos en espera por repositorio",
        ),
        ConfigText::Help { section: "actions", key: "max_waiting_jobs_per_repository" } => text.literal(
            "Las ejecuciones nuevas que superen este número de trabajos en espera fallan con un error legible.",
        ),
        ConfigText::Label { section: "actions", key: "max_runs_per_event" } => text.literal("Máximo de ejecuciones por evento"),
        ConfigText::Help { section: "actions", key: "max_runs_per_event" } => text.literal(
            "Ejecuciones que puede iniciar un evento de envío o de solicitud de cambios.",
        ),
        ConfigText::Label { section: "actions", key: "max_jobs_per_event" } => text.literal("Máximo de trabajos por evento"),
        ConfigText::Help { section: "actions", key: "max_jobs_per_event" } => text.literal(
            "Trabajos que puede crear un evento entre todas sus ejecuciones.",
        ),
        ConfigText::Label { section: "actions", key: "max_schedules_per_repository" } => text.literal(
            "Máximo de programaciones por repositorio",
        ),
        ConfigText::Help { section: "actions", key: "max_schedules_per_repository" } => text.literal(
            "Entradas de flujos de trabajo programados por repositorio.",
        ),
        ConfigText::Label { section: "actions", key: "long_poll_seconds" } => text.literal("Sondeo largo de los agentes (segundos)"),
        ConfigText::Help { section: "actions", key: "long_poll_seconds" } => text.literal(
            "Cuánto tiempo se mantiene abierto el sondeo de trabajo de un agente, de 1 a 25.",
        ),
        ConfigText::Label { section: "actions", key: "max_polls" } => text.literal("Máximo de sondeos de agentes"),
        ConfigText::Help { section: "actions", key: "max_polls" } => text.literal(
            "Sondeos de agentes abiertos a la vez; al superarlos se rechazan sondeos hasta que termine alguno.",
        ),
        ConfigText::Label { section: "actions", key: "max_artifact_bytes" } => text.literal("Tamaño máximo de artefacto (bytes)"),
        ConfigText::Help { section: "actions", key: "max_artifact_bytes" } => text.literal(
            "Artefacto más grande que puede subir un trabajo.",
        ),
        ConfigText::Label { section: "actions", key: "max_artifact_storage_bytes" } => text.literal(
            "Almacenamiento de artefactos por repositorio (bytes)",
        ),
        ConfigText::Help { section: "actions", key: "max_artifact_storage_bytes" } => text.literal(
            "Bytes de artefactos conservados por repositorio; se rechazan las subidas que lo superen. Las copias de \
             seguridad (formato 5 y posteriores) incluyen los artefactos.",
        ),
        ConfigText::Label { section: "actions", key: "artifact_retention_days" } => text.literal("Conservación de los artefactos (días)"),
        ConfigText::Help { section: "actions", key: "artifact_retention_days" } => text.literal(
            "Días que se conservan los artefactos. El retention-days de un flujo de trabajo solo puede reducirlo.",
        ),
        ConfigText::Label { section: "actions", key: "max_cache_storage_bytes" } => text.literal(
            "Almacenamiento de caché por repositorio (bytes)",
        ),
        ConfigText::Help { section: "actions", key: "max_cache_storage_bytes" } => text.literal(
            "Bytes de actions/cache conservados por repositorio para todos sus agentes; primero se eliminan las \
             entradas usadas hace más tiempo. Las copias de seguridad (formato 5 y posteriores) incluyen las entradas \
             de caché.",
        ),
        ConfigText::Label { section: "actions", key: "cache_retention_days" } => text.literal("Conservación de la caché (días)"),
        ConfigText::Help { section: "actions", key: "cache_retention_days" } => text.literal(
            "Días que se conserva una entrada de caché sin restaurarse.",
        ),
        ConfigText::Label { section: "provider", key: "kind" } => text.literal("Tipo de proveedor"),
        ConfigText::Help { section: "provider", key: "kind" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "provider", key: "id" } => text.literal("ID del proveedor"),
        ConfigText::Help { section: "provider", key: "id" } => text.literal(
            "Un ID corto y único usado en las URL de retorno. Si lo cambias, tendrás que volver a introducir el secreto del proveedor.",
        ),
        ConfigText::Label { section: "provider", key: "name" } => text.literal("Nombre visible"),
        ConfigText::Help { section: "provider", key: "name" } => text.literal("Se muestra en los botones y en la configuración."),
        ConfigText::Label { section: "provider", key: "issuer" } => text.literal("URL del emisor OIDC"),
        ConfigText::Help { section: "provider", key: "issuer" } => text.literal("URL del emisor OIDC (solo kind = \"oidc\")."),
        ConfigText::Label { section: "provider", key: "base_url" } => text.literal("URL de GitHub Enterprise"),
        ConfigText::Help { section: "provider", key: "base_url" } => text.literal(
            "URL de GitHub Enterprise Server (solo kind = \"github\"); por defecto, https://github.com.",
        ),
        ConfigText::Label { section: "provider", key: "client_id" } => text.literal("ID de cliente"),
        ConfigText::Help { section: "provider", key: "client_id" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "provider", key: "client_secret" } => text.literal("Secreto de cliente"),
        ConfigText::Help { section: "provider", key: "client_secret" } => text.literal(
            "Nunca se registra ni se muestra (consulta la implementación de Debug).",
        ),
        ConfigText::Label { section: "provider", key: "scopes" } => text.literal("Ámbitos solicitados"),
        ConfigText::Help { section: "provider", key: "scopes" } => text.literal(
            "Introduce un ámbito por línea. Usa los valores predeterminados del proveedor para solicitar sus ámbitos recomendados.",
        ),
        ConfigText::Label { section: "provider", key: "allow_registration" } => text.literal("Creación de cuentas"),
        ConfigText::Help { section: "provider", key: "allow_registration" } => text.literal("Se aplica tras reiniciar el servidor."),
        ConfigText::Label { section: "provider", key: "trust_email" } => text.literal("Confiar en las direcciones de correo verificadas"),
        ConfigText::Help { section: "provider", key: "trust_email" } => text.literal(
            "Vincula un primer inicio de sesión con la única cuenta cuyo correo verificado coincide con el correo \
             verificado del proveedor. Solo para proveedores que controlan las direcciones de sus usuarios. Un \
             registro a través de él también da por verificada la dirección que verificó, sin correo de confirmación.",
        ),
        ConfigText::Label { section: "provider", key: "trust_mfa" } => text.literal(
            "Confiar en la autenticación multifactor del proveedor",
        ),
        ConfigText::Help { section: "provider", key: "trust_mfa" } => text.literal(
            "Omite el paso de dos pasos propio de la cuenta cuando la reclamación amr del token de ID indica que el \
             proveedor realizó una autenticación multifactor (\"mfa\"). Solo OIDC.",
        ),
    }
}
