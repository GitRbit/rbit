//! Issues and pull requests: the shared core of milestone 2 wave B.
//!
//! A pull request is an issue with a branch attached. Both kinds share one number
//! sequence per repository ([`IssueNumber`]), comments and the timeline
//! ([`TimelineEvent`]). Who may do what with a thread is decided purely by
//! [`issue_permissions`] and [`comment_permissions`].
//!
//! The integrator owns this file (see `docs/design/milestone-2b.md`); workstreams request
//! changes through their reports.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AccountId, NameError, RepoAccess, RepoId, Timestamp, User};

// ------------------------------------------------------------------ ids

/// Database id of an issue or pull request (the `issues` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IssueId(pub i64);

/// Database id of a comment on an issue or pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommentId(pub i64);

/// Database id of a timeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EventId(pub i64);

/// Database id of a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LabelId(pub i64);

/// Database id of a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MilestoneId(pub i64);

/// Database id of a pull request review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReviewId(pub i64);

// ------------------------------------------------------------------ numbers

/// Per-repository number of an issue or pull request, always `> 0`. Parsed from URL
/// segments (`"12"`) with [`IssueNumber::parse`]; displayed as `#12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct IssueNumber(u32);

impl IssueNumber {
    const KIND: &'static str = "issue number";

    /// Accepts only canonical decimal numbers: digits, no sign, no leading zero, `1..=u32::MAX`.
    /// Anything else (which a URL handler answers with 404) is an error.
    pub fn parse(value: &str) -> Result<Self, NameError> {
        let kind = Self::KIND;
        if value.is_empty() {
            return Err(NameError::Empty { kind });
        }
        if value.len() > 10 {
            return Err(NameError::TooLong { kind, max: 10 });
        }
        if !value.bytes().all(|b| b.is_ascii_digit()) || value.starts_with('0') {
            return Err(NameError::NotANumber { kind });
        }
        value.parse::<u32>().map_err(|_| NameError::NotANumber { kind }).and_then(Self::try_from)
    }

    /// `None` for zero.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for IssueNumber {
    type Error = NameError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(NameError::NotANumber { kind: Self::KIND })
    }
}

impl TryFrom<i64> for IssueNumber {
    type Error = NameError;

    /// For database rows.
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value).map_err(|_| NameError::NotANumber { kind: Self::KIND }).and_then(Self::try_from)
    }
}

impl From<IssueNumber> for u32 {
    fn from(number: IssueNumber) -> Self {
        number.0
    }
}

