//! French (`fr`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};
use crate::plural::Plural;

entries! { Fr:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("Administration du site"),
        AdminMessage::Administration => text.literal("Administration"),
        AdminMessage::Dashboard => text.literal("Tableau de bord"),
        AdminMessage::Users => text.literal("Utilisateurs"),
        AdminMessage::Organizations => text.literal("Organisations"),
        AdminMessage::Repositories => text.literal("Dépôts"),
        AdminMessage::Settings => text.literal("Paramètres"),
        AdminMessage::AuditLog => text.literal("Journal d’audit"),
        AdminMessage::Configuration => text.literal("Configuration"),
        AdminMessage::Runners => text.literal("Runners"),
        AdminMessage::Search => text.literal("Rechercher"),
        AdminMessage::Pagination => text.literal("Pagination"),
        AdminMessage::Previous => text.literal("Précédent"),
        AdminMessage::Next => text.literal("Suivant"),
        AdminMessage::NoResults => text.literal("Aucun résultat"),
        AdminMessage::Cancel => text.literal("Annuler"),
        AdminMessage::Actions => text.literal("Actions"),
        AdminMessage::Private => text.literal("Privé"),
        AdminMessage::Archived => text.literal("Archivé"),
        AdminMessage::Delete => text.literal("Supprimer"),
        AdminMessage::TypeToConfirmLead => text.literal("Saisissez"),
        AdminMessage::TypeToConfirmTail => text.literal("pour confirmer"),
        AdminMessage::NewPassword => text.literal("Nouveau mot de passe"),
        AdminMessage::ConfirmPassword => text.literal("Confirmer le nouveau mot de passe"),
        AdminMessage::Unavailable => text.literal("indisponible"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("Une vue d’ensemble de cette instance."),
        DashboardMessage::RestartServer => text.literal("Redémarrer le serveur"),
        DashboardMessage::RestartUnavailableHint => text.literal("Le redémarrage n’est pas disponible sur cet hôte"),
        DashboardMessage::SoftwareUpdates => text.literal("Mises à jour logicielles"),
        DashboardMessage::ConfigureUpdates => text.literal(
            "Configurer la recherche de mises à jour et la confidentialité de la version",
        ),
        DashboardMessage::LiveMonitoring => text.literal("Surveillance en direct"),
        DashboardMessage::MonitoringIntro => text.literal(
            "Trafic, temps de réponse et activité Git au cours des cinq dernières minutes.",
        ),
        DashboardMessage::Snapshot => text.literal("Instantané"),
        DashboardMessage::Pause => text.literal("Pause"),
        DashboardMessage::NeedsJavaScript => text.literal(
            "Les mises à jour en direct nécessitent JavaScript. Rechargez cette page pour obtenir un nouvel instantané.",
        ),
        DashboardMessage::System => text.literal("Système"),
        DashboardMessage::InstanceSettings => text.literal("Paramètres de l’instance"),
        DashboardMessage::Edit => text.literal("Modifier"),
        DashboardMessage::Users => text.literal("Utilisateurs"),
        DashboardMessage::Organizations => text.literal("Organisations"),
        DashboardMessage::SharedNamespaces => text.literal("Espaces de noms partagés"),
        DashboardMessage::Repositories => text.literal("Dépôts"),
        DashboardMessage::AccessTokens => text.literal("Jetons d’accès"),
        DashboardMessage::AcrossAllUsers => text.literal("Tous utilisateurs confondus"),
        DashboardMessage::Uptime => text.literal("Disponibilité"),
        DashboardMessage::DataDirectory => text.literal("Répertoire de données"),
        DashboardMessage::DiskUsage => text.literal("Utilisation du disque"),
        DashboardMessage::SiteName => text.literal("Nom du site"),
        DashboardMessage::Explore => text.literal("Page Explorer"),
        DashboardMessage::ExploreEnabled => text.literal("Activée"),
        DashboardMessage::ExploreDisabled => text.literal("Désactivée"),
        DashboardMessage::Browsing => text.literal("Navigation"),
        DashboardMessage::BrowsingOpen => text.literal("Tout le monde, connecté ou non"),
        DashboardMessage::BrowsingSignedIn => text.literal("Utilisateurs connectés uniquement"),
        DashboardMessage::Registration => text.literal("Inscriptions"),
        DashboardMessage::RegistrationOpen => text.literal("Ouvertes à tous"),
        DashboardMessage::RegistrationClosed => text.literal("Fermées (les administrateurs créent les comptes)"),
        DashboardMessage::OrgPolicy => text.literal("Organisations"),
        DashboardMessage::AnyUserCreates => text.literal("Tout utilisateur peut en créer"),
        DashboardMessage::AdminsCreate => text.literal("Seuls les administrateurs peuvent en créer"),
        DashboardMessage::NewRepositories => text.literal("Nouveaux dépôts"),
        DashboardMessage::PublicByDefault => text.literal("Publics par défaut"),
        DashboardMessage::PrivateByDefault => text.literal("Privés par défaut"),
        DashboardMessage::EmailDigests => text.literal("Récapitulatifs par e-mail"),
        DashboardMessage::Restarting => text.literal("Redémarrage du serveur"),
        DashboardMessage::RestartRequested => text.literal("Redémarrage demandé"),
        DashboardMessage::RestartingText => text.literal(
            "Le serveur termine les requêtes HTTP en cours, arrête SSH et les tâches d’arrière-plan, et ferme le \
             stockage avant de redémarrer. Les transferts de longue durée peuvent prendre un certain temps à se terminer.",
        ),
        DashboardMessage::Waiting => text.literal("En attente du retour du serveur…"),
        DashboardMessage::ReturnToDashboard => text.literal("Revenir au tableau de bord"),
        DashboardMessage::WaitNoScript => text.literal(
            "Patientez un instant, puis revenez au tableau de bord pour vous reconnecter.",
        ),
        DashboardMessage::RestartQuestion => text.literal("Redémarrer cette application rbit\u{202f}?"),
        DashboardMessage::RestartText => text.literal(
            "Le site sera brièvement indisponible pendant que l’application s’arrête puis redémarre avec sa \
             configuration enregistrée. Les requêtes HTTP peuvent se terminer\u{202f}; les connexions SSH seront fermées. \
             Les utilisateurs conserveront leurs sessions enregistrées.",
        ),
        DashboardMessage::RestartAudited => text.literal(
            "Cette action est consignée dans le journal d’audit de l’administration.",
        ),
        DashboardMessage::RestartUnavailable => text.literal("Le redémarrage n’est pas disponible"),
        DashboardMessage::RestartUnsupported => text.literal(
            "Cet hôte ne prend pas en charge le redémarrage de l’application. Redémarrez rbit avec votre gestionnaire \
             de services.",
        ),
        DashboardMessage::BackToDashboard => text.literal("Retour au tableau de bord"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("Requêtes dans la fenêtre"),
        MonitoringMessage::ErrorRate => text.literal("Taux d’erreurs serveur"),
        MonitoringMessage::ProcessUptime => text.literal("Durée de fonctionnement du processus"),
        MonitoringMessage::Samples => text.literal("Échantillons de 5\u{a0}secondes"),
        MonitoringMessage::FiveMinutesAgo => text.literal("Il y a 5\u{a0}minutes"),
        MonitoringMessage::LatestSample => text.literal("Dernier échantillon"),
        MonitoringMessage::Collecting => text.literal(
            "Collecte du premier échantillon. Les graphiques commencent au bout de cinq secondes.",
        ),
        MonitoringMessage::HttpResponses => text.literal("Réponses HTTP"),
        MonitoringMessage::CurrentWindow => text.literal("Fenêtre actuelle"),
        MonitoringMessage::Status => text.literal("Statut"),
        MonitoringMessage::RequestsColumn => text.literal("Requêtes"),
        MonitoringMessage::Share => text.literal("Part"),
        MonitoringMessage::GitActivity => text.literal("Activité Git"),
        MonitoringMessage::Operation => text.literal("Opération"),
        MonitoringMessage::Exchanges => text.literal("Échanges"),
        MonitoringMessage::CloneFetch => text.literal("Clone / fetch"),
        MonitoringMessage::Push => text.literal("Push"),
        MonitoringMessage::GitNote => text.literal(
            "Échanges Smart HTTP, échecs compris. SSH et les annonces de références sont exclus. Le temps de réponse \
             mesure le délai jusqu’aux en-têtes\u{202f}; les transferts en flux peuvent se poursuivre.",
        ),
        MonitoringMessage::Footnote => text.literal(
            "Ce processus uniquement\u{202f}; l’historique est réinitialisé au redémarrage. Les requêtes de \
             surveillance sont exclues.",
        ),
        MonitoringMessage::Informational => text.literal("Informatives"),
        MonitoringMessage::Successful => text.literal("Réussies"),
        MonitoringMessage::Redirects => text.literal("Redirections"),
        MonitoringMessage::ClientErrors => text.literal("Erreurs client"),
        MonitoringMessage::ServerErrors => text.literal("Erreurs serveur"),
        MonitoringMessage::RequestTraffic => text.literal("Trafic des requêtes"),
        MonitoringMessage::ResponseTime => text.literal("Temps de réponse"),
        MonitoringMessage::RequestsPerSecond => text.literal("req/s"),
        MonitoringMessage::MillisecondsAverage => text.literal("ms moy."),
        MonitoringMessage::WaitingFirst => text.literal("En attente du premier échantillon"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("Voir les notes de version de rbit ");
            text.argument(version);
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.argument(admins);
            text.literal(" administrateurs · ");
            text.argument(suspended);
            text.literal(" suspendus");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("1 administrateur · ");
            text.argument(suspended);
            text.literal(" suspendus");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.argument(public);
            text.literal(" publics · ");
            text.argument(private);
            text.literal(" privés");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" sur ");
            text.argument(total);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("Envoyés à ");
            text.argument(hour);
            text.literal(", sauf choix contraire des utilisateurs");
        }
        AdminText::DeleteLabel(name) => {
            text.literal("Supprimer ");
            text.argument(name);
        }
        AdminText::ActionsFor(name) => {
            text.literal("Actions pour ");
            text.argument(name);
        }
        AdminText::PasswordLength(length) => {
            text.literal("Au moins ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " caractère.",
                _ => " caractères.",
            });
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.argument(title);
            text.literal(" au cours des cinq dernières minutes. Dernière valeur\u{a0}: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". L’absence de requêtes laisse des trous dans le temps de réponse.");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("Mis à jour à ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" sur ");
            text.number(limit);
            text.literal(match text.plural(limit) {
                Plural::One => " échantillon",
                _ => " échantillons",
            });
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · Administration");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("Définir le mot de passe de ");
            text.argument(name);
            text.literal(" · Administration");
        }
        AdminText::DeletePageTitle(name) => {
            text.literal("Supprimer ");
            text.argument(name);
            text.literal(" · Administration");
        }
    }
}
