//! French (`fr`) entries for `catalog/repo/settings.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::settings::{RepoSettingsLabel, RepoSettingsMessage};
use crate::plural::Plural;

entries! { Fr:
    RepoSettingsLabel => MessageWriter |text| {
        RepoSettingsLabel::Settings => text.literal("Paramètres"),
        RepoSettingsLabel::AdminsOf => text.literal("Seuls les administrateurs de"),
        RepoSettingsLabel::AdminsOfEnd => text.literal("peuvent voir cette page."),
        RepoSettingsLabel::SettingsSaved => text.literal("Paramètres enregistrés."),
        RepoSettingsLabel::General => text.literal("Général"),
        RepoSettingsLabel::Description => text.literal("Description"),
        RepoSettingsLabel::Optional => text.literal("facultatif"),
        RepoSettingsLabel::Visibility => text.literal("Visibilité"),
        RepoSettingsLabel::Public => text.literal("Public"),
        RepoSettingsLabel::PublicHint => text.literal("Tout le monde peut voir et cloner ce dépôt."),
        RepoSettingsLabel::Private => text.literal("Privé"),
        RepoSettingsLabel::PrivateHint => text.literal("Seules les personnes ayant accès peuvent le voir et le cloner."),
        RepoSettingsLabel::DefaultBranch => text.literal("Branche par défaut"),
        RepoSettingsLabel::PushABranch => text.literal("Poussez une branche pour choisir la branche par défaut."),
        RepoSettingsLabel::DefaultBranchHint => text.literal(
            "Affichée en premier lors de la navigation et extraite par git clone.",
        ),
        RepoSettingsLabel::SaveChanges => text.literal("Enregistrer les modifications"),
        RepoSettingsLabel::RepositoryName => text.literal("Nom du dépôt"),
        RepoSettingsLabel::Name => text.literal("Nom"),
        RepoSettingsLabel::RenameHint => text.literal(
            "Les liens et les URL de clonage qui utilisent l’ancien nom cesseront de fonctionner.",
        ),
        RepoSettingsLabel::Rename => text.literal("Renommer"),
        RepoSettingsLabel::DangerZone => text.literal("Zone de danger"),
        RepoSettingsLabel::UnarchiveTitle => text.literal("Désarchiver ce dépôt"),
        RepoSettingsLabel::UnarchiveHint => text.literal(
            "Il accepte de nouveau les push, les tickets, les pull requests, les commentaires et les releases, et ses \
             miroirs reprennent.",
        ),
        RepoSettingsLabel::Unarchive => text.literal("Désarchiver le dépôt"),
        RepoSettingsLabel::ArchiveTitle => text.literal("Archiver ce dépôt"),
        RepoSettingsLabel::ArchiveHint => text.literal(
            "Il passe en lecture seule\u{a0}: chacun peut encore le cloner, en créer un fork et lui ajouter une étoile, \
             mais personne ne peut pousser, fusionner ni modifier ses tickets, ses pull requests ou ses releases, et \
             ses miroirs sont mis en pause. Vous pouvez le désarchiver à tout moment.",
        ),
        RepoSettingsLabel::Archive => text.literal("Archiver le dépôt"),
        RepoSettingsLabel::DeleteTitle => text.literal("Supprimer ce dépôt"),
        RepoSettingsLabel::DeleteHint => text.literal(
            "Ses fichiers, son historique et ses étoiles sont supprimés définitivement. Cette action est irréversible.",
        ),
        RepoSettingsLabel::Type => text.literal("Saisissez"),
        RepoSettingsLabel::ToConfirm => text.literal("pour confirmer"),
        RepoSettingsLabel::DeleteRepository => text.literal("Supprimer le dépôt"),
        RepoSettingsLabel::DeployKeys => text.literal("Clés de déploiement"),
        RepoSettingsLabel::DeployKeysHint => text.literal(
            "Des clés qui donnent à un serveur ou à un job de CI un accès SSH à ce seul dépôt.",
        ),
        RepoSettingsLabel::DeployKeysLeadStart => text.literal(
            "Une clé de déploiement donne à un serveur ou à un job de CI un accès SSH à",
        ),
        RepoSettingsLabel::DeployKeysLeadEnd => text.literal(
            "uniquement. Les clés sont en lecture seule, sauf si vous les autorisez à pousser.",
        ),
        RepoSettingsLabel::NewDeployKey => text.literal("Nouvelle clé de déploiement"),
        RepoSettingsLabel::Title => text.literal("Titre"),
        RepoSettingsLabel::TitlePlaceholder => text.literal("p. ex. Serveur de production"),
        RepoSettingsLabel::TitleHint => text.literal("Par défaut, le commentaire de la clé."),
        RepoSettingsLabel::Key => text.literal("Clé"),
        RepoSettingsLabel::KeyHint => text.literal(
            "Une clé ne peut appartenir qu’à un seul dépôt ou compte\u{202f}; générez une clé distincte pour chacun.",
        ),
        RepoSettingsLabel::AllowWrite => text.literal("Autoriser l’accès en écriture"),
        RepoSettingsLabel::AllowWriteHint => text.literal("La clé peut pousser vers ce dépôt en plus de le cloner."),
        RepoSettingsLabel::AddDeployKey => text.literal("Ajouter la clé de déploiement"),
        RepoSettingsLabel::Keys => text.literal("Clés"),
        RepoSettingsLabel::NoDeployKeys => text.literal("Aucune clé de déploiement"),
        RepoSettingsLabel::NoDeployKeysText => text.literal(
            "Ajoutez une clé pour permettre à un serveur ou à un job de CI de cloner ce dépôt en SSH.",
        ),
        RepoSettingsLabel::Delete => text.literal("Supprimer"),
        RepoSettingsLabel::Mirror => text.literal("Miroir"),
        RepoSettingsLabel::MirrorHint => text.literal(
            "L’amont, l’intervalle de récupération et le résultat de la dernière récupération.",
        ),
        RepoSettingsLabel::MirrorLead => text.literal(
            "Ce dépôt récupère les branches et les tags de son amont et n’accepte pas les push.",
        ),
        RepoSettingsLabel::Upstream => text.literal("Amont"),
        RepoSettingsLabel::LastFetch => text.literal("Dernière récupération"),
        RepoSettingsLabel::Failed => text.literal("Échec"),
        RepoSettingsLabel::Ok => text.literal("OK"),
        RepoSettingsLabel::NotYet => text.literal("Pas encore"),
        RepoSettingsLabel::NextFetch => text.literal("Prochaine récupération"),
        RepoSettingsLabel::Error => text.literal("Erreur"),
        RepoSettingsLabel::FetchNow => text.literal("Récupérer maintenant"),
        RepoSettingsLabel::FetchEvery => text.literal("Récupérer toutes les"),
        RepoSettingsLabel::Minutes => text.literal("minutes"),
        RepoSettingsLabel::Save => text.literal("Enregistrer"),
        RepoSettingsLabel::Saved => text.literal("Enregistré."),
        RepoSettingsLabel::FetchQueued => text.literal(
            "Une récupération est en file d’attente\u{202f}; actualisez dans un instant pour voir son résultat.",
        ),
        RepoSettingsLabel::PushQueued => text.literal(
            "Un push est en file d’attente\u{202f}; actualisez dans un instant pour voir son résultat.",
        ),
        RepoSettingsLabel::StopMirroring => text.literal("Arrêter la mise en miroir"),
        RepoSettingsLabel::StopHint => text.literal(
            "Le dépôt conserve son contenu, cesse les récupérations et accepte les push comme n’importe quel autre.",
        ),
        RepoSettingsLabel::PushMirrors => text.literal("Miroirs push"),
        RepoSettingsLabel::PushMirrorsHint => text.literal(
            "Des dépôts distants qui reçoivent les branches et les tags de ce dépôt après chaque push.",
        ),
        RepoSettingsLabel::PushMirrorsLead => text.literal(
            "Des dépôts distants qui reçoivent les branches et les tags de ce dépôt. Chaque push écrase les branches et \
             les tags du dépôt distant et supprime ceux qui n’existent plus ici.",
        ),
        RepoSettingsLabel::PushMirrorsDisabled => text.literal("Les miroirs push sont désactivés"),
        RepoSettingsLabel::PushMirrorsDisabledText => text.literal(
            "Un administrateur de cette instance a désactivé les miroirs push.",
        ),
        RepoSettingsLabel::NoPushMirrors => text.literal("Aucun miroir push pour l’instant"),
        RepoSettingsLabel::NoPushMirrorsText => text.literal(
            "Ajoutez un dépôt distant ci-dessous pour conserver une copie de ce dépôt ailleurs.",
        ),
        RepoSettingsLabel::Paused => text.literal("En pause"),
        RepoSettingsLabel::OnPush => text.literal("À chaque push"),
        RepoSettingsLabel::LastPush => text.literal("Dernier push"),
        RepoSettingsLabel::NotPushedYet => text.literal("Pas encore poussé"),
        RepoSettingsLabel::As => text.literal("en tant que"),
        RepoSettingsLabel::SecretConfigured => text.literal("secret configuré"),
        RepoSettingsLabel::PushNow => text.literal("Pousser maintenant"),
        RepoSettingsLabel::Edit => text.literal("Modifier"),
        RepoSettingsLabel::Remove => text.literal("Retirer"),
        RepoSettingsLabel::AddPushMirrorTitle => text.literal("Ajouter un miroir push"),
        RepoSettingsLabel::AddPushMirror => text.literal("Ajouter le miroir push"),
        RepoSettingsLabel::PushMirrorAdded => text.literal(
            "Miroir push ajouté. Le premier push est en cours\u{202f}; actualisez dans un instant pour voir son résultat.",
        ),
        RepoSettingsLabel::PushMirrorRemoved => text.literal(
            "Miroir push retiré. Le dépôt distant conserve ce qu’il contient.",
        ),
        RepoSettingsLabel::EditPushMirror => text.literal("Modifier le miroir push"),
        RepoSettingsLabel::PushMirror => text.literal("Miroir push"),
        RepoSettingsLabel::Enabled => text.literal("Activé"),
        RepoSettingsLabel::EnabledHint => text.literal(
            "Un miroir en pause conserve ses paramètres et ne reçoit aucun push.",
        ),
        RepoSettingsLabel::Cancel => text.literal("Annuler"),
        RepoSettingsLabel::RemoteUrl => text.literal("URL du dépôt distant"),
        RepoSettingsLabel::UrlHintStart => text.literal("Une URL"),
        RepoSettingsLabel::UrlHintOr => text.literal("ou"),
        RepoSettingsLabel::UrlHintEnd => text.literal("sans nom d’utilisateur ni mot de passe."),
        RepoSettingsLabel::UserName => text.literal("Nom d’utilisateur"),
        RepoSettingsLabel::PasswordOrToken => text.literal("Mot de passe ou jeton"),
        RepoSettingsLabel::SecretPlaceholder => text.literal("Configuré — laissez vide pour le conserver"),
        RepoSettingsLabel::SecretHint => text.literal(
            "Envoyé au dépôt distant par authentification HTTP Basic. Il ne pourra plus être affiché.",
        ),
        RepoSettingsLabel::SecretHintKept => text.literal(
            "Envoyé au dépôt distant par authentification HTTP Basic. Il ne pourra plus être affiché et devra être \
             saisi de nouveau si le dépôt distant change d’hôte.",
        ),
        RepoSettingsLabel::PushAfterPush => text.literal("Pousser après chaque push"),
        RepoSettingsLabel::PushAfterPushHint => text.literal(
            "Quelques secondes après chaque modification de ce dépôt. Chaque miroir est aussi poussé selon un calendrier.",
        ),
    }
    RepoSettingsMessage<'a> => MessageWriter |text| {
        RepoSettingsMessage::AtLeastMinutes { minutes } => {
            text.literal("Au moins ");
            text.number(minutes);
            minutes_noun(text, minutes);
        }
        RepoSettingsMessage::PushSchedule { minutes } => {
            text.literal("Chaque miroir est poussé au moins une fois par période de ");
            text.number(minutes);
            minutes_noun(text, minutes);
        }
        RepoSettingsMessage::DeleteKey { title } => {
            text.literal("Supprimer ");
            text.argument(title);
        }
    }
}

/// « minute. » or « minutes. », by `minutes`.
fn minutes_noun(text: &mut MessageWriter, minutes: u64) {
    text.literal(match text.plural(minutes) {
        Plural::One => " minute.",
        _ => " minutes.",
    });
}
