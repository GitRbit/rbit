//! Signing in and up, the second sign-in step and sign-in through another provider.
use super::phrase;
use crate::catalog::fallback::{MessageFamily, MessageWriter};

/// What a throttled visitor was trying to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Attempt {
    SignIn,
    SignUp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthMessage<'a> {
    SignInTitle,
    SignUpTitle,
    /// The site name.
    SignInTo(&'a str),
    SignInLede,
    UsernameOrEmail,
    Password,
    ForgotPassword,
    SignIn,
    Or,
    /// The provider's name.
    SignInWith(&'a str),
    NoAccount,
    SignUp,
    CreateYourAccount,
    /// The site name.
    HostYourCode(&'a str),
    Username,
    UsernameHint,
    Email,
    /// The minimum number of characters.
    MinimumLength(usize),
    SetupToken,
    SetupTokenHint,
    CreateAccount,
    RegistrationClosed,
    HaveAccount,
    TwoFactorTitle,
    PasskeyOrApp,
    PasskeyOnly,
    AppOnly,
    UsePasskey,
    PasskeyUnsupported,
    AuthenticationCode,
    LostDevice,
    RecoveryCode,
    LostPasskey,
    VerifyCode,
    NotYou,
    DifferentAccount,
    /// The provider's name, then the site name.
    ProviderLede(&'a str, &'a str),
    /// The provider's name.
    VerifiedBy(&'a str),
    ConfirmByEmail,
    /// The provider's name.
    ProviderPasswordNote(&'a str),
    /// The provider's name; follows a "Sign in" link.
    LinkInSettings(&'a str),
    SignInFailed,
    BackToSignIn,
    BackToSecuritySettings,
    IncorrectCredentials,
    Suspended,
    SignInExpired,
    /// What was attempted and how many seconds remain until the next try.
    TooManyAttempts(Attempt, u64),
}

impl MessageFamily for AuthMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::SignInTitle => "auth.sign-in.title",
            Self::SignUpTitle => "auth.sign-up.title",
            Self::SignInTo(_) => "auth.sign-in.heading",
            Self::SignInLede => "auth.sign-in.lede",
            Self::UsernameOrEmail => "auth.sign-in.login",
            Self::Password => "auth.password",
            Self::ForgotPassword => "auth.sign-in.forgot",
            Self::SignIn => "auth.sign-in.submit",
            Self::Or => "auth.or",
            Self::SignInWith(_) => "auth.sign-in.provider",
            Self::NoAccount => "auth.sign-in.no-account",
            Self::SignUp => "auth.sign-in.sign-up",
            Self::CreateYourAccount => "auth.sign-up.heading",
            Self::HostYourCode(_) => "auth.sign-up.lede",
            Self::Username => "auth.username",
            Self::UsernameHint => "auth.username-hint",
            Self::Email => "auth.email",
            Self::MinimumLength(_) => "auth.password-minimum",
            Self::SetupToken => "auth.sign-up.setup-token",
            Self::SetupTokenHint => "auth.sign-up.setup-token-hint",
            Self::CreateAccount => "auth.sign-up.submit",
            Self::RegistrationClosed => "auth.sign-up.closed",
            Self::HaveAccount => "auth.sign-up.have-account",
            Self::TwoFactorTitle => "auth.two-factor.title",
            Self::PasskeyOrApp => "auth.two-factor.lede-passkey-or-app",
            Self::PasskeyOnly => "auth.two-factor.lede-passkey",
            Self::AppOnly => "auth.two-factor.lede-app",
            Self::UsePasskey => "auth.two-factor.use-passkey",
            Self::PasskeyUnsupported => "auth.two-factor.passkey-unsupported",
            Self::AuthenticationCode => "auth.two-factor.code",
            Self::LostDevice => "auth.two-factor.lost-device",
            Self::RecoveryCode => "auth.two-factor.recovery-code",
            Self::LostPasskey => "auth.two-factor.lost-passkey",
            Self::VerifyCode => "auth.two-factor.submit",
            Self::NotYou => "auth.two-factor.not-you",
            Self::DifferentAccount => "auth.two-factor.different-account",
            Self::ProviderLede(..) => "auth.external.lede",
            Self::VerifiedBy(_) => "auth.external.verified-by",
            Self::ConfirmByEmail => "auth.external.confirm-by-email",
            Self::ProviderPasswordNote(_) => "auth.external.password-note",
            Self::LinkInSettings(_) => "auth.external.link-in-settings",
            Self::SignInFailed => "auth.external.failed",
            Self::BackToSignIn => "auth.external.back-to-sign-in",
            Self::BackToSecuritySettings => "auth.external.back-to-security",
            Self::IncorrectCredentials => "auth.error.incorrect",
            Self::Suspended => "auth.error.suspended",
            Self::SignInExpired => "auth.error.expired",
            Self::TooManyAttempts(..) => "auth.error.too-many-attempts",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::SignInTo(site) => return phrase(text, "Sign in to ", site, ""),
            Self::SignInWith(provider) => return phrase(text, "Sign in with ", provider, ""),
            Self::HostYourCode(site) => return phrase(text, "Host your code on ", site, "."),
            Self::MinimumLength(count) => return minimum_length(text, count),
            Self::ProviderLede(provider, site) => {
                phrase(text, "You signed in with ", provider, ". ");
                return phrase(text, "Choose a username on ", site, ".");
            }
            Self::VerifiedBy(provider) => {
                return phrase(text, "Verified by ", provider, ". A different address has to be confirmed by email.");
            }
            Self::ProviderPasswordNote(provider) => {
                return phrase(
                    text,
                    "You'll sign in with ",
                    provider,
                    "; your account has no password until you set one with a reset link. Git over HTTPS needs a \
                     personal access token.",
                );
            }
            Self::LinkInSettings(provider) => return phrase(text, "and link ", provider, " in your security settings."),
            Self::TooManyAttempts(attempt, seconds) => return too_many_attempts(text, attempt, seconds),
            other => static_text(other),
        };
        text.literal(literal);
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

/// Wording without arguments.
const fn static_text(message: AuthMessage<'_>) -> &'static str {
    match message {
        AuthMessage::SignInTitle | AuthMessage::SignIn => "Sign in",
        AuthMessage::SignUpTitle | AuthMessage::SignUp => "Sign up",
        AuthMessage::SignInLede => "Use your username or email address.",
        AuthMessage::UsernameOrEmail => "Username or email",
        AuthMessage::Password => "Password",
        AuthMessage::ForgotPassword => "Forgot password?",
        AuthMessage::Or => "or",
        AuthMessage::NoAccount => "Don't have an account?",
        AuthMessage::CreateYourAccount => "Create your account",
        AuthMessage::Username => "Username",
        AuthMessage::UsernameHint => "Letters, digits, '-', '_' and '.'",
        AuthMessage::Email => "Email",
        AuthMessage::SetupToken => "Setup token",
        AuthMessage::SetupTokenHint => "This first account administers the server. Find the token in the server log.",
        AuthMessage::CreateAccount => "Create account",
        AuthMessage::RegistrationClosed => "Registration is closed on this server. Ask an administrator for an account.",
        AuthMessage::HaveAccount => "Already have an account?",
        AuthMessage::TwoFactorTitle => "Two-factor authentication",
        AuthMessage::PasskeyOrApp => "Use your passkey or enter the 6-digit code from your authenticator app.",
        AuthMessage::PasskeyOnly => "Use your passkey or security key to finish signing in.",
        AuthMessage::AppOnly => "Enter the 6-digit code from your authenticator app.",
        AuthMessage::UsePasskey => "Use a passkey",
        AuthMessage::PasskeyUnsupported => "This browser can't use passkeys. Enter a code instead.",
        AuthMessage::AuthenticationCode => "Authentication code",
        AuthMessage::LostDevice => "Lost your device? Enter one of your recovery codes instead.",
        AuthMessage::RecoveryCode => "Recovery code",
        AuthMessage::LostPasskey => "Lost your passkey? Enter one of your recovery codes.",
        AuthMessage::VerifyCode => "Verify code",
        AuthMessage::NotYou => "Not you?",
        AuthMessage::DifferentAccount => "Sign in with a different account",
        AuthMessage::ConfirmByEmail => "You'll get an email to confirm this address.",
        AuthMessage::SignInFailed => "Sign-in didn't work",
        AuthMessage::BackToSignIn => "Back to sign in",
        AuthMessage::BackToSecuritySettings => "Back to security settings",
        AuthMessage::IncorrectCredentials => "Incorrect username or password.",
        AuthMessage::Suspended => "This account has been suspended. Contact an administrator.",
        AuthMessage::SignInExpired => "Your sign-in expired. Sign in again.",
        AuthMessage::SignInTo(_)
        | AuthMessage::SignInWith(_)
        | AuthMessage::HostYourCode(_)
        | AuthMessage::MinimumLength(_)
        | AuthMessage::ProviderLede(..)
        | AuthMessage::VerifiedBy(_)
        | AuthMessage::ProviderPasswordNote(_)
        | AuthMessage::LinkInSettings(_)
        | AuthMessage::TooManyAttempts(..) => "",
    }
}

fn minimum_length(text: &mut MessageWriter, count: usize) {
    text.literal("At least ");
    text.number(u64::try_from(count).unwrap_or(u64::MAX));
    text.literal(if count == 1 { " character." } else { " characters." });
}

/// "Too many sign-in attempts. Try again in 2 minutes."
fn too_many_attempts(text: &mut MessageWriter, attempt: Attempt, seconds: u64) {
    text.literal(match attempt {
        Attempt::SignIn => "Too many sign-in attempts. Try again in ",
        Attempt::SignUp => "Too many sign-up attempts. Try again in ",
    });
    if seconds < 60 {
        text.number(seconds);
        text.literal(if seconds == 1 { " second." } else { " seconds." });
    } else {
        let minutes = seconds.div_ceil(60);
        text.number(minutes);
        text.literal(if minutes == 1 { " minute." } else { " minutes." });
    }
}
