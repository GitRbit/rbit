//! French (`fr`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { Fr:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.literal("Les workflows situés dans ");
            text.argument(directory);
            text.literal(" s’exécutent sur des runners lorsque leurs événements se produisent dans ");
            text.argument(repository);
            text.literal(".");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("Secrets et variables des workflows des dépôts de ");
            text.argument(organization);
            text.literal(". Un secret ou une variable de même nom défini dans un dépôt l’emporte.");
        }
        ActionsSettingsMessage::AccountIntro => text.literal(
            "Secrets et variables des workflows de vos propres dépôts. Un secret ou une variable de même nom défini dans \
             un dépôt l’emporte.",
        ),
        ActionsSettingsMessage::General => text.literal("Général"),
        ActionsSettingsMessage::RunWorkflows => text.literal("Exécuter les workflows dans ce dépôt"),
        ActionsSettingsMessage::RunWorkflowsHint => text.literal(
            "Désactivé\u{a0}: aucune nouvelle exécution ne démarre et l’onglet Actions est masqué. Les exécutions \
             précédentes et leurs journaux sont conservés.",
        ),
        ActionsSettingsMessage::DefaultPermission => text.literal("Autorisation par défaut du jeton des workflows"),
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("La clé ");
            text.argument(key);
            text.literal(
                " d’un workflow peut seulement la réduire. Les exécutions issues de forks ou d’auteurs externes reçoivent \
                 toujours un jeton en lecture seule.",
            );
        }
        ActionsSettingsMessage::PermissionRead => text.literal("Lecture"),
        ActionsSettingsMessage::PermissionReadHint => text.literal(
            "Les workflows peuvent lire le contenu, les tickets et les pull requests du dépôt.",
        ),
        ActionsSettingsMessage::PermissionWrite => text.literal("Lecture et écriture"),
        ActionsSettingsMessage::PermissionWriteHint => text.literal(
            "Les workflows peuvent aussi pousser et commenter les tickets et les pull requests. La protection de branche \
             s’applique toujours.",
        ),
        ActionsSettingsMessage::SaveChanges => text.literal("Enregistrer les modifications"),
        ActionsSettingsMessage::RepositoryRunnersHint => text.literal(
            "Les runners de ce dépôt, ceux dont il hérite et les jetons d’enregistrement.",
        ),
        ActionsSettingsMessage::OrganizationRunnersHint => text.literal(
            "Les runners de tous les dépôts de l’organisation et les jetons d’enregistrement.",
        ),
        ActionsSettingsMessage::AccountRunnersHint => text.literal(
            "Les runners de tous les dépôts de votre compte et les jetons d’enregistrement.",
        ),
        ActionsSettingsMessage::EnvironmentsHint => text.literal(
            "Environnements de déploiement\u{a0}: relecteurs requis, branches autorisées, ainsi que leurs propres \
             secrets et variables.",
        ),
        ActionsSettingsMessage::TrustedTitle => text.literal("Auteurs de pull requests de confiance"),
        ActionsSettingsMessage::TrustedIntro => text.literal(
            "Leurs pull requests issues de forks s’exécutent sans attendre d’approbation, toujours avec un jeton en \
             lecture seule et sans secrets ni variables. Les auteurs sont ajoutés depuis l’approbation d’une exécution, \
             avec «\u{a0}toujours exécuter les pull requests de cet auteur\u{a0}».",
        ),
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("Aucun auteur de confiance"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => text.literal(
            "Les pull requests issues de forks et de personnes sans accès en écriture attendent qu’une personne ayant \
             accès en écriture approuve chaque exécution.",
        ),
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("De confiance ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("De confiance ");
            text.argument(since);
            text.literal(", par ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.literal("Ne plus faire confiance à ");
            text.argument(name);
        }
        ActionsSettingsMessage::Remove => text.literal("Retirer"),
        ActionsSettingsMessage::Saved => text.literal("Paramètres enregistrés."),
        ActionsSettingsMessage::SecretSaved => text.literal("Secret enregistré."),
        ActionsSettingsMessage::SecretDeleted => text.literal("Secret supprimé."),
        ActionsSettingsMessage::VariableSaved => text.literal("Variable enregistrée."),
        ActionsSettingsMessage::VariableDeleted => text.literal("Variable supprimée."),
        ActionsSettingsMessage::Untrusted => text.literal("Les pull requests de cet auteur nécessitent de nouveau une approbation."),
        ActionsSettingsMessage::RepositoryAccess => text.literal("Accès aux dépôts"),
        ActionsSettingsMessage::AllRepositories => text.literal("Tous les dépôts"),
        ActionsSettingsMessage::AllRepositoriesHint => text.literal(
            "Tous les dépôts de ce propriétaire, y compris ceux créés ultérieurement.",
        ),
        ActionsSettingsMessage::SelectedRepositories => text.literal("Dépôts sélectionnés"),
        ActionsSettingsMessage::SelectedRepositoriesHint => text.literal("Uniquement les dépôts cochés ci-dessous."),
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("Ce propriétaire n’a encore aucun dépôt."),
    }
}
