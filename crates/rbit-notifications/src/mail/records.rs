//! Feature-owned digest and recipient records; SQL row decoding stays in storage.
use super::policy::{Cadence, MailPreference};
use crate::NotificationReason;
use rbit_collaboration::threads::{IssueId, IssueKind, IssueNumber};
use rbit_contracts::{AccountId, RepoId, Timestamp};

/// An active user with a verified address, read afresh for each delivery, with their
/// email settings (`preference: None` follows the configured default).
#[derive(Debug, Clone)]
pub struct MailRecipient<U> {
    pub account: AccountId,
    pub preference: Option<MailPreference>,
    pub cadence: Cadence,
    /// What composition needs to address and render for them.
    pub user: U,
}

/// A notification that waits for a digest instead of being emailed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDigestItem {
    pub account: AccountId,
    pub repo: RepoId,
    pub issue: IssueId,
    pub reason: NotificationReason,
    /// One sentence, e.g. `bob commented on issue #3.`
    pub headline: String,
    /// An excerpt of the text, already shortened, or empty.
    pub excerpt: String,
    /// The fragment of the thread page to link to.
    pub anchor: Option<String>,
    pub created_at: Timestamp,
}

/// A user with notifications waiting for a digest, and their digest settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingDigest {
    pub account: AccountId,
    /// `Immediate` for users who switched back after notifications were queued.
    pub cadence: Cadence,
    /// `None` follows the instance's default digest hour.
    pub digest_hour: Option<u8>,
}

/// A digest claimed for one user and period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimedDigest {
    pub id: i64,
    pub account: AccountId,
    pub period: Timestamp,
    /// The items attached to it.
    pub items: u64,
}

/// The outcome of claiming a user's digest for a period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// The period was claimed before (by this or another sender): nothing to do.
    Taken,
    /// Nothing queued before the period is left to send; no digest is recorded.
    Nothing {
        /// Items deleted because their thread was read in the app.
        skipped_read: u64,
    },
    /// The digest to send now.
    Claimed {
        digest: ClaimedDigest,
        /// Items left out because their thread was read in the app.
        skipped_read: u64,
    },
}

/// One notification of a claimed digest, with the current names of its thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestItem {
    pub repo: RepoId,
    pub issue: IssueId,
    pub reason: NotificationReason,
    pub headline: String,
    pub excerpt: String,
    pub anchor: Option<String>,
    pub created_at: Timestamp,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub title: String,
    pub owner: String,
    pub repo_name: String,
}

/// What became of a claimed digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestState {
    /// Claimed; sending has not finished (or the process stopped while sending).
    Claimed,
    /// The mail transport accepted it.
    Sent,
    /// The mail transport refused it or timed out. It is not retried.
    Failed,
    /// Nothing was left to send once access and preferences were checked again.
    Empty,
}

impl DigestState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Empty => "empty",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Claimed, Self::Sent, Self::Failed, Self::Empty].into_iter().find(|state| state.as_str() == value)
    }
}
