//! Every key of the shell families (sign-in, recovery, home, inbox, shared pages)
//! renders in English and every diagnostic catalog, and falls back to the same English
//! text, marked as fallback, in Spanish, which has not translated them.
use std::collections::BTreeSet;

use rbit_localization::{
    Catalog, Locale, MessageFamily,
    shell::{
        auth::{Attempt, AuthMessage},
        home::{Feature, HomeMessage, Involvement},
        inbox::{InboxMessage, ThreadState},
        pages::{Failure, PageMessage},
        recovery::RecoveryMessage,
    },
};

/// Unique ids, non-empty text everywhere, English-derived catalogs never fall back and
/// Spanish shows the English text marked as fallback.
fn check_family<M: MessageFamily + std::fmt::Debug>(messages: &[M]) {
    let ids: BTreeSet<_> = messages.iter().map(|&message| message.id()).collect();
    assert_eq!(ids.len(), messages.len(), "every listed message has its own id");
    for &message in messages {
        let english = Catalog::new(Locale::English).localized(message);
        assert!(!english.as_str().is_empty() && !english.fallback(), "{message:?}");
        for locale in [Locale::PseudoExpanded, Locale::PseudoRtl] {
            let text = Catalog::new(locale).localized(message);
            assert!(text.as_str().starts_with('[') && !text.fallback(), "{message:?} in {locale:?}");
        }
        let spanish = Catalog::new(Locale::Spanish).localized(message);
        assert_eq!(spanish.as_str(), english.as_str(), "{message:?}");
        assert!(spanish.fallback(), "{message:?} is untranslated in Spanish");
    }
}

fn english<M: MessageFamily>(message: M) -> String {
    Catalog::new(Locale::English).localized(message).to_string()
}

#[test]
fn auth_keys_render_and_fall_back() {
    use AuthMessage as M;
    check_family(&[
        M::SignInTitle,
        M::SignUpTitle,
        M::SignInTo("rbit"),
        M::SignInLede,
        M::UsernameOrEmail,
        M::Password,
        M::ForgotPassword,
        M::SignIn,
        M::Or,
        M::SignInWith("GitHub"),
        M::NoAccount,
        M::SignUp,
        M::CreateYourAccount,
        M::HostYourCode("rbit"),
        M::Username,
        M::UsernameHint,
        M::Email,
        M::MinimumLength(12),
        M::SetupToken,
        M::SetupTokenHint,
        M::CreateAccount,
        M::RegistrationClosed,
        M::HaveAccount,
        M::TwoFactorTitle,
        M::PasskeyOrApp,
        M::PasskeyOnly,
        M::AppOnly,
        M::UsePasskey,
        M::PasskeyUnsupported,
        M::AuthenticationCode,
        M::LostDevice,
        M::RecoveryCode,
        M::LostPasskey,
        M::VerifyCode,
        M::NotYou,
        M::DifferentAccount,
        M::ProviderLede("Corp SSO", "rbit"),
        M::VerifiedBy("Corp SSO"),
        M::ConfirmByEmail,
        M::ProviderPasswordNote("Corp SSO"),
        M::LinkInSettings("Corp SSO"),
        M::SignInFailed,
        M::BackToSignIn,
        M::BackToSecuritySettings,
        M::IncorrectCredentials,
        M::Suspended,
        M::SignInExpired,
        M::TooManyAttempts(Attempt::SignIn, 30),
    ]);
}

#[test]
fn auth_counts_and_arguments_read_naturally() {
    use AuthMessage as M;
    assert_eq!(english(M::TooManyAttempts(Attempt::SignIn, 1)), "Too many sign-in attempts. Try again in 1 second.");
    assert_eq!(english(M::TooManyAttempts(Attempt::SignIn, 45)), "Too many sign-in attempts. Try again in 45 seconds.");
    assert_eq!(english(M::TooManyAttempts(Attempt::SignUp, 60)), "Too many sign-up attempts. Try again in 1 minute.");
    assert_eq!(english(M::TooManyAttempts(Attempt::SignUp, 61)), "Too many sign-up attempts. Try again in 2 minutes.");
    assert_eq!(english(M::MinimumLength(1)), "At least 1 character.");
    assert_eq!(english(M::MinimumLength(12)), "At least 12 characters.");
    assert_eq!(english(M::ProviderLede("<Corp>", "rbit")), "You signed in with <Corp>. Choose a username on rbit.");
    let rtl = Catalog::new(Locale::PseudoRtl).localized(M::SignInWith("GitHub")).to_string();
    assert!(rtl.contains("\u{2068}GitHub\u{2069}"), "arguments are isolated: {rtl}");
}

