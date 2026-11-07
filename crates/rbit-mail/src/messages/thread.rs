//! Individual issue and pull-request activity messages.

use super::MailMessage;
use crate::MailError;
use askama::Template;

// ------------------------------------------------------------------ thread notification

/// Milestone 3 workstream N: activity on an issue or pull request the recipient follows
/// (a mention, an assignment, a review request, a review, a comment, a state change).
/// Short on purpose: what happened, an excerpt, a link to the thread, why the recipient
/// got it, and links to the notification settings and to stop these emails.
#[derive(Debug, Clone)]
pub struct ThreadNotification {
    pub site_name: String,
    pub base_url: String,
    /// `owner/repo`.
    pub repository: String,
    pub number: u64,
    pub title: String,
    /// One sentence, e.g. `bob commented on issue #3.`
    pub headline: String,
    /// The comment, description or review text (Markdown source, already shortened), or empty.
    pub excerpt: String,
    /// The thread, anchored at the activity.
    pub link: String,
    /// Whether this is a pull request (the button says so).
    pub is_pull: bool,
    /// Whether the message continues the thread (the subject starts with `Re:`).
    pub is_reply: bool,
    /// e.g. `You are receiving this because you were mentioned.`
    pub reason: String,
    pub settings_link: String,
    /// The page that turns notification emails off (the link in `List-Unsubscribe`).
    pub unsubscribe_link: String,
}

impl ThreadNotification {
    fn subject(&self) -> String {
        let prefix = if self.is_reply { "Re: " } else { "" };
        format!("{prefix}[{}] {} (#{})", self.repository, self.title, self.number)
    }

    /// The button label.
    pub fn action_label(&self) -> &'static str {
        if self.is_pull { "View pull request" } else { "View issue" }
    }
}

#[derive(Template)]
#[template(path = "thread_notification.txt")]
struct ThreadNotificationText<'a> {
    m: &'a ThreadNotification,
}

#[derive(Template)]
#[template(path = "thread_notification.html")]
struct ThreadNotificationHtml<'a> {
    m: &'a ThreadNotification,
}

templates!(ThreadNotification, text = ThreadNotificationText, html = ThreadNotificationHtml);
