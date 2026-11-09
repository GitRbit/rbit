//! French (`fr`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { Fr:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("Secrets"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("Les workflows les lisent avec ");
            text.argument(expression);
            text.literal(
                ". Les valeurs sont chiffrées et ne peuvent plus être affichées, seulement remplacées. Les exécutions \
                 issues de forks ou d’auteurs sans accès en écriture ne les reçoivent jamais\u{202f}; toute personne \
                 pouvant pousser peut les lire au moyen d’un workflow.",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("Les secrets ne peuvent être ni stockés ni utilisés\u{a0}: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("Aucun secret"),
        ActionsSecretsMessage::NoSecretsHint => text.literal(
            "Ajoutez un secret pour les jetons et les clés dont vos workflows ont besoin.",
        ),
        ActionsSecretsMessage::CannotDecrypt => text.literal("Impossible à déchiffrer — définissez-le de nouveau"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("Mis à jour ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("Tous les dépôts"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dépôt sélectionné",
                _ => " dépôts sélectionnés",
            });
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("Supprimer le secret ");
            text.argument(name);
        }
        ActionsSecretsMessage::Delete => text.literal("Supprimer"),
        ActionsSecretsMessage::ReplaceValue => text.literal("Remplacer la valeur"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.literal("Nouvelle valeur de ");
            text.argument(name);
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("Remplacer le secret"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("Choisir les dépôts"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("Les dépôts dont les workflows reçoivent ");
            text.argument(name);
            text.literal(", sans modifier sa valeur.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("Enregistrer les dépôts"),
        ActionsSecretsMessage::NewSecret => text.literal("Nouveau secret"),
        ActionsSecretsMessage::Name => text.literal("Nom"),
        ActionsSecretsMessage::SecretNameHint => text.literal(
            "Lettres, chiffres et tirets bas, sans commencer par un chiffre ni par GITHUB_, GITEA_, FORGEJO_ ou RBIT_. \
             Les noms sont enregistrés en majuscules\u{202f}; un nom existant est remplacé.",
        ),
        ActionsSecretsMessage::Value => text.literal("Valeur"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("Au plus ");
            text.number(kib);
            text.literal("\u{a0}Kio. Elle ne pourra plus être affichée.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("Ajouter le secret"),
        ActionsSecretsMessage::VariablesTitle => text.literal("Variables"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("Les workflows les lisent avec ");
            text.argument(expression);
            text.literal(
                ". Elles ne sont pas secrètes\u{a0}: toute personne pouvant modifier un workflow peut les afficher. Les \
                 exécutions issues de forks ou d’auteurs sans accès en écriture ne les reçoivent jamais.",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("Aucune variable"),
        ActionsSecretsMessage::NoVariablesHint => text.literal(
            "Ajoutez une variable pour la configuration que partagent vos workflows, comme une région ou une version.",
        ),
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("Supprimer la variable ");
            text.argument(name);
        }
        ActionsSecretsMessage::Edit => text.literal("Modifier"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.literal("Valeur de ");
            text.argument(name);
        }
        ActionsSecretsMessage::SaveVariable => text.literal("Enregistrer la variable"),
        ActionsSecretsMessage::NewVariable => text.literal("Nouvelle variable"),
        ActionsSecretsMessage::VariableNameHint => text.literal(
            "Les mêmes règles que pour les noms de secrets\u{202f}; un nom existant est remplacé.",
        ),
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("Au plus ");
            text.number(kib);
            text.literal("\u{a0}Kio.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("Ajouter la variable"),
    }
}
