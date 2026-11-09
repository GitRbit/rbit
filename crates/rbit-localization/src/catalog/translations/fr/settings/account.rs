//! French (`fr`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};
use crate::plural::Plural;

entries! { Fr:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("Mettre à jour votre mot de passe"),
        AccountMessage::Title => text.literal("Compte"),
        AccountMessage::Intro => text.literal(
            "Votre façon de vous connecter. Toute modification demande votre mot de passe actuel.",
        ),
        AccountMessage::Username => text.literal("Nom d’utilisateur"),
        AccountMessage::UsernameUpdated => text.literal("Nom d’utilisateur mis à jour."),
        AccountMessage::UsernameHint => text.literal(
            "Les liens vers votre profil et les remotes Git qui utilisent votre ancien nom continuent de fonctionner \
             jusqu’à ce que quelqu’un prenne ce nom. Mettez à jour vos liens et vos remotes. Connectez-vous avec votre \
             nouveau nom d’utilisateur ou votre adresse e-mail.",
        ),
        AccountMessage::CurrentPassword => text.literal("Mot de passe actuel"),
        AccountMessage::ChangeUsername => text.literal("Changer de nom d’utilisateur"),
        AccountMessage::VerificationSent => text.literal(
            "E-mail de vérification envoyé. Consultez votre boîte de réception.",
        ),
        AccountMessage::Verified => text.literal("Vérifiée"),
        AccountMessage::Unverified => text.literal("Non vérifiée"),
        AccountMessage::VerifiedText => text.literal("Vous avez confirmé que cette adresse vous appartient."),
        AccountMessage::PendingText => text.literal("Confirmez l’adresse avec le lien que nous vous avons envoyé par e-mail."),
        AccountMessage::UnavailableText => text.literal(
            "L’e-mail n’est pas disponible sur ce serveur\u{a0}: l’adresse ne peut donc pas être vérifiée.",
        ),
        AccountMessage::Resend => text.literal("Renvoyer l’e-mail de vérification"),
        AccountMessage::EmailAddress => text.literal("Adresse e-mail"),
        AccountMessage::EmailUpdated => text.literal("Adresse e-mail mise à jour."),
        AccountMessage::Email => text.literal("E-mail"),
        AccountMessage::EmailHintLead => text.literal(
            "Vous pouvez vous connecter avec cette adresse ou avec votre nom d’utilisateur. Actuellement",
        ),
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("Mettre à jour l’adresse e-mail"),
        AccountMessage::Password => text.literal("Mot de passe"),
        AccountMessage::PasswordChanged => text.literal("Mot de passe modifié."),
        AccountMessage::NewPassword => text.literal("Nouveau mot de passe"),
        AccountMessage::ConfirmPassword => text.literal("Confirmer le nouveau mot de passe"),
        AccountMessage::ChangePassword => text.literal("Changer le mot de passe"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal(
                "Un administrateur a créé votre compte avec un mot de passe initial. Choisissez un nouveau mot de passe \
                 avant de continuer vers ",
            );
            text.argument(site);
            text.literal(".");
        }
        AccountText::PasswordHint(length) => {
            text.literal("Au moins ");
            text.number(length);
            text.literal(match text.plural(length) {
                Plural::One => " caractère.",
                _ => " caractères.",
            });
            text.literal(" Quelques mots sans rapport entre eux forment un mot de passe robuste et facile à retenir.");
        }
    }
}
