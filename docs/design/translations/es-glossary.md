# Spanish (es) glossary

The Spanish catalog is neutral international Spanish. Its terminology follows the
hand-written entries of the original families (navigation, Explore, Actions overview and
job pages, theme picker, language settings); the table-backed translations in
`crates/rbit-localization/src/catalog/translations/es/` use the same terms.

## Style

- Address the reader with **tú**: "Elige", "Añade", "tu cuenta", "Prueba otra búsqueda".
- Buttons and menu items use the infinitive: "Guardar", "Eliminar", "Crear repositorio".
- Headings and labels use sentence case: "Protección de ramas", not "Protección De Ramas".
- Quotation marks are «…». Questions and exclamations open with ¿ and ¡.
- Keep the one-character ellipsis `…`. Keep the English separators ` · `.
- Never translate machine values, code, commands (`git push`), file names, keys, or
  product and protocol names: rbit, Git, Git LFS, GitHub, Gitea, SSH, HTTPS, OpenID
  Connect, OAuth, WebAuthn, SHA-256, README, YAML, JSON, ZIP, tar.gz, cron.
- Counts: `One` takes the singular, `_` (other and whole millions) the plural.

## Terms

| English | Spanish | Notes |
|---|---|---|
| account | cuenta | |
| action (Actions feature) | Acciones | product area, capitalized as a name |
| activity | actividad | |
| add | añadir | |
| administrator / site administration | administrador / administración del sitio | |
| appearance | apariencia | |
| approve / approval | aprobar / aprobación | "Pendiente de aprobación" |
| archive / archived | archivar / archivado | |
| artifact | artefacto | |
| assignee / assign | persona asignada (asignados) / asignar | "Asignar a mí" |
| audit log | registro de auditoría | |
| author | autor | |
| avatar | avatar | |
| base branch / head branch | rama base / rama de origen | |
| blame | autoría | "Ver autoría" |
| branch | rama | |
| branch protection | protección de ramas | |
| cancel | cancelar | |
| check (status check) | comprobación | "comprobaciones obligatorias" |
| clone / clone URL | clonar / URL de clonación | |
| close / closed | cerrar / cerrada (incidencia, solicitud) | feminine for issues and pull requests |
| collaborator | colaborador | |
| comment | comentario / comentar | |
| commit (noun) | commit (los commits) | masculine; verb "hacer commit" |
| compare | comparar | |
| configuration (instance) | configuración | |
| contribution | contribución | |
| conversation | conversación | |
| delete | eliminar | permanent removal |
| remove | quitar | detach from a list without destroying |
| delivery (webhook) | entrega | |
| deploy / deployment | desplegar / despliegue | |
| deploy key | clave de despliegue | |
| diff | diferencias | "Ver diferencias" |
| directory / folder | carpeta | |
| disable / enable | desactivar / activar | |
| dispatch (event) | despacho | |
| download / upload | descargar / subir | |
| draft | borrador | |
| edit | editar | |
| email | correo electrónico (correo) | |
| environment | entorno | |
| event | evento | |
| fetch (Git) | obtener / obtención | "Obtener ahora", "Última obtención" |
| file | archivo | |
| fork (noun / verb) | bifurcación / bifurcar | |
| history | historial | |
| inbox | bandeja de entrada | |
| instance | instancia | |
| issue | incidencia | feminine |
| job | trabajo | |
| label | etiqueta | issue labels |
| lock (conversation) | bloquear | |
| log | registro | |
| member | miembro | |
| mention | mención / mencionar | |
| merge | fusionar / fusión | "Fusionar solicitud de cambios" |
| merged | fusionada | |
| milestone | hito | |
| mirror | réplica | "réplica de" |
| pull mirror / push mirror | réplica de extracción / réplica de envío | |
| upstream (of a mirror) | origen | "URL de origen" |
| remote | remoto | |
| payload (webhook) | carga | "URL de la carga" |
| timeout | tiempo de espera; "Tiempo de … (segundos)" in field labels | |
| override (configuration) | sustitución | |
| live (branding, policies) | en vivo | |
| outdated (review comment) | obsoleto | |
| dismiss (review) | descartar | |
| trusted | de confianza | |
| scope (token, OAuth) | alcance (token) / ámbito (OAuth) | |
| expire / expiration | caducar / caducidad | |
| notification | notificación | |
| open (state) | abierta | |
| organization | organización | |
| owner | propietario | |
| passkey | llave de acceso | |
| password | contraseña | |
| path | ruta | |
| permission | permiso | |
| preview | vista previa | |
| profile | perfil | |
| pull request | solicitud de cambios (solicitudes de cambios) | the Actions event label keeps "solicitud de incorporación" |
| push (noun / verb) | envío / enviar | "envíalo" |
| rebase | rebase | "Fusionar con rebase" |
| recovery code | código de recuperación | |
| ref | referencia | |
| release | versión (versiones) | |
| rename | cambiar el nombre | |
| repository | repositorio | |
| rerun | repetir | "Repetir trabajo" |
| review / reviewer | revisión / revisor | "Solicitar revisión" |
| request changes | solicitar cambios | |
| resolve | resolver | |
| rule | regla | |
| run (noun / verb) | ejecución / ejecutar | |
| runner | agente | |
| save | guardar | |
| schedule | programación | |
| secret | secreto | |
| session | sesión | |
| settings | configuración | |
| sign in / sign out / sign up | iniciar sesión / cerrar sesión / registrarse | |
| squash | squash | "Fusionar con squash" |
| SSH key | clave SSH | |
| star / starred / stargazers | estrella / marcar con estrella / con estrella | "Quitar estrella" |
| status | estado | |
| step | paso | |
| subscribe | suscribirse | |
| tag (Git) | tag (los tags) | kept distinct from labels |
| team | equipo | |
| theme | tema | |
| token (access token) | token de acceso | |
| topic | tema del repositorio | |
| two-factor authentication | autenticación en dos pasos | |
| user / username | usuario / nombre de usuario | |
| visibility | visibilidad | público, privado, interno |
| webhook | webhook | |
| wiki | wiki (la wiki) | feminine |
| workflow | flujo de trabajo (flujo) | the short "flujo" in labels |

## Status words

Correcto (success), Fallido (failure; the Actions run filter uses "Error"), Cancelado,
Omitido, En cola, En curso, En espera, Pendiente de aprobación.
