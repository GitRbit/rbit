//! French (`fr`) entries for `catalog/settings/config.rs`.
//!
//! The schema text (`ConfigText`) is too large for one file: its arms live in the sibling
//! modules, each declaring a share of the table with `config_table!`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::{ConfigMessage, ConfigPageText, ConfigText};

/// Declares a sibling module's share of the `ConfigText` table as a `write` function that
/// returns `false` for the messages it has no arm for.
macro_rules! config_table {
    ($family:ident<$lifetime:lifetime> => $writer:ident |$text:ident| { $($arms:tt)* }) => {
        #[allow(
            clippy::match_same_arms,
            clippy::too_many_lines,
            reason = "a table lists one arm per message of the schema"
        )]
        pub(super) fn write<$lifetime>(message: $family<$lifetime>, $text: &mut $writer) -> bool {
            match message {
                $($arms)*
                _ => return false,
            }
            true
        }
    };
}

mod fields_a;
mod fields_b;
mod sections;

entries! { Fr:
    ConfigText<'a> => MessageWriter |text| {
        message => return fields_a::write(message, text) || fields_b::write(message, text) || sections::write(message, text),
    }
    ConfigMessage => MessageWriter |text| {
        ConfigMessage::Title => text.literal("Configuration"),
        ConfigMessage::Intro => text.literal(
            "Gérez votre instance auto-hébergée avec des paramètres classés par fonctionnalité.",
        ),
        ConfigMessage::RestartServer => text.literal("Redémarrer le serveur"),
        ConfigMessage::Saved => text.literal(
            "Paramètres enregistrés. Redémarrez le serveur pour appliquer les modifications de déploiement.",
        ),
        ConfigMessage::RestartRequired => text.literal("Redémarrage requis"),
        ConfigMessage::EnvironmentWins => text.literal("Les variables d’environnement ont la priorité."),
        ConfigMessage::LivePolicies => text.literal("Image de marque et règles — appliquées immédiatement"),
        ConfigMessage::SettingsSection => text.literal("Section de paramètres"),
        ConfigMessage::OpenSection => text.literal("Ouvrir la section"),
        ConfigMessage::Sections => text.literal("Sections de configuration"),
        ConfigMessage::LiveOverrides => text.literal(
            "L’image de marque et les règles d’inscription enregistrées en direct remplacent ces valeurs par défaut \
             du déploiement.",
        ),
        ConfigMessage::IdentityProviders => text.literal("Fournisseurs d’identité"),
        ConfigMessage::ProvidersHint => text.literal(
            "Ajoutez et retirez d’abord les cartes de fournisseurs, puis saisissez les nouveaux identifiants et \
             enregistrez. Les modifications des fournisseurs restent dans ce formulaire tant que vous n’enregistrez pas \
             la section.",
        ),
        ConfigMessage::CallbackLead => text.literal(
            "Enregistrez une URL de rappel auprès de chaque fournisseur\u{a0}: votre URL publique suivie de",
        ),
        ConfigMessage::CallbackTail => text.literal(
            ", en remplaçant PROVIDER_ID par l’ID ci-dessous. Pour OpenID Connect, définissez l’URL de l’émetteur\u{202f}; \
             pour GitHub Enterprise, l’URL Enterprise.",
        ),
        ConfigMessage::NoProviders => text.literal(
            "Aucun fournisseur de connexion externe n’est configuré. La connexion locale est disponible.",
        ),
        ConfigMessage::RemoveProvider => text.literal("Retirer le fournisseur"),
        ConfigMessage::AddProvider => text.literal("Ajouter un fournisseur d’identité"),
        ConfigMessage::NewProvider => text.literal("Nouveau fournisseur de connexion"),
        ConfigMessage::OnlySection => text.literal("Seule cette section sera mise à jour."),
        ConfigMessage::Recovery => text.literal("Récupération et valeurs par défaut du déploiement"),
        ConfigMessage::RecoveryLead => text.literal("Si les paramètres enregistrés empêchent le démarrage, définissez"),
        ConfigMessage::RecoveryTail => text.literal(
            "dans l’environnement du serveur, redémarrez, puis corrigez ou supprimez les remplacements ici.",
        ),
        ConfigMessage::RemovingOverrides => text.literal(
            "La suppression des remplacements rétablit la configuration du fichier et de l’environnement au prochain \
             redémarrage. L’image de marque et les règles en direct sont conservées.",
        ),
        ConfigMessage::RemoveOverrides => text.literal("Supprimer tous les remplacements de déploiement"),
        ConfigMessage::Optional => text.literal("(facultatif)"),
        ConfigMessage::OnePerLine => text.literal("Une entrée par ligne."),
        ConfigMessage::ProviderDefaults => text.literal("Utiliser les valeurs par défaut du fournisseur"),
        ConfigMessage::Configured => text.literal("Configuré — laissez vide pour le conserver"),
        ConfigMessage::NotConfigured => text.literal("Non configuré"),
        ConfigMessage::SecretHint => text.literal(
            "Laissez vide pour conserver l’identifiant actuel. Les nouveaux identifiants ne sont jamais réaffichés après \
             une erreur\u{a0}: saisissez-les de nouveau en corrigeant le formulaire. Protégez la base de données de \
             l’instance et ses sauvegardes.",
        ),
        ConfigMessage::ClearSecret => text.literal("Effacer cet identifiant"),
        ConfigMessage::BlankDefault => text.literal("Laissez vide pour utiliser la valeur par défaut."),
    }
    ConfigPageText<'a> => MessageWriter |text| {
        ConfigPageText::SaveSection(section) => {
            text.literal("Enregistrer les paramètres de la section ");
            text.argument(section);
        }
    }
}
