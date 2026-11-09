//! French (`fr`) entries for `catalog/repo/create.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::create::{RepoCreateLabel, RepoCreateMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoCreateLabel => MessageWriter |text| {
        RepoCreateLabel::NewRepository => text.literal("Nouveau dépôt"),
        RepoCreateLabel::RepositoryType => text.literal("Type de dépôt"),
        RepoCreateLabel::Empty => text.literal("Vide"),
        RepoCreateLabel::Mirror => text.literal("Miroir"),
        RepoCreateLabel::Import => text.literal("Importation"),
        RepoCreateLabel::NewLead => text.literal(
            "Créez un dépôt vide, importez un projet ou gardez un miroir synchronisé avec sa source.",
        ),
        RepoCreateLabel::Owner => text.literal("Propriétaire"),
        RepoCreateLabel::Name => text.literal("Nom"),
        RepoCreateLabel::Description => text.literal("Description"),
        RepoCreateLabel::Optional => text.literal("facultatif"),
        RepoCreateLabel::Visibility => text.literal("Visibilité"),
        RepoCreateLabel::Public => text.literal("Public"),
        RepoCreateLabel::Private => text.literal("Privé"),
        RepoCreateLabel::PublicRepository => text.literal("Tout le monde peut voir et cloner ce dépôt."),
        RepoCreateLabel::PublicMirror => text.literal("Tout le monde peut voir et cloner le miroir."),
        RepoCreateLabel::PublicImport => text.literal("Tout le monde peut voir et cloner le dépôt."),
        RepoCreateLabel::PrivateHint => text.literal(
            "Seul le propriétaire peut le voir. Pour une organisation, cela désigne ses membres.",
        ),
        RepoCreateLabel::Cancel => text.literal("Annuler"),
        RepoCreateLabel::CreateRepository => text.literal("Créer le dépôt"),
        RepoCreateLabel::MirrorLead => text.literal(
            "Un miroir récupère régulièrement les branches et les tags d’un dépôt public hébergé ailleurs. Il est en \
             lecture seule ici\u{a0}: les modifications proviennent de l’amont.",
        ),
        RepoCreateLabel::MirrorsDisabled => text.literal("Les miroirs sont désactivés"),
        RepoCreateLabel::MirrorsDisabledText => text.literal("Un administrateur de cette instance a désactivé les miroirs pull."),
        RepoCreateLabel::UpstreamUrl => text.literal("URL en amont"),
        RepoCreateLabel::UrlHintStart => text.literal("Une URL"),
        RepoCreateLabel::UrlHintOr => text.literal("ou"),
        RepoCreateLabel::UrlHintEnd => text.literal("d’un dépôt public."),
        RepoCreateLabel::FromTheUrl => text.literal("d’après l’URL"),
        RepoCreateLabel::FetchEvery => text.literal("Récupérer toutes les"),
        RepoCreateLabel::Minutes => text.literal("minutes"),
        RepoCreateLabel::CreateMirror => text.literal("Créer le miroir"),
        RepoCreateLabel::MirrorARepository => text.literal("Créer le miroir d’un dépôt"),
        RepoCreateLabel::ImportLead => text.literal(
            "Copie un dépôt depuis GitHub ou Gitea avec son historique et, si vous le souhaitez, ses tickets, pull \
             requests, étiquettes, jalons, releases et fichiers Git LFS. Les numéros des tickets et des pull requests \
             restent les mêmes.",
        ),
        RepoCreateLabel::ImportingDisabled => text.literal("L’importation est désactivée"),
        RepoCreateLabel::ImportingDisabledText => text.literal("Un administrateur de cette instance a désactivé l’importation."),
        RepoCreateLabel::ImportFrom => text.literal("Importer depuis"),
        RepoCreateLabel::ServerAddress => text.literal("Adresse du serveur"),
        RepoCreateLabel::NotNeededForGitHub => text.literal("inutile pour github.com"),
        RepoCreateLabel::Repository => text.literal("Dépôt"),
        RepoCreateLabel::OwnerNameOrUrl => text.literal("propriétaire/nom ou son URL"),
        RepoCreateLabel::AccessToken => text.literal("Jeton d’accès"),
        RepoCreateLabel::OptionalForPublic => text.literal("facultatif pour les dépôts publics"),
        RepoCreateLabel::TokenHint => text.literal(
            "Un jeton qui peut lire le dépôt relève les limites de débit de la source et est nécessaire pour les dépôts \
             privés. Il n’est utilisé que pendant l’importation et n’est jamais conservé.",
        ),
        RepoCreateLabel::AsOnTheSource => text.literal("comme sur la source"),
        RepoCreateLabel::BringAlong => text.literal("Importer aussi"),
        RepoCreateLabel::Wiki => text.literal("Wiki"),
        RepoCreateLabel::WikiNotSupported => text.literal("Non pris en charge\u{a0}: cette instance n’a pas de wikis."),
        RepoCreateLabel::StartImport => text.literal("Lancer l’importation"),
        RepoCreateLabel::ImportARepository => text.literal("Importer un dépôt"),
        RepoCreateLabel::ImportTitle => text.literal("Importation"),
        RepoCreateLabel::FailedNote => text.literal("Tout ce qui a été importé jusque-là se trouve dans le dépôt."),
        RepoCreateLabel::NotReached => text.literal("Non atteint"),
        RepoCreateLabel::Started => text.literal("Démarré"),
        RepoCreateLabel::WaitingForSlot => text.literal("En attente d’un créneau libre"),
        RepoCreateLabel::Ended => text.literal("Terminé"),
        RepoCreateLabel::UpdatesByItself => text.literal("Cette page se met à jour d’elle-même."),
        RepoCreateLabel::SkippedOrIncomplete => text.literal("Ignoré ou incomplet"),
        RepoCreateLabel::GoToRepository => text.literal("Accéder au dépôt"),
        RepoCreateLabel::OpensAutomatically => text.literal(
            "Le dépôt s’ouvrira automatiquement à la fin de sa première récupération en miroir.",
        ),
        RepoCreateLabel::FetchInProgress => text.literal("Récupération du miroir en cours"),
        RepoCreateLabel::TransferSizeNote => text.literal(
            "La taille du transfert dépend de l’amont. Cette page affiche l’étape en cours au fil de l’avancement.",
        ),
        RepoCreateLabel::RefreshStatus => text.literal("Actualiser l’état"),
        RepoCreateLabel::SettingsAndRetry => text.literal("Paramètres du miroir et nouvelle tentative"),
        RepoCreateLabel::NoScriptHint => text.literal(
            "Utilisez «\u{a0}Actualiser l’état\u{a0}» pour suivre l’avancement et ouvrez le dépôt lorsqu’il est prêt.",
        ),
        RepoCreateLabel::Connecting => text.literal("Connexion au dépôt en amont."),
        RepoCreateLabel::Fetching => text.literal(
            "Téléchargement des branches, des tags et des objets Git. Les dépôts volumineux peuvent prendre plusieurs minutes.",
        ),
        RepoCreateLabel::Finalizing => text.literal("Préparation du contenu du dépôt et de la branche par défaut."),
        RepoCreateLabel::FetchFailedRetry => text.literal(
            "Le miroir n’a pas pu être récupéré. Un administrateur du dépôt peut réessayer depuis les paramètres du miroir.",
        ),
        RepoCreateLabel::Complete => text.literal("Mise en miroir terminée. Ouverture du dépôt…"),
        RepoCreateLabel::Queued => text.literal("En attente d’un worker de miroir disponible."),
        RepoCreateLabel::Stopped => text.literal("La mise en miroir s’est arrêtée. Ouverture du dépôt…"),
        RepoCreateLabel::Mirroring => text.literal("Mise en miroir"),
    }
    RepoCreateMessage<'a> => MessageWriter |text| {
        RepoCreateMessage::AtLeastMinutes { minutes } => {
            text.literal("Au moins ");
            text.number(minutes);
            text.literal(match text.plural(minutes) {
                Plural::One => " minute.",
                _ => " minutes.",
            });
        }
        RepoCreateMessage::ImportFrom { source } => {
            text.literal("Depuis ");
            text.argument(source);
            text.literal("\u{a0}:");
        }
        RepoCreateMessage::MoreWarnings { count } => {
            text.literal("et ");
            text.number(count);
            text.literal(" de plus (voir le journal du serveur)");
        }
        RepoCreateMessage::MirroringName { name } => {
            text.literal("Mise en miroir de ");
            text.argument(name);
        }
        RepoCreateMessage::FetchFailed { error } => {
            text.literal("Le miroir n’a pas pu être récupéré\u{a0}: ");
            text.argument(error);
        }
    }
}
