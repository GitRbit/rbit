//! What one notification email says about an activity, and its dedup key.
use super::policy::{capitalised, excerpt_of, kind_label};
use rbit_collaboration::{
    comments::CommentId,
    threads::{IssueKind, StateReason},
};
use rbit_contracts::{AccountId, Timestamp};

/// An activity that sends email, with the content composition already loaded for it.
/// Events that send none (deleted comments, title edits, other actions, rows that no
/// longer exist) never become a `MailEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailEvent<'a> {
    Opened,
    /// The description changed; only users newly mentioned in it receive this.
    BodyEdited,
    Assigned(AccountId),
    ReviewRequested(AccountId),
    Closed(StateReason),
    Reopened,
    Comment {
        id: CommentId,
        /// Only users newly mentioned by an edit receive it.
        edited: bool,
        /// Written by an rbit Actions task rather than a person.
        by_actions: bool,
        body: &'a str,
    },
    Review {
        id: i64,
        verdict: ReviewVerdict,
        body: &'a str,
    },
}

/// How a submitted review ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approved,
    ChangesRequested,
    Commented,
}

/// The thread an activity happened on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailThread<'a> {
    pub kind: IssueKind,
    pub number: u32,
    pub body: &'a str,
}

/// What an event did, as one email describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailActivity {
    /// The dedup key, unique per thread and activity; also ends the `Message-ID`.
    pub key: String,
    pub headline: String,
    pub excerpt: String,
    /// The fragment of the thread page to link to.
    pub anchor: Option<String>,
    /// Whether this message starts the thread (its `Message-ID` is the thread's).
    pub opening: bool,
}

/// Describes `event` on `thread` by `actor` (a username; `None` for automation) at `now`.
/// Repeated activities (assignment, state changes) are keyed by time; content activities
/// by their row, so a re-delivered event is never mailed twice.
pub fn describe(event: MailEvent<'_>, thread: MailThread<'_>, actor: Option<&str>, now: Timestamp) -> MailActivity {
    let what = format!("{} #{}", kind_label(thread.kind), thread.number);
    let who = actor.unwrap_or("Someone");
    let stamped = |name: &str| format!("{name}-{}", now.seconds());
    let activity = |key: String, headline: String, excerpt: &str, anchor: Option<String>, opening: bool| MailActivity {
        key,
        headline,
        excerpt: excerpt_of(excerpt),
        anchor,
        opening,
    };
    match event {
        MailEvent::Opened => activity("opened".into(), format!("{who} opened {what}."), thread.body, None, true),
        MailEvent::BodyEdited => activity("opened".into(), format!("{who} mentioned you in {what}."), thread.body, None, true),
        MailEvent::Assigned(assignee) => {
            activity(stamped(&format!("assigned-{}", assignee.0)), format!("{who} assigned you to {what}."), "", None, false)
        }
        MailEvent::ReviewRequested(reviewer) => activity(
            stamped(&format!("review-requested-{}", reviewer.0)),
            format!("{who} requested your review on {what}."),
            "",
            None,
            false,
        ),
        MailEvent::Closed(reason) => {
            let headline = match (actor, reason) {
                (None, StateReason::Merged) => capitalised(&format!("{what} was merged.")),
                (None, _) => capitalised(&format!("{what} was closed.")),
                (Some(actor), StateReason::Merged) => format!("{actor} merged {what}."),
                (Some(actor), StateReason::Completed) => format!("{actor} closed {what} as completed."),
                (Some(actor), StateReason::NotPlanned) => format!("{actor} closed {what} as not planned."),
            };
            let name = if reason == StateReason::Merged { "merged" } else { "closed" };
            activity(stamped(name), headline, "", None, false)
        }
        MailEvent::Reopened => activity(stamped("reopened"), format!("{who} reopened {what}."), "", None, false),
        MailEvent::Comment { id, edited, by_actions, body } => {
            // A workflow's comment has no author; rbit Actions wrote it.
            let who = if by_actions { "rbit Actions" } else { who };
            let headline =
                if edited { format!("{who} mentioned you in a comment on {what}.") } else { format!("{who} commented on {what}.") };
            activity(format!("comment-{}", id.0), headline, body, Some(format!("issuecomment-{}", id.0)), false)
        }
        MailEvent::Review { id, verdict, body } => {
            let headline = match verdict {
                ReviewVerdict::Approved => format!("{who} approved {what}."),
                ReviewVerdict::ChangesRequested => format!("{who} requested changes on {what}."),
                ReviewVerdict::Commented => format!("{who} reviewed {what}."),
            };
            activity(format!("review-{id}"), headline, body, Some(format!("pullrequestreview-{id}")), false)
        }
    }
}
