//! The account settings page: username, email address and password.

labels! {
    /// Fixed labels of `/settings/account`.
    AccountMessage, "settings.account" {
        UpdatePassword = "update-password" => "Update your password",
        Title = "title" => "Account",
        Intro = "intro" => "How you sign in. Changes ask for your current password.",
        Username = "username" => "Username",
        UsernameUpdated = "username-updated" => "Username updated.",
        UsernameHint = "username-hint" => "Profile links and Git remotes using your old name keep working until someone takes that name. Update your links and remotes. Sign in with your new username or email.",
        CurrentPassword = "current-password" => "Current password",
        ChangeUsername = "change-username" => "Change username",
        VerificationSent = "verification-sent" => "Verification email sent. Check your inbox.",
        Verified = "verified" => "Verified",
        Unverified = "unverified" => "Unverified",
        VerifiedText = "verified-text" => "You have confirmed that this address is yours.",
        PendingText = "pending-text" => "Confirm the address with the link we emailed you.",
        UnavailableText = "unavailable-text" => "Email is not available on this server, so the address can't be verified.",
        Resend = "resend" => "Resend verification email",
        EmailAddress = "email-address" => "Email address",
        EmailUpdated = "email-updated" => "Email address updated.",
        Email = "email" => "Email",
        EmailHintLead = "email-hint-lead" => "You can sign in with this address or your username. Currently",
        EmailHintTail = "email-hint-tail" => ".",
        UpdateEmail = "update-email" => "Update email",
        Password = "password" => "Password",
        PasswordChanged = "password-changed" => "Password changed.",
        NewPassword = "new-password" => "New password",
        ConfirmPassword = "confirm-password" => "Confirm new password",
        ChangePassword = "change-password" => "Change password",
    }
}

messages! {
    /// Account settings text with arguments.
    AccountText<'a>, "settings.account" {
        InitialPassword(site: &'a str) = "initial-password" => [
            "An administrator created your account with an initial password. Choose a new password before continuing to ",
            {site},
            "."
        ],
        PasswordHint(length: u64) = "password-hint" => [
            "At least ",
            (length),
            " characters. A few unrelated words make a strong, memorable password."
        ],
    }
}
