//! The notification inbox and the one-click page that turns notification emails off.
use super::phrase;
use crate::catalog::fallback::{MessageFamily, MessageWriter};

/// The state of a notification's thread, named beside its icon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadState {
    OpenIssue,
    ClosedIssue,
    OpenPull,
    MergedPull,
    ClosedPull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InboxMessage<'a> {
    Notifications,
    Show,
    Unread,
    All,
    MarkAllRead,
    NoNotifications,
    NoNotificationsText,
    AllCaughtUp,
    AllCaughtUpText,
    MarkAsRead,
    Unsubscribe,
    State(ThreadState),
    StopEmails,
    /// The account's username.
    TurnOffFor(&'a str),
    TurnOff,
    EmailsOff,
    /// The account's username.
    NoLongerReceive(&'a str),
    NotificationSettings,
    LinkNotValid,
    LinkNotValidText,
}

impl MessageFamily for InboxMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Notifications => "inbox.title",
            Self::Show => "inbox.filter",
            Self::Unread => "inbox.unread",
            Self::All => "inbox.all",
            Self::MarkAllRead => "inbox.mark-all-read",
            Self::NoNotifications => "inbox.empty-title",
            Self::NoNotificationsText => "inbox.empty-text",
            Self::AllCaughtUp => "inbox.caught-up-title",
            Self::AllCaughtUpText => "inbox.caught-up-text",
            Self::MarkAsRead => "inbox.mark-read",
            Self::Unsubscribe => "inbox.unsubscribe",
            Self::State(state) => match state {
                ThreadState::OpenIssue => "inbox.state.open-issue",
                ThreadState::ClosedIssue => "inbox.state.closed-issue",
                ThreadState::OpenPull => "inbox.state.open-pull",
                ThreadState::MergedPull => "inbox.state.merged-pull",
                ThreadState::ClosedPull => "inbox.state.closed-pull",
            },
            Self::StopEmails => "unsubscribe.confirm.title",
            Self::TurnOffFor(_) => "unsubscribe.confirm.text",
            Self::TurnOff => "unsubscribe.confirm.submit",
            Self::EmailsOff => "unsubscribe.done.title",
            Self::NoLongerReceive(_) => "unsubscribe.done.text",
            Self::NotificationSettings => "unsubscribe.settings",
            Self::LinkNotValid => "unsubscribe.invalid.title",
            Self::LinkNotValidText => "unsubscribe.invalid.text",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::TurnOffFor(username) => {
                return phrase(text, "Turn off notification emails for ", username, "? Notifications keep appearing in the inbox.");
            }
            Self::NoLongerReceive(username) => {
                return phrase(text, "", username, " will no longer receive notification emails. You can turn them back on at any time.");
            }
            Self::Notifications => "Notifications",
            Self::Show => "Show",
            Self::Unread => "Unread",
            Self::All => "All",
            Self::MarkAllRead => "Mark all as read",
            Self::NoNotifications => "No notifications",
            Self::NoNotificationsText => {
                "You hear here about issues and pull requests you take part in: ones you opened, commented on, are assigned \
                 to or were mentioned in."
            }
            Self::AllCaughtUp => "All caught up",
            Self::AllCaughtUpText => "Nothing unread. Older notifications are under All.",
            Self::MarkAsRead => "Mark as read",
            Self::Unsubscribe => "Unsubscribe",
            Self::State(state) => match state {
                ThreadState::OpenIssue => "Open issue",
                ThreadState::ClosedIssue => "Closed issue",
                ThreadState::OpenPull => "Open pull request",
                ThreadState::MergedPull => "Merged pull request",
                ThreadState::ClosedPull => "Closed pull request",
            },
            Self::StopEmails => "Stop notification emails",
            Self::TurnOff => "Turn off notification emails",
            Self::EmailsOff => "Notification emails are off",
            Self::NotificationSettings => "Notification settings",
            Self::LinkNotValid => "This link is not valid",
            Self::LinkNotValidText => "The link may be incomplete. Sign in to choose which notifications are emailed to you.",
        };
        text.literal(literal);
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
