//! French (`fr`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;

entries! { Fr:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("Réinitialiser votre mot de passe"),
        RecoveryMessage::ForgotLede => text.literal(
            "Saisissez l’adresse e-mail de votre compte et nous vous enverrons un lien pour choisir un nouveau mot de passe.",
        ),
        RecoveryMessage::ForgotWithoutEmail => text.literal(
            "L’e-mail n’est pas disponible sur ce serveur\u{a0}: les mots de passe ne peuvent donc pas être réinitialisés \
             par e-mail. Demandez à un administrateur de vous définir un nouveau mot de passe.",
        ),
        RecoveryMessage::CheckEmail => text.literal("Consultez vos e-mails"),
        RecoveryMessage::LinkSent => text.literal(
            "Si un compte utilise cette adresse, nous lui avons envoyé un lien pour choisir un nouveau mot de passe. Le \
             lien ne fonctionne qu’une fois et expire au bout d’une heure.",
        ),
        RecoveryMessage::NothingArrived => text.literal("Rien reçu\u{202f}? Vérifiez votre dossier de courrier indésirable, ou"),
        RecoveryMessage::TryAgain => text.literal("réessayez"),
        RecoveryMessage::SendResetLink => text.literal("Envoyer le lien de réinitialisation"),
        RecoveryMessage::RememberedIt => text.literal("Il vous est revenu\u{202f}?"),
        RecoveryMessage::RequestNewLink => text.literal("Demander un nouveau lien"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal(
            "L’e-mail n’est pas disponible sur ce serveur\u{a0}: aucun nouveau lien ne peut donc être envoyé. Demandez à \
             un administrateur de vous définir un nouveau mot de passe.",
        ),
        RecoveryMessage::ChooseNewPassword => text.literal("Choisir un nouveau mot de passe"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("Pour ");
            text.argument(username);
            text.literal(". Vous l’utiliserez désormais pour vous connecter.");
        }
        RecoveryMessage::NewPassword => text.literal("Nouveau mot de passe"),
        RecoveryMessage::ConfirmNewPassword => text.literal("Confirmer le nouveau mot de passe"),
        RecoveryMessage::ChangePassword => text.literal("Changer le mot de passe"),
        RecoveryMessage::BackToSignIn => text.literal("Retour à la connexion"),
        RecoveryMessage::PasswordChanged => text.literal("Mot de passe modifié"),
        RecoveryMessage::PasswordChangedText => text.literal("Votre nouveau mot de passe est défini. Utilisez-le lors de votre prochaine connexion."),
        RecoveryMessage::LinkExpired => text.literal("Ce lien a expiré"),
        RecoveryMessage::ResetLinkExpiredText => text.literal(
            "Les liens de réinitialisation ne fonctionnent qu’une fois et expirent au bout d’une heure. Demandez-en un \
             nouveau pour continuer.",
        ),
        RecoveryMessage::AccountSuspended => text.literal("Ce compte est suspendu"),
        RecoveryMessage::AccountSuspendedText => text.literal("Son mot de passe ne peut pas être réinitialisé. Contactez un administrateur."),
        RecoveryMessage::EmailVerified => text.literal("Adresse e-mail vérifiée"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("Merci d’avoir confirmé ");
            text.argument(address);
            text.literal(".");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal(
            "Les liens de vérification ne fonctionnent qu’une fois, expirent au bout de deux jours et cessent de \
             fonctionner si l’adresse change. Connectez-vous et envoyez-en un nouveau depuis les paramètres de votre compte.",
        ),
        RecoveryMessage::GoToAccountSettings => text.literal("Accéder aux paramètres du compte"),
        RecoveryMessage::SignIn => text.literal("Se connecter"),
    }
}
