//! German (`de`) entries for `catalog/shell/recovery.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::recovery::RecoveryMessage;

entries! { De:
    RecoveryMessage<'a> => MessageWriter |text| {
        RecoveryMessage::ResetTitle => text.literal("Passwort zurücksetzen"),
        RecoveryMessage::ForgotLede => text.literal(
            "Geben Sie die E-Mail-Adresse Ihres Kontos ein. Wir senden Ihnen einen Link, mit dem Sie ein neues \
             Passwort festlegen können.",
        ),
        RecoveryMessage::ForgotWithoutEmail => text.literal(
            "E-Mail ist auf diesem Server nicht verfügbar, daher können Passwörter nicht per E-Mail zurückgesetzt \
             werden. Bitten Sie einen Administrator, ein neues Passwort für Sie festzulegen.",
        ),
        RecoveryMessage::CheckEmail => text.literal("Prüfen Sie Ihre E-Mails"),
        RecoveryMessage::LinkSent => text.literal(
            "Falls ein Konto diese Adresse verwendet, haben wir einen Link zum Festlegen eines neuen Passworts \
             gesendet. Der Link funktioniert einmal und läuft nach einer Stunde ab.",
        ),
        RecoveryMessage::NothingArrived => text.literal(
            "Nichts angekommen? Prüfen Sie Ihren Spam-Ordner oder",
        ),
        RecoveryMessage::TryAgain => text.literal("versuchen Sie es erneut"),
        RecoveryMessage::SendResetLink => text.literal("Link zum Zurücksetzen senden"),
        RecoveryMessage::RememberedIt => text.literal("Wieder eingefallen?"),
        RecoveryMessage::RequestNewLink => text.literal("Neuen Link anfordern"),
        RecoveryMessage::NewLinkWithoutEmail => text.literal(
            "E-Mail ist auf diesem Server nicht verfügbar, daher kann kein neuer Link gesendet werden. Bitten Sie \
             einen Administrator, ein neues Passwort für Sie festzulegen.",
        ),
        RecoveryMessage::ChooseNewPassword => text.literal("Neues Passwort festlegen"),
        RecoveryMessage::ResetFor(username) => {
            text.literal("Für ");
            text.argument(username);
            text.literal(". Sie melden sich ab jetzt damit an.");
        }
        RecoveryMessage::NewPassword => text.literal("Neues Passwort"),
        RecoveryMessage::ConfirmNewPassword => text.literal("Neues Passwort bestätigen"),
        RecoveryMessage::ChangePassword => text.literal("Passwort ändern"),
        RecoveryMessage::BackToSignIn => text.literal("Zurück zur Anmeldung"),
        RecoveryMessage::PasswordChanged => text.literal("Passwort geändert"),
        RecoveryMessage::PasswordChangedText => text.literal(
            "Ihr neues Passwort ist festgelegt. Verwenden Sie es bei Ihrer nächsten Anmeldung.",
        ),
        RecoveryMessage::LinkExpired => text.literal("Dieser Link ist abgelaufen"),
        RecoveryMessage::ResetLinkExpiredText => text.literal(
            "Links zum Zurücksetzen des Passworts funktionieren einmal und laufen nach einer Stunde ab. Fordern Sie \
             einen neuen an, um fortzufahren.",
        ),
        RecoveryMessage::AccountSuspended => text.literal("Dieses Konto ist gesperrt"),
        RecoveryMessage::AccountSuspendedText => text.literal(
            "Sein Passwort kann nicht zurückgesetzt werden. Wenden Sie sich an einen Administrator.",
        ),
        RecoveryMessage::EmailVerified => text.literal("E-Mail-Adresse bestätigt"),
        RecoveryMessage::ThanksForConfirming(address) => {
            text.literal("Danke, dass Sie ");
            text.argument(address);
            text.literal(" bestätigt haben.");
        }
        RecoveryMessage::VerifyLinkExpiredText => text.literal(
            "Bestätigungslinks funktionieren einmal, laufen nach zwei Tagen ab und werden ungültig, wenn sich die \
             Adresse ändert. Melden Sie sich an und senden Sie in Ihren Kontoeinstellungen einen neuen.",
        ),
        RecoveryMessage::GoToAccountSettings => text.literal("Zu den Kontoeinstellungen"),
        RecoveryMessage::SignIn => text.literal("Anmelden"),
    }
}
