//! Repository-grouped notification digest messages.

use super::MailMessage;
use crate::MailError;
use askama::Template;

// ------------------------------------------------------------------ notification digest

/// Notifications collected into one email a day or a week, grouped by repository and
/// thread: what happened, a short excerpt and a link for each, plus the same settings and
/// unsubscribe links as a [`super::ThreadNotification`].
#[derive(Clone)]
pub struct DigestNotification {
    pub site_name: String,
    pub base_url: String,
    /// A weekly digest (otherwise daily).
    pub weekly: bool,
    /// The repositories, each with its threads, in the order they are shown.
    pub repositories: Vec<DigestRepository>,
    pub settings_link: String,
    /// The page that turns notification emails off (the link in `List-Unsubscribe`).
    pub unsubscribe_link: String,
}

/// One repository of a digest.
#[derive(Clone)]
pub struct DigestRepository {
    /// `owner/repo`.
    pub name: String,
    pub link: String,
    pub threads: Vec<DigestThread>,
}

/// One issue or pull request of a digest.
#[derive(Clone)]
pub struct DigestThread {
    pub number: u64,
    pub title: String,
    pub is_pull: bool,
    pub link: String,
    pub entries: Vec<DigestEntry>,
}

/// One notification of a digest.
#[derive(Clone)]
pub struct DigestEntry {
    /// One sentence, e.g. `bob commented on issue #3.`
    pub headline: String,
    /// A one-line excerpt, or empty.
    pub excerpt: String,
    /// The thread, anchored at the activity.
    pub link: String,
}

impl DigestNotification {
    fn subject(&self) -> String {
        format!("Your {} digest: {}", self.period(), self.summary())
    }

    /// `daily` or `weekly`.
    pub fn period(&self) -> &'static str {
        if self.weekly { "weekly" } else { "daily" }
    }

    /// The number of notifications.
    pub fn count(&self) -> usize {
        self.repositories.iter().flat_map(|repo| &repo.threads).map(|thread| thread.entries.len()).sum()
    }

    /// e.g. `3 notifications in 2 repositories`.
    pub fn summary(&self) -> String {
        let count = self.count();
        let repositories = self.repositories.len();
        format!(
            "{count} notification{} in {repositories} repositor{}",
            if count == 1 { "" } else { "s" },
            if repositories == 1 { "y" } else { "ies" }
        )
    }
}

impl DigestThread {
    /// `Issue #3` or `Pull request #3`.
    pub fn label(&self) -> String {
        format!("{} #{}", if self.is_pull { "Pull request" } else { "Issue" }, self.number)
    }
}

#[derive(Template)]
#[template(path = "notification_digest.txt")]
struct DigestNotificationText<'a> {
    m: &'a DigestNotification,
}

#[derive(Template)]
#[template(path = "notification_digest.html")]
struct DigestNotificationHtml<'a> {
    m: &'a DigestNotification,
}

templates!(DigestNotification, text = DigestNotificationText, html = DigestNotificationHtml);

impl std::fmt::Debug for DigestNotification {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("DigestNotification").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for DigestRepository {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("DigestRepository").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for DigestThread {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("DigestThread").finish_non_exhaustive()
    }
}

impl std::fmt::Debug for DigestEntry {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("DigestEntry").finish_non_exhaustive()
    }
}
