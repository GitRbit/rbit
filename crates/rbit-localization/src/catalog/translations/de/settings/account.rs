//! German (`de`) entries for `catalog/settings/account.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::account::{AccountMessage, AccountText};

entries! { De:
    AccountMessage => MessageWriter |text| {
        AccountMessage::UpdatePassword => text.literal("Passwort aktualisieren"),
        AccountMessage::Title => text.literal("Konto"),
        AccountMessage::Intro => {
            text.literal("Wie Sie sich anmelden. Für Änderungen wird Ihr aktuelles Passwort abgefragt.");
        }
        AccountMessage::Username => text.literal("Benutzername"),
        AccountMessage::UsernameUpdated => text.literal("Benutzername aktualisiert."),
        AccountMessage::UsernameHint => {
            text.literal(
                "Profil-Links und Git-Remotes mit Ihrem alten Namen funktionieren weiter, bis jemand anderes diesen \
                 Namen übernimmt. Aktualisieren Sie Ihre Links und Remotes. Melden Sie sich mit Ihrem neuen \
                 Benutzernamen oder Ihrer E-Mail-Adresse an.",
            );
        }
        AccountMessage::CurrentPassword => text.literal("Aktuelles Passwort"),
        AccountMessage::ChangeUsername => text.literal("Benutzernamen ändern"),
        AccountMessage::VerificationSent => {
            text.literal("Bestätigungs-E-Mail gesendet. Sehen Sie in Ihrem Posteingang nach.");
        }
        AccountMessage::Verified => text.literal("Bestätigt"),
        AccountMessage::Unverified => text.literal("Nicht bestätigt"),
        AccountMessage::VerifiedText => text.literal("Sie haben bestätigt, dass diese Adresse Ihnen gehört."),
        AccountMessage::PendingText => {
            text.literal("Bestätigen Sie die Adresse über den Link, den wir Ihnen per E-Mail gesendet haben.");
        }
        AccountMessage::UnavailableText => {
            text.literal(
                "E-Mail ist auf diesem Server nicht verfügbar, daher kann die Adresse nicht bestätigt werden.",
            );
        }
        AccountMessage::Resend => text.literal("Bestätigungs-E-Mail erneut senden"),
        AccountMessage::EmailAddress => text.literal("E-Mail-Adresse"),
        AccountMessage::EmailUpdated => text.literal("E-Mail-Adresse aktualisiert."),
        AccountMessage::Email => text.literal("E-Mail"),
        AccountMessage::EmailHintLead => {
            text.literal("Sie können sich mit dieser Adresse oder Ihrem Benutzernamen anmelden. Derzeit");
        }
        AccountMessage::EmailHintTail => text.literal("."),
        AccountMessage::UpdateEmail => text.literal("E-Mail aktualisieren"),
        AccountMessage::Password => text.literal("Passwort"),
        AccountMessage::PasswordChanged => text.literal("Passwort geändert."),
        AccountMessage::NewPassword => text.literal("Neues Passwort"),
        AccountMessage::ConfirmPassword => text.literal("Neues Passwort bestätigen"),
        AccountMessage::ChangePassword => text.literal("Passwort ändern"),
    }
    AccountText<'a> => MessageWriter |text| {
        AccountText::InitialPassword(site) => {
            text.literal(
                "Ein Administrator hat Ihr Konto mit einem Anfangspasswort erstellt. Wählen Sie ein neues Passwort, \
                 bevor Sie mit ",
            );
            text.argument(site);
            text.literal(" fortfahren.");
        }
        AccountText::PasswordHint(length) => {
            text.literal("Mindestens ");
            text.number(length);
            text.literal(
                " Zeichen. Einige zusammenhanglose Wörter ergeben ein starkes, leicht zu merkendes Passwort.",
            );
        }
    }
}