impl FromStr for IssueNumber {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// `#12`. Use [`IssueNumber::get`] for URLs.
impl fmt::Display for IssueNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

// ------------------------------------------------------------------ enums

/// Generates `as_str`, `parse` and `ALL` for a field-less enum stored as text.
macro_rules! text_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stable identifier stored in the database and used in payloads.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

text_enum!(
    /// Whether a numbered thread is a plain issue or a pull request.
    IssueKind {
        Issue => "issue",
        Pull => "pull",
    }
);

text_enum!(
    IssueState {
        Open => "open",
        Closed => "closed",
    }
);

text_enum!(
    /// Why a thread was closed. `Merged` is only possible for pull requests.
    StateReason {
        Completed => "completed",
        NotPlanned => "not_planned",
        Merged => "merged",
    }
);

text_enum!(
    /// How a pull request was merged.
    MergeStrategy {
        /// A merge commit with two parents.
        Merge => "merge",
        /// One new commit with the head's tree.
        Squash => "squash",
        /// The head's commits replayed onto the base.
        Rebase => "rebase",
    }
);

text_enum!(
    /// A pull request review's state. `Pending` reviews are visible only to their author.
    ReviewState {
        Pending => "pending",
        Commented => "commented",
        Approved => "approved",
        ChangesRequested => "changes_requested",
        Dismissed => "dismissed",
    }
);

// ------------------------------------------------------------------ text

/// Longest issue or pull request title, in characters.
pub const MAX_TITLE_CHARS: usize = 256;
/// Longest body or comment, in bytes (after line endings are normalised).
pub const MAX_TEXT_BYTES: usize = 65_536;

/// Why a title, body or comment was refused. Messages are shown to users as is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IssueTextError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },
    #[error("{kind} must be at most {max} characters")]
    TooManyCharacters { kind: &'static str, max: usize },
    #[error("{kind} must be at most {max} bytes")]
    TooLong { kind: &'static str, max: usize },
    #[error("{kind} must not contain control characters")]
    ControlCharacter { kind: &'static str },
}

/// An issue or pull request title: trimmed, `1..=256` characters, no control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueTitle(String);

impl IssueTitle {
    pub fn parse(value: &str) -> Result<Self, IssueTextError> {
        let kind = "title";
        let value = value.trim();
        if value.is_empty() {
            return Err(IssueTextError::Empty { kind });
        }
        if value.chars().count() > MAX_TITLE_CHARS {
            return Err(IssueTextError::TooManyCharacters { kind, max: MAX_TITLE_CHARS });
        }
        if value.chars().any(char::is_control) {
            return Err(IssueTextError::ControlCharacter { kind });
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IssueTitle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<IssueTitle> for String {
    fn from(title: IssueTitle) -> Self {
        title.0
    }
}

/// Markdown for issue bodies and comments: at most [`MAX_TEXT_BYTES`], with `\r\n` (and
/// lone `\r`) normalised to `\n`. May be empty for bodies ([`IssueText::body`]), never
/// blank for comments ([`IssueText::comment`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct IssueText(String);

impl IssueText {
    /// An issue or pull request description, possibly empty.
    pub fn body(value: &str) -> Result<Self, IssueTextError> {
        Self::normalise("description", value)
    }

    /// A comment: like a body, but it must contain something other than whitespace.
    pub fn comment(value: &str) -> Result<Self, IssueTextError> {
        let text = Self::normalise("comment", value)?;
        if text.0.trim().is_empty() {
            return Err(IssueTextError::Empty { kind: "comment" });
        }
        Ok(text)
    }

    fn normalise(kind: &'static str, value: &str) -> Result<Self, IssueTextError> {
        let text = value.replace("\r\n", "\n").replace('\r', "\n");
        if text.len() > MAX_TEXT_BYTES {
            return Err(IssueTextError::TooLong { kind, max: MAX_TEXT_BYTES });
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for IssueText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<IssueText> for String {
    fn from(text: IssueText) -> Self {
        text.0
    }
}

// ------------------------------------------------------------------ entities

/// An issue or pull request (the shared `issues` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub id: IssueId,
    pub repo_id: RepoId,
    pub number: IssueNumber,
    pub kind: IssueKind,
    pub title: IssueTitle,
    pub body: IssueText,
    /// `None` once the author's account is deleted (shown as a ghost).
    pub author: Option<AccountId>,
    pub state: IssueState,
    /// Set exactly when closed.
    pub state_reason: Option<StateReason>,
    pub closed_at: Option<Timestamp>,
    pub closed_by: Option<AccountId>,
    pub locked: bool,
    pub comment_count: u32,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Issue {
    pub fn is_open(&self) -> bool {
        self.state == IssueState::Open
    }

    pub fn is_pull(&self) -> bool {
        self.kind == IssueKind::Pull
    }

    /// Whether `user` opened it.
    pub fn is_author(&self, user: &User) -> bool {
        self.author == Some(user.id())
    }
}

/// A comment on an issue or pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub issue_id: IssueId,
    pub author: Option<AccountId>,
    pub body: IssueText,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// `None` until the comment is edited.
    pub edited_at: Option<Timestamp>,
}

/// A label as it was when an event happened (labels can be renamed or deleted later).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabelSnapshot {
    pub id: LabelId,
    pub name: String,
    /// `#rrggbb`, lower case.
    pub color: String,
}

// ------------------------------------------------------------------ timeline

/// Everything the timeline can show besides comments. Stored as `kind` (the variant name
/// in `snake_case`, see [`TimelineEvent::kind`]) plus `data` (the variant's fields as a
/// JSON object); serde's adjacent tagging produces exactly that shape. Unknown kinds are
/// skipped when reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum TimelineEvent {
    // --- core (T0)
    /// `commit`: closed by a pushed commit; `pull`: closed because that PR was merged.
    Closed {
        reason: StateReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pull: Option<IssueNumber>,
    },
    Reopened,
    Renamed {
        from: String,
        to: String,
    },
    Locked,
    Unlocked,
    // --- issues (I)
    Labeled {
        label: LabelSnapshot,
    },
    Unlabeled {
        label: LabelSnapshot,
    },
    Milestoned {
        milestone: MilestoneId,
        title: String,
    },
    Demilestoned {
        milestone: MilestoneId,
        title: String,
    },
    Assigned {
        assignee: AccountId,
    },
    Unassigned {
        assignee: AccountId,
    },
    /// A commit in this repository mentions the issue ("referenced this in abc1234").
    CommitReferenced {
        commit: String,
        summary: String,
    },
    /// Another issue or PR mentions this one; shown only to viewers who can read the source.
    CrossReferenced {
        source: IssueId,
    },
    // --- pull requests (P)
    HeadPushed {
        before: String,
        after: String,
        count: u32,
        forced: bool,
    },
    BaseChanged {
        from: String,
        to: String,
    },
    /// `strategy: None`: merged outside Rbit (the head became part of the base).
    Merged {
        commit: String,
        strategy: Option<MergeStrategy>,
    },
    ReviewRequested {
        reviewer: AccountId,
    },
    ReviewRequestRemoved {
        reviewer: AccountId,
    },
    ReviewDismissed {
        review: ReviewId,
        reviewer: Option<AccountId>,
    },
    HeadRefDeleted {
        branch: String,
    },
    ConvertedToDraft,
    ReadyForReview,
}

impl TimelineEvent {
    /// The `kind` column: the variant name in `snake_case`.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Closed { .. } => "closed",
            Self::Reopened => "reopened",
            Self::Renamed { .. } => "renamed",
            Self::Locked => "locked",
            Self::Unlocked => "unlocked",
            Self::Labeled { .. } => "labeled",
            Self::Unlabeled { .. } => "unlabeled",
            Self::Milestoned { .. } => "milestoned",
            Self::Demilestoned { .. } => "demilestoned",
            Self::Assigned { .. } => "assigned",
            Self::Unassigned { .. } => "unassigned",
            Self::CommitReferenced { .. } => "commit_referenced",
            Self::CrossReferenced { .. } => "cross_referenced",
            Self::HeadPushed { .. } => "head_pushed",
            Self::BaseChanged { .. } => "base_changed",
            Self::Merged { .. } => "merged",
            Self::ReviewRequested { .. } => "review_requested",
            Self::ReviewRequestRemoved { .. } => "review_request_removed",
            Self::ReviewDismissed { .. } => "review_dismissed",
            Self::HeadRefDeleted { .. } => "head_ref_deleted",
            Self::ConvertedToDraft => "converted_to_draft",
            Self::ReadyForReview => "ready_for_review",
        }
    }

    /// The account the event names besides its actor (assignee, reviewer), if any.
    pub const fn subject_account(&self) -> Option<AccountId> {
        match self {
            Self::Assigned { assignee } | Self::Unassigned { assignee } => Some(*assignee),
            Self::ReviewRequested { reviewer } | Self::ReviewRequestRemoved { reviewer } => Some(*reviewer),
            Self::ReviewDismissed { reviewer, .. } => *reviewer,
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ domain event actions

/// What happened to an issue or pull request (`DomainEvent::Issue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueAction {
    /// Published by I for issues and by P for pull requests, once their own data is in place.
    Opened,
    /// Only the fields that changed are `Some`, holding their previous value.
    Edited {
        previous_title: Option<String>,
        previous_body: Option<String>,
        previous_base: Option<String>,
    },
    /// `Merged` ⇒ the `pull_request` webhook reports `closed` with `merged: true`.
    Closed {
        reason: StateReason,
    },
    Reopened,
    Locked,
    Unlocked,
    /// The row is gone; the snapshot is what it was.
    Deleted {
        snapshot: Box<Issue>,
    },
    Labeled {
        label: LabelSnapshot,
    },
    Unlabeled {
        label: LabelSnapshot,
    },
    Assigned {
        assignee: AccountId,
    },
    Unassigned {
        assignee: AccountId,
    },
    Milestoned {
        milestone: MilestoneId,
    },
    Demilestoned {
        milestone: MilestoneId,
        title: String,
    },
    /// The head branch of a pull request moved.
    Synchronized {
        before: String,
        after: String,
    },
    ReviewRequested {
        reviewer: AccountId,
    },
    ReviewRequestRemoved {
        reviewer: AccountId,
    },
    ConvertedToDraft,
    ReadyForReview,
}

/// What happened to a comment (`DomainEvent::IssueComment`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentAction {
    Created,
    Edited {
        previous_body: String,
    },
    /// The row is gone; the snapshot is what it was.
    Deleted {
        snapshot: Comment,
    },
}

/// What happened to a pull request review (`DomainEvent::PullRequestReview`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    Submitted,
    Edited { previous_body: String },
    Dismissed,
}

// ------------------------------------------------------------------ permissions

/// What a viewer may do with an issue or pull request. Reading is not listed: whoever can
/// read the repository can read every thread in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent permission flags")]
pub struct IssuePermissions {
    /// Add a comment (signed in; on a locked thread only writers).
    pub can_comment: bool,
    /// Edit the title and body.
    pub can_edit: bool,
    /// Close and reopen.
    pub can_set_state: bool,
    pub can_lock: bool,
    /// Labels, milestones, assignees, reviewers.
    pub can_triage: bool,
    /// Delete the whole thread (issues only).
    pub can_delete: bool,
    /// Delete other people's comments.
    pub can_moderate: bool,
}

/// What `viewer` may do with `issue`, given their `access` to its repository (token
/// scopes already folded in by the caller with `TokenScope::limit`).
///
/// | Action | Who |
/// |---|---|
/// | comment | signed in, `access ≥ Read`; on a locked thread `access ≥ Write` |
/// | edit title/body, close/reopen | the author (with `access ≥ Read`), or `access ≥ Write` |
/// | lock/unlock, triage, moderate comments | `access ≥ Write` |
/// | delete | `access ≥ Admin`, issues only (pull requests cannot be deleted) |
///
/// Anonymous viewers and viewers without read access may do nothing.
pub fn issue_permissions(viewer: Option<&User>, access: RepoAccess, issue: &Issue) -> IssuePermissions {
    let Some(viewer) = viewer else { return IssuePermissions::default() };
    if !access.can_read() {
        return IssuePermissions::default();
    }
    let writer = access.can_write();
    let author = issue.is_author(viewer);
    IssuePermissions {
        can_comment: writer || !issue.locked,
        can_edit: writer || author,
        can_set_state: writer || author,
        can_lock: writer,
        can_triage: writer,
        can_delete: access.can_admin() && issue.kind == IssueKind::Issue,
        can_moderate: writer,
    }
}

/// What a viewer may do with one comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommentPermissions {
    /// Its author, while they may still comment on the thread.
    pub can_edit: bool,
    /// Its author, or anyone who may moderate the thread.
    pub can_delete: bool,
}

/// What `viewer` may do with `comment` on a thread they hold `permissions` for (from
/// [`issue_permissions`]).
pub fn comment_permissions(viewer: Option<&User>, permissions: IssuePermissions, comment: &Comment) -> CommentPermissions {
    let own = viewer.is_some_and(|viewer| comment.author == Some(viewer.id()));
    CommentPermissions {
        can_edit: own && permissions.can_comment,
        can_delete: (own && permissions.can_comment) || permissions.can_moderate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Account, AccountKind, AccountName, Email};

    fn user(id: i64) -> User {
        User {
            account: Account {
                id: AccountId(id),
                kind: AccountKind::User,
                name: format!("user{id}").parse::<AccountName>().unwrap(),
                display_name: None,
                bio: None,
                created_at: Timestamp(0),
            },
            email: "user@example.com".parse::<Email>().unwrap(),
            is_admin: false,
        }
    }

    fn issue(kind: IssueKind, locked: bool) -> Issue {
        Issue {
            id: IssueId(1),
            repo_id: RepoId(1),
            number: IssueNumber::new(1).unwrap(),
            kind,
            title: IssueTitle::parse("A bug").unwrap(),
            body: IssueText::default(),
            author: Some(AccountId(10)),
            state: IssueState::Open,
            state_reason: None,
            closed_at: None,
            closed_by: None,
            locked,
            comment_count: 0,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        }
    }

    #[test]
    fn issue_numbers_parse_only_canonical_positive_integers() {
        assert_eq!(IssueNumber::parse("12").unwrap().get(), 12);
        assert_eq!(IssueNumber::parse("4294967295").unwrap().get(), u32::MAX);
        for bad in ["", "0", "012", "-1", "+1", "1.0", " 1", "1 ", "#1", "abc", "4294967296", "99999999999", "１"] {
            assert!(IssueNumber::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(IssueNumber::new(7).unwrap().to_string(), "#7");
        assert_eq!(IssueNumber::new(0), None);
        assert!(IssueNumber::try_from(-3_i64).is_err());
        assert_eq!(IssueNumber::try_from(3_i64).unwrap().get(), 3);
    }

    #[test]
    fn issue_numbers_deserialize_with_validation() {
        assert_eq!(serde_json::from_str::<IssueNumber>("5").unwrap().get(), 5);
        assert!(serde_json::from_str::<IssueNumber>("0").is_err());
        assert_eq!(serde_json::to_string(&IssueNumber::new(5).unwrap()).unwrap(), "5");
    }

    #[test]
    fn text_enums_round_trip() {
        for kind in IssueKind::ALL {
            assert_eq!(IssueKind::parse(kind.as_str()), Some(*kind));
        }
        for state in IssueState::ALL {
            assert_eq!(IssueState::parse(state.as_str()), Some(*state));
        }
        for reason in StateReason::ALL {
            assert_eq!(StateReason::parse(reason.as_str()), Some(*reason));
            assert_eq!(serde_json::to_string(reason).unwrap(), format!("\"{}\"", reason.as_str()));
        }
        for strategy in MergeStrategy::ALL {
            assert_eq!(MergeStrategy::parse(strategy.as_str()), Some(*strategy));
        }
        for state in ReviewState::ALL {
            assert_eq!(ReviewState::parse(state.as_str()), Some(*state));
            assert_eq!(serde_json::to_string(state).unwrap(), format!("\"{}\"", state.as_str()));
        }
        assert_eq!(IssueKind::parse("pr"), None);
        assert_eq!(StateReason::NotPlanned.as_str(), "not_planned");
        assert_eq!(ReviewState::ChangesRequested.as_str(), "changes_requested");
    }

    #[test]
    fn titles_are_trimmed_and_bounded() {
        assert_eq!(IssueTitle::parse("  Crash on start \t").unwrap().as_str(), "Crash on start");
        assert_eq!(IssueTitle::parse("   "), Err(IssueTextError::Empty { kind: "title" }));
        assert_eq!(IssueTitle::parse("a\u{7}b"), Err(IssueTextError::ControlCharacter { kind: "title" }));
        assert_eq!(IssueTitle::parse("line\nbreak"), Err(IssueTextError::ControlCharacter { kind: "title" }));
        assert!(IssueTitle::parse(&"é".repeat(MAX_TITLE_CHARS)).is_ok(), "characters, not bytes");
        assert_eq!(
            IssueTitle::parse(&"x".repeat(MAX_TITLE_CHARS + 1)),
            Err(IssueTextError::TooManyCharacters { kind: "title", max: MAX_TITLE_CHARS })
        );
    }

    #[test]
    fn texts_normalise_line_endings_and_are_bounded() {
        assert_eq!(IssueText::body("a\r\nb\rc\n").unwrap().as_str(), "a\nb\nc\n");
        assert!(IssueText::body("").unwrap().is_empty());
        assert_eq!(IssueText::comment(" \r\n\t"), Err(IssueTextError::Empty { kind: "comment" }));
        assert_eq!(IssueText::comment("LGTM").unwrap().as_str(), "LGTM");
        assert!(IssueText::body(&"x".repeat(MAX_TEXT_BYTES)).is_ok());
        assert_eq!(
            IssueText::body(&"x".repeat(MAX_TEXT_BYTES + 1)),
            Err(IssueTextError::TooLong { kind: "description", max: MAX_TEXT_BYTES })
        );
        // Normalisation happens before the limit: CRLF counts as one byte.
        assert_eq!(IssueText::comment(&"x\r\n".repeat(MAX_TEXT_BYTES / 2)).unwrap().as_str().len(), MAX_TEXT_BYTES);
    }

    #[test]
    fn timeline_events_serialize_as_kind_and_data() {
        let label = LabelSnapshot { id: LabelId(3), name: "bug".to_owned(), color: "#d73a4a".to_owned() };
        let cases = [
            (
                TimelineEvent::Closed { reason: StateReason::Completed, commit: Some("abc".to_owned()), pull: None },
                r#"{"kind":"closed","data":{"reason":"completed","commit":"abc"}}"#,
            ),
            (TimelineEvent::Reopened, r#"{"kind":"reopened"}"#),
            (TimelineEvent::Labeled { label }, r##"{"kind":"labeled","data":{"label":{"id":3,"name":"bug","color":"#d73a4a"}}}"##),
            (TimelineEvent::CrossReferenced { source: IssueId(9) }, r#"{"kind":"cross_referenced","data":{"source":9}}"#),
            (
                TimelineEvent::Merged { commit: "def".to_owned(), strategy: Some(MergeStrategy::Squash) },
                r#"{"kind":"merged","data":{"commit":"def","strategy":"squash"}}"#,
            ),
            (
                TimelineEvent::Merged { commit: "def".to_owned(), strategy: None },
                r#"{"kind":"merged","data":{"commit":"def","strategy":null}}"#,
            ),
            (TimelineEvent::ReadyForReview, r#"{"kind":"ready_for_review"}"#),
        ];
        for (event, json) in cases {
            assert_eq!(serde_json::to_string(&event).unwrap(), json);
            assert_eq!(serde_json::from_str::<TimelineEvent>(json).unwrap(), event);
            assert!(json.contains(&format!("\"kind\":\"{}\"", event.kind())), "{json}");
        }
        assert!(serde_json::from_str::<TimelineEvent>(r#"{"kind":"teleported"}"#).is_err(), "unknown kinds are rejected");
    }

    #[test]
    fn every_timeline_kind_matches_serde() {
        let account = AccountId(1);
        let label = LabelSnapshot { id: LabelId(1), name: "x".to_owned(), color: "#000000".to_owned() };
        let all = [
            TimelineEvent::Closed { reason: StateReason::Merged, commit: None, pull: IssueNumber::new(2) },
            TimelineEvent::Reopened,
            TimelineEvent::Renamed { from: "a".to_owned(), to: "b".to_owned() },
            TimelineEvent::Locked,
            TimelineEvent::Unlocked,
            TimelineEvent::Labeled { label: label.clone() },
            TimelineEvent::Unlabeled { label },
            TimelineEvent::Milestoned { milestone: MilestoneId(1), title: "v1".to_owned() },
            TimelineEvent::Demilestoned { milestone: MilestoneId(1), title: "v1".to_owned() },
            TimelineEvent::Assigned { assignee: account },
            TimelineEvent::Unassigned { assignee: account },
            TimelineEvent::CommitReferenced { commit: "abc".to_owned(), summary: "Fix".to_owned() },
            TimelineEvent::CrossReferenced { source: IssueId(1) },
            TimelineEvent::HeadPushed { before: "a".to_owned(), after: "b".to_owned(), count: 2, forced: true },
            TimelineEvent::BaseChanged { from: "main".to_owned(), to: "dev".to_owned() },
            TimelineEvent::Merged { commit: "c".to_owned(), strategy: Some(MergeStrategy::Rebase) },
            TimelineEvent::ReviewRequested { reviewer: account },
            TimelineEvent::ReviewRequestRemoved { reviewer: account },
            TimelineEvent::ReviewDismissed { review: ReviewId(1), reviewer: None },
            TimelineEvent::HeadRefDeleted { branch: "feature".to_owned() },
            TimelineEvent::ConvertedToDraft,
            TimelineEvent::ReadyForReview,
        ];
        for event in all {
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(value["kind"], event.kind());
            assert_eq!(serde_json::from_value::<TimelineEvent>(value).unwrap(), event);
        }
    }

    /// Who the viewer is relative to the thread.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Viewer {
        Anonymous,
        Author,
        Other,
    }

    /// The permission table of `docs/design/milestone-2b.md`, written out independently.
    fn expected(viewer: Viewer, access: RepoAccess, locked: bool, kind: IssueKind) -> IssuePermissions {
        let signed_in = viewer != Viewer::Anonymous;
        let read = signed_in && access >= RepoAccess::Read;
        let write = signed_in && access >= RepoAccess::Write;
        let admin = signed_in && access >= RepoAccess::Admin;
        let author = read && viewer == Viewer::Author;
        IssuePermissions {
            can_comment: if locked { write } else { read },
            can_edit: author || write,
            can_set_state: author || write,
            can_lock: write,
            can_triage: write,
            can_delete: admin && kind == IssueKind::Issue,
            can_moderate: write,
        }
    }

    #[test]
    fn issue_permissions_follow_the_table_exhaustively() {
        let accesses = [RepoAccess::None, RepoAccess::Read, RepoAccess::Write, RepoAccess::Admin];
        let author = user(10);
        let other = user(20);
        let mut admin = user(30);
        admin.is_admin = true; // site administrators get their power through `access`, not here
        for viewer in [Viewer::Anonymous, Viewer::Author, Viewer::Other] {
            for access in accesses {
                for locked in [false, true] {
                    for kind in IssueKind::ALL.iter().copied() {
                        let thread = issue(kind, locked);
                        let user = match viewer {
                            Viewer::Anonymous => None,
                            Viewer::Author => Some(&author),
                            Viewer::Other => Some(&other),
                        };
                        let actual = issue_permissions(user, access, &thread);
                        assert_eq!(actual, expected(viewer, access, locked, kind), "{viewer:?} {access:?} locked={locked} {kind:?}");
                        if viewer == Viewer::Other {
                            assert_eq!(issue_permissions(Some(&admin), access, &thread), actual, "is_admin alone grants nothing");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_thread_without_author_is_nobody_s() {
        let mut thread = issue(IssueKind::Issue, false);
        thread.author = None;
        let permissions = issue_permissions(Some(&user(10)), RepoAccess::Read, &thread);
        assert!(permissions.can_comment && !permissions.can_edit && !permissions.can_set_state);
    }

    #[test]
    fn comment_permissions_combine_ownership_and_moderation() {
        let author = user(10);
        let other = user(20);
        let comment = Comment {
            id: CommentId(1),
            issue_id: IssueId(1),
            author: Some(author.id()),
            body: IssueText::comment("hi").unwrap(),
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
            edited_at: None,
        };
        let open = issue(IssueKind::Issue, false);
        let locked = issue(IssueKind::Issue, true);
        let check = |viewer: Option<&User>, access, thread: &Issue| {
            let permissions = comment_permissions(viewer, issue_permissions(viewer, access, thread), &comment);
            (permissions.can_edit, permissions.can_delete)
        };
        assert_eq!(check(Some(&author), RepoAccess::Read, &open), (true, true));
        assert_eq!(check(Some(&author), RepoAccess::Read, &locked), (false, false), "locked threads freeze readers");
        assert_eq!(check(Some(&author), RepoAccess::Write, &locked), (true, true));
        assert_eq!(check(Some(&other), RepoAccess::Read, &open), (false, false));
        assert_eq!(check(Some(&other), RepoAccess::Write, &open), (false, true), "writers moderate, never edit");
        assert_eq!(check(None, RepoAccess::Admin, &open), (false, false));
        assert_eq!(check(Some(&author), RepoAccess::None, &open), (false, false));
    }

    #[test]
    fn subject_accounts() {
        assert_eq!(TimelineEvent::Assigned { assignee: AccountId(4) }.subject_account(), Some(AccountId(4)));
        assert_eq!(TimelineEvent::ReviewDismissed { review: ReviewId(1), reviewer: None }.subject_account(), None);
        assert_eq!(TimelineEvent::Locked.subject_account(), None);
    }
}
