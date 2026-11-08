//! Resetting a forgotten password and confirming an email address.
use super::phrase;
use crate::catalog::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryMessage<'a> {
    ResetTitle,
    ForgotLede,
    ForgotWithoutEmail,
    CheckEmail,
    LinkSent,
    NothingArrived,
    TryAgain,
    SendResetLink,
    RememberedIt,
    RequestNewLink,
    NewLinkWithoutEmail,
    ChooseNewPassword,
    /// The account's username.
    ResetFor(&'a str),
    NewPassword,
    ConfirmNewPassword,
    ChangePassword,
    BackToSignIn,
    PasswordChanged,
    PasswordChangedText,
    LinkExpired,
    ResetLinkExpiredText,
    AccountSuspended,
    AccountSuspendedText,
    EmailVerified,
    /// The confirmed address.
    ThanksForConfirming(&'a str),
    VerifyLinkExpiredText,
    GoToAccountSettings,
    SignIn,
}

impl MessageFamily for RecoveryMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::ResetTitle => "recovery.forgot.title",
            Self::ForgotLede => "recovery.forgot.lede",
            Self::ForgotWithoutEmail => "recovery.forgot.no-email",
            Self::CheckEmail => "recovery.forgot.check-email",
            Self::LinkSent => "recovery.forgot.sent",
            Self::NothingArrived => "recovery.forgot.nothing-arrived",
            Self::TryAgain => "recovery.forgot.try-again",
            Self::SendResetLink => "recovery.forgot.submit",
            Self::RememberedIt => "recovery.forgot.remembered",
            Self::RequestNewLink => "recovery.reset.request-new-link",
            Self::NewLinkWithoutEmail => "recovery.reset.no-email",
            Self::ChooseNewPassword => "recovery.reset.title",
            Self::ResetFor(_) => "recovery.reset.lede",
            Self::NewPassword => "recovery.reset.new-password",
            Self::ConfirmNewPassword => "recovery.reset.confirm-password",
            Self::ChangePassword => "recovery.reset.submit",
            Self::BackToSignIn => "recovery.reset.back",
            Self::PasswordChanged => "recovery.complete.title",
            Self::PasswordChangedText => "recovery.complete.text",
            Self::LinkExpired => "recovery.link-expired",
            Self::ResetLinkExpiredText => "recovery.reset.expired-text",
            Self::AccountSuspended => "recovery.reset.suspended",
            Self::AccountSuspendedText => "recovery.reset.suspended-text",
            Self::EmailVerified => "recovery.verify.title",
            Self::ThanksForConfirming(_) => "recovery.verify.thanks",
            Self::VerifyLinkExpiredText => "recovery.verify.expired-text",
            Self::GoToAccountSettings => "recovery.verify.account-settings",
            Self::SignIn => "recovery.sign-in",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::ResetFor(username) => return phrase(text, "For ", username, ". You'll sign in with it from now on."),
            Self::ThanksForConfirming(address) => return phrase(text, "Thanks for confirming ", address, "."),
            Self::ResetTitle => "Reset your password",
            Self::ForgotLede => "Enter the email address of your account and we'll send you a link to choose a new password.",
            Self::ForgotWithoutEmail => {
                "Email is not available on this server, so passwords can't be reset by email. Ask an administrator to set a \
                 new password for you."
            }
            Self::CheckEmail => "Check your email",
            Self::LinkSent => {
                "If an account uses that address, we've sent it a link to choose a new password. The link works once and \
                 expires in an hour."
            }
            Self::NothingArrived => "Nothing arrived? Check your spam folder, or",
            Self::TryAgain => "try again",
            Self::SendResetLink => "Send reset link",
            Self::RememberedIt => "Remembered it?",
            Self::RequestNewLink => "Request a new link",
            Self::NewLinkWithoutEmail => {
                "Email is not available on this server, so a new link can't be sent. Ask an administrator to set a new \
                 password for you."
            }
            Self::ChooseNewPassword => "Choose a new password",
            Self::NewPassword => "New password",
            Self::ConfirmNewPassword => "Confirm new password",
            Self::ChangePassword => "Change password",
            Self::BackToSignIn => "Back to sign in",
            Self::PasswordChanged => "Password changed",
            Self::PasswordChangedText => "Your new password is set. Use it the next time you sign in.",
            Self::LinkExpired => "This link has expired",
            Self::ResetLinkExpiredText => "Password reset links work once and expire after an hour. Request a new one to continue.",
            Self::AccountSuspended => "This account is suspended",
            Self::AccountSuspendedText => "Its password can't be reset. Contact an administrator.",
            Self::EmailVerified => "Email address verified",
            Self::VerifyLinkExpiredText => {
                "Verification links work once, expire after two days and stop working when the address changes. Sign in \
                 and send a new one from your account settings."
            }
            Self::GoToAccountSettings => "Go to account settings",
            Self::SignIn => "Sign in",
        };
        text.literal(literal);
    }
}