#[test]
fn recovery_keys_render_and_fall_back() {
    use RecoveryMessage as M;
    check_family(&[
        M::ResetTitle,
        M::ForgotLede,
        M::ForgotWithoutEmail,
        M::CheckEmail,
        M::LinkSent,
        M::NothingArrived,
        M::TryAgain,
        M::SendResetLink,
        M::RememberedIt,
        M::RequestNewLink,
        M::NewLinkWithoutEmail,
        M::ChooseNewPassword,
        M::ResetFor("alice"),
        M::NewPassword,
        M::ConfirmNewPassword,
        M::ChangePassword,
        M::BackToSignIn,
        M::PasswordChanged,
        M::PasswordChangedText,
        M::LinkExpired,
        M::ResetLinkExpiredText,
        M::AccountSuspended,
        M::AccountSuspendedText,
        M::EmailVerified,
        M::ThanksForConfirming("alice@example.com"),
        M::VerifyLinkExpiredText,
        M::GoToAccountSettings,
        M::SignIn,
    ]);
    assert_eq!(english(M::ResetFor("alice")), "For alice. You'll sign in with it from now on.");
}

#[test]
fn home_keys_render_and_fall_back() {
    use HomeMessage as M;
    let mut all = vec![
        M::SearchPublic,
        M::Search,
        M::ExploreAll,
        M::ThisInstance,
        M::RepositoryCount(2),
        M::UserCount(1),
        M::OrganizationCount(0),
        M::RecentlyUpdated,
        M::ViewAll,
        M::NoPublicRepositories,
        M::NoPublicRepositoriesText,
        M::FreshServer,
        M::BuiltForSelfHosting,
        M::Dashboard,
        M::ViewProfile,
        M::New,
        M::NewRepository,
        M::PersonalDashboards,
        M::Issues,
        M::PullRequests,
        M::Repositories,
        M::PrivateCount(3),
        M::FindRepository,
        M::FilterRepositories,
        M::NoRepositories,
        M::NoRepositoriesText,
        M::CreateRepository,
        M::NoFilterMatch,
        M::Organizations,
        M::NewOrganization,
        M::NotAMember,
        M::CreateOne,
        M::ShareWithTeam,
        M::Explore,
        M::NothingPublic,
        M::YourInvolvement,
        M::SearchIn("Issues"),
        M::SearchHelp,
        M::SearchHelpText,
        M::Results,
        M::OpenCount(4),
        M::ClosedCount(1),
        M::NoMatchingThreads,
        M::NoMatchingThreadsText,
        M::CommentCount(2),
    ];
    for feature in [Feature::Binary, Feature::Protocols, Feature::Teams, Feature::Releases, Feature::Security, Feature::Screens] {
        all.extend([M::FeatureTitle(feature), M::FeatureText(feature)]);
    }
    for tab in [Involvement::Created, Involvement::Assigned, Involvement::Mentioned, Involvement::ReviewRequested] {
        all.push(M::InvolvementTab(tab));
    }
    check_family(&all);
    assert_eq!(english(M::RepositoryCount(1)), "Repository");
    assert_eq!(english(M::UserCount(2)), "Users");
    assert_eq!(english(M::CommentCount(1)), "1 comment");
    assert_eq!(english(M::OpenCount(7)), "7 Open");
}

#[test]
fn inbox_keys_render_and_fall_back() {
    use InboxMessage as M;
    let mut all = vec![
        M::Notifications,
        M::Show,
        M::Unread,
        M::All,
        M::MarkAllRead,
        M::NoNotifications,
        M::NoNotificationsText,
        M::AllCaughtUp,
        M::AllCaughtUpText,
        M::MarkAsRead,
        M::Unsubscribe,
        M::StopEmails,
        M::TurnOffFor("alice"),
        M::TurnOff,
        M::EmailsOff,
        M::NoLongerReceive("alice"),
        M::NotificationSettings,
        M::LinkNotValid,
        M::LinkNotValidText,
    ];
    for state in [ThreadState::OpenIssue, ThreadState::ClosedIssue, ThreadState::OpenPull, ThreadState::MergedPull, ThreadState::ClosedPull]
    {
        all.push(M::State(state));
    }
    check_family(&all);
    assert_eq!(
        english(M::NoLongerReceive("alice")),
        "alice will no longer receive notification emails. You can turn them back on at any time."
    );
}

#[test]
fn page_keys_render_and_fall_back() {
    use PageMessage as M;
    let mut all = vec![
        M::NotFoundText,
        M::UnavailableText,
        M::InternalText,
        M::GoHome,
        M::Explore,
        M::Offline,
        M::OfflineText,
        M::Retry,
        M::Private,
        M::Archived,
        M::Stars(3),
        M::Pagination,
        M::Previous,
        M::Next,
        M::PageOf(2, 5),
        M::EditProfile,
        M::Joined("Oct 15, 2026"),
        M::Profile,
        M::Repositories,
        M::StarsTab,
        M::NewRepository,
        M::NoStars("alice"),
        M::NoPublicRepositories("alice"),
    ];
    for failure in [Failure::NotFound, Failure::Forbidden, Failure::BadRequest, Failure::SlowDown, Failure::Unavailable, Failure::Internal]
    {
        all.push(M::ErrorTitle(failure));
    }
    check_family(&all);
    assert_eq!(english(M::PageOf(2, 5)), "Page 2 of 5");
    assert_eq!(english(M::Stars(1)), "1 star");
    assert_eq!(english(M::NoStars("alice")), "alice hasn't starred any repositories yet.");
}
