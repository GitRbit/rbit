//! French (`fr`) entries for `catalog/settings/security.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::security::{SecurityMessage, SecurityText};

entries! { Fr:
    SecurityMessage => MessageWriter |text| {
        SecurityMessage::Title => text.literal("Sécurité"),
        SecurityMessage::Intro => text.literal(
            "Protégez votre compte avec une clé d’accès ou une application d’authentification, et gardez un œil sur les \
             endroits où vous êtes connecté.",
        ),
        SecurityMessage::TwoFactor => text.literal("Authentification à deux facteurs"),
        SecurityMessage::On => text.literal("Activée"),
        SecurityMessage::Off => text.literal("Désactivée"),
        SecurityMessage::EnabledLead => text.literal(
            "La connexion demande une clé d’accès ou un code après votre mot de passe. Git en HTTPS nécessite un",
        ),
        SecurityMessage::TokenLink => text.literal("jeton d’accès personnel"),
        SecurityMessage::EnabledTail => text.literal("\u{202f}; votre mot de passe n’y fonctionne plus."),
        SecurityMessage::Disabled => text.literal(
            "Ajoutez une seconde étape à la connexion\u{a0}: une clé d’accès, une application d’authentification, ou les deux.",
        ),
        SecurityMessage::Passkeys => text.literal("Clés d’accès et clés de sécurité"),
        SecurityMessage::PasskeysText => text.literal(
            "Confirmez vos connexions avec votre empreinte digitale, votre visage, le code PIN de votre appareil, votre \
             téléphone ou une clé matérielle comme une YubiKey.",
        ),
        SecurityMessage::Remove => text.literal("Retirer"),
        SecurityMessage::OnlyFactor => text.literal(
            "C’est votre seul second facteur\u{a0}: le retirer désactive l’authentification à deux facteurs et supprime \
             vos codes de récupération.",
        ),
        SecurityMessage::PasskeyGone => text.literal("Vous ne pourrez plus vous connecter avec cette clé d’accès."),
        SecurityMessage::Name => text.literal("Nom"),
        SecurityMessage::PasskeyPlaceholder => text.literal("p. ex. MacBook ou YubiKey"),
        SecurityMessage::AddPasskey => text.literal("Ajouter une clé d’accès"),
        SecurityMessage::PasskeyUnsupported => text.literal(
            "L’ajout d’une clé d’accès nécessite un navigateur qui les prend en charge, avec JavaScript activé.",
        ),
        SecurityMessage::PasskeyUrlLead => text.literal("Les clés d’accès nécessitent que"),
        SecurityMessage::PasskeyUrlTail => text.literal("soit défini sur l’adresse de ce site."),
        SecurityMessage::Authenticator => text.literal("Application d’authentification"),
        SecurityMessage::AuthenticatorText => text.literal(
            "Utilisez les codes à 6\u{a0}chiffres d’une application comme 1Password, Aegis ou Google Authenticator.",
        ),
        SecurityMessage::SetUp => text.literal("Configurer"),
        SecurityMessage::RemoveAuthenticator => text.literal("Retirer l’application d’authentification"),
        SecurityMessage::PasswordAlone => text.literal(
            "Votre compte ne sera plus protégé que par votre mot de passe, et vos codes de récupération seront supprimés.",
        ),
        SecurityMessage::PasskeysStay => text.literal("Vos clés d’accès restent actives comme second facteur."),
        SecurityMessage::RecoveryCodes => text.literal("Codes de récupération"),
        SecurityMessage::Regenerate => text.literal("Générer de nouveaux codes de récupération"),
        SecurityMessage::CodesStop => text.literal("Vos codes de récupération actuels cesseront de fonctionner."),
        SecurityMessage::GenerateCodes => text.literal("Générer de nouveaux codes"),
        SecurityMessage::SignInMethods => text.literal("Méthodes de connexion"),
        SecurityMessage::Password => text.literal("Mot de passe"),
        SecurityMessage::NoPassword => text.literal("Aucun mot de passe"),
        SecurityMessage::WithPassword => text.literal("Connectez-vous avec votre mot de passe ou avec un compte associé."),
        SecurityMessage::LinkedOnlyLead => text.literal(
            "Votre compte n’a pas de mot de passe\u{a0}: vous vous connectez avec un compte associé. Pour en définir un, \
             demandez un",
        ),
        SecurityMessage::ResetLink => text.literal("lien de réinitialisation du mot de passe"),
        SecurityMessage::LinkedOnlyMiddle => text.literal(". Git en HTTPS nécessite un"),
        SecurityMessage::LinkedOnlyTail => text.literal("."),
        SecurityMessage::NoLongerAvailable => text.literal("N’est plus disponible"),
        SecurityMessage::Unlink => text.literal("Dissocier"),
        SecurityMessage::OnlyWay => text.literal("Seul moyen de connexion"),
        SecurityMessage::NotLinked => text.literal("Non associé"),
        SecurityMessage::Link => text.literal("Associer"),
        SecurityMessage::Sessions => text.literal("Sessions"),
        SecurityMessage::SignOutOthers => text.literal("Se déconnecter partout ailleurs"),
        SecurityMessage::ThisDevice => text.literal("Cet appareil"),
        SecurityMessage::SignOut => text.literal("Se déconnecter"),
        SecurityMessage::RecentActivity => text.literal("Activité récente"),
        SecurityMessage::NoActivity => text.literal("Aucune activité pour l’instant"),
        SecurityMessage::NoActivityText => text.literal(
            "Les connexions et les modifications de sécurité de votre compte apparaîtront ici.",
        ),
        SecurityMessage::SetupTitle => text.literal("Configurer une application d’authentification"),
        SecurityMessage::SetupIntro => text.literal(
            "Scannez le code QR avec votre application d’authentification, puis saisissez le code à 6\u{a0}chiffres \
             qu’elle affiche.",
        ),
        SecurityMessage::CantScan => text.literal("Impossible de scanner\u{202f}? Saisissez plutôt cette clé"),
        SecurityMessage::CopyKey => text.literal("Copier la clé"),
        SecurityMessage::Copy => text.literal("Copier"),
        SecurityMessage::TimeBased => text.literal("Basé sur le temps, 6\u{a0}chiffres, toutes les 30\u{a0}secondes."),
        SecurityMessage::ConfirmCode => text.literal("2. Confirmer un code"),
        SecurityMessage::AuthenticationCode => text.literal("Code d’authentification"),
        SecurityMessage::Cancel => text.literal("Annuler"),
        SecurityMessage::Enable => text.literal("Activer"),
        SecurityMessage::TwoFactorOn => text.literal("L’authentification à deux facteurs est activée."),
        SecurityMessage::SaveCodes => text.literal("Enregistrez ces codes maintenant"),
        SecurityMessage::CodesText => text.literal(
            "Chaque code vous connecte une fois si vous perdez vos clés d’accès ou votre application d’authentification. \
             Ils ne seront plus affichés\u{a0}: conservez-les en lieu sûr, par exemple dans un gestionnaire de mots de passe.",
        ),
        SecurityMessage::SavedCodes => text.literal("J’ai enregistré mes codes"),
        SecurityMessage::AppOrRecoveryCode => text.literal("Code d’authentification ou de récupération"),
        SecurityMessage::RecoveryCode => text.literal("Code de récupération"),
        SecurityMessage::PasskeyInstead => text.literal("Ou confirmez avec l’une de vos clés d’accès plutôt qu’avec un code."),
        SecurityMessage::AuthenticatorRemoved => text.literal("Application d’authentification retirée."),
        SecurityMessage::AuthenticatorAdded => text.literal("Application d’authentification ajoutée."),
        SecurityMessage::PasskeyAdded => text.literal("Clé d’accès ajoutée."),
        SecurityMessage::PasskeyRemoved => text.literal("Clé d’accès retirée."),
        SecurityMessage::SessionRevoked => text.literal("Session déconnectée."),
        SecurityMessage::OtherSessionsRevoked => text.literal("Toutes les autres sessions ont été déconnectées."),
        SecurityMessage::SetupCancelled => text.literal("Configuration de l’authentification à deux facteurs annulée."),
        SecurityMessage::IdentityLinked => text.literal("Compte associé. Vous pouvez désormais vous connecter avec."),
        SecurityMessage::IdentityUnlinked => text.literal("Compte dissocié."),
    }
    SecurityText<'a> => MessageWriter |text| {
        SecurityText::AddToApp(site) => {
            text.literal("1. Ajouter ");
            text.argument(site);
            text.literal(" à votre application");
        }
        SecurityText::UnlinkLabel(provider) => {
            text.literal("Dissocier ");
            text.argument(provider);
        }
        SecurityText::SignOutLabel(device) => {
            text.literal("Déconnecter ");
            text.argument(device);
        }
        SecurityText::WithPasskey(action) => {
            text.argument(action);
            text.literal(" avec une clé d’accès");
        }
    }
}
