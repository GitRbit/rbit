//! Who wants which notification email, and the text and threading rules every message
//! shares. Rendering templates and transports stay outside the feature.
use crate::NotificationReason;
use rbit_collaboration::threads::IssueKind;

/// Longest excerpt quoted in a message, in characters.
pub const MAX_EXCERPT_CHARS: usize = 1000;
/// Longest excerpt of one notification in a digest, in characters (on one line).
pub const MAX_DIGEST_EXCERPT_CHARS: usize = 200;
/// The latest digest hour (UTC); later hours are refused or clamped.
pub const MAX_DIGEST_HOUR: u8 = 23;
/// Where users manage their notification emails.
pub const SETTINGS_PATH: &str = "/settings/notifications";
/// The one-click unsubscribe endpoint; the token follows.
pub const UNSUBSCRIBE_PATH: &str = "/email/unsubscribe/";

/// Which notifications a user also receives by email (the configured preference,
/// mapped by composition).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailPreference {
    Off,
    /// Leaves out threads only subscribed to by hand.
    Participating,
    All,
}

/// When a user's notification emails are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Immediate,
    Daily,
    Weekly,
}

impl Cadence {
    pub const fn is_digest(self) -> bool {
        !matches!(self, Self::Immediate)
    }
}

/// Whether `preference` asks for email about a notification with `reason`.
pub fn wants(preference: MailPreference, reason: NotificationReason) -> bool {
    match preference {
        MailPreference::Off => false,
        MailPreference::Participating => reason != NotificationReason::Manual,
        MailPreference::All => true,
    }
}

/// Whether a chosen digest hour is accepted (0 to 23 UTC, or none).
pub fn valid_digest_hour(hour: Option<u8>) -> bool {
    hour.is_none_or(|hour| hour <= MAX_DIGEST_HOUR)
}

/// The end of "You are receiving this because ...".
pub const fn reason_clause(reason: NotificationReason) -> &'static str {
    match reason {
        NotificationReason::Manual => "you are subscribed to this thread",
        NotificationReason::Comment => "you commented on this thread",
        NotificationReason::Reviewed => "you reviewed this pull request",
        NotificationReason::Author => "you opened this thread",
        NotificationReason::Mention => "you were mentioned",
        NotificationReason::Assigned => "you are assigned",
        NotificationReason::ReviewRequested => "your review was requested",
    }
}

pub const fn kind_label(kind: IssueKind) -> &'static str {
    match kind {
        IssueKind::Issue => "issue",
        IssueKind::Pull => "pull request",
    }
}

/// The URL segment of a thread (`/owner/repo/{segment}/N`), also used in message ids.
pub const fn thread_segment(kind: IssueKind) -> &'static str {
    match kind {
        IssueKind::Issue => "issues",
        IssueKind::Pull => "pulls",
    }
}

/// `owner/repo/{issues|pulls}/N`: the thread's path below the base URL.
pub fn thread_path(full_name: &str, kind: IssueKind, number: u32) -> String {
    format!("{full_name}/{}/{number}", thread_segment(kind))
}

/// The absolute link of a thread page, at `anchor` when given.
pub fn thread_link(base_url: &str, path: &str, anchor: Option<&str>) -> String {
    match anchor {
        Some(anchor) => format!("{base_url}/{path}#{anchor}"),
        None => format!("{base_url}/{path}"),
    }
}

pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// `text` trimmed and cut to [`MAX_EXCERPT_CHARS`] characters.
pub fn excerpt_of(text: &str) -> String {
    cut(text.trim(), MAX_EXCERPT_CHARS)
}

/// `text` on one line (runs of whitespace become one space), cut to
/// [`MAX_DIGEST_EXCERPT_CHARS`] characters.
pub fn one_line_excerpt(text: &str) -> String {
    cut(&text.split_whitespace().collect::<Vec<_>>().join(" "), MAX_DIGEST_EXCERPT_CHARS)
}

fn cut(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", text[..end].trim_end()),
        None => text.to_owned(),
    }
}

/// The right-hand side of message ids: the public URL's host, without port or user info.
pub fn message_id_host(base_url: &str) -> String {
    let authority = base_url.split_once("://").map_or(base_url, |(_, rest)| rest);
    let authority = authority.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = if authority.starts_with('[') {
        // An IPv6 literal keeps its brackets: `[::1]` is a valid domain literal.
        authority.split_inclusive(']').next().unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    if host.is_empty() { "localhost".to_owned() } else { host.to_ascii_lowercase() }
}

/// The `Message-ID` and `In-Reply-To` of a message about the thread at `path`: the
/// opening message is the thread's root, every other one replies to it.
pub fn thread_message_ids(path: &str, host: &str, key: &str, opening: bool) -> (String, Option<String>) {
    let root = format!("<{path}@{host}>");
    if opening { (root, None) } else { (format!("<{path}/{key}@{host}>"), Some(root)) }
}

/// A digest's `Message-ID`: stable per user and period, so a duplicate is recognisable.
pub fn digest_message_id(account: i64, period: i64, host: &str) -> String {
    format!("<digest/{account}/{period}@{host}>")
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
