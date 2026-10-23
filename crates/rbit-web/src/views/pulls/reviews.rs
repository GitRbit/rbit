//! Pull request review and conversation view models.

use super::{
    ActorView, PullUrls, RefContext, ReviewComment, ReviewData, ReviewEntry, ReviewSide, ReviewState, TimelineItemView, line_anchor,
    render_issue_markdown, review_anchor, review_comment_anchor,
};

/// Resolution state and the action available to the current viewer.
#[derive(Debug, Clone, Default)]
pub struct ThreadResolutionView {
    pub resolved: bool,
    /// Only offered to a writer or the submitted root's author.
    pub action_url: Option<String>,
}

/// A review comment as shown in the conversation or under a diff line.
#[derive(Debug, Clone)]
pub struct ReviewCommentView {
    pub anchor: String,
    pub author: ActorView,
    pub body_html: String,
    pub when: String,
    pub when_iso: String,
    pub path: String,
    /// `L12` / `R12`.
    pub line_label: String,
    /// The stored diff excerpt, one `(css kind, text)` per line after the header (app.css
    /// draws the sign from the kind).
    pub hunk_header: String,
    pub hunk: Vec<(&'static str, String)>,
    pub outdated: bool,
    pub pending: bool,
    pub is_reply: bool,
    /// Where the comment's line is on the Files changed tab.
    pub line_url: String,
    /// `POST` target of the reply form.
    pub reply_to: i64,
    pub resolution: ThreadResolutionView,
    pub replies: Vec<Self>,
}

impl ReviewCommentView {
    pub fn new(comment: &ReviewComment, author: Option<&rbit_core::Account>, pending: bool, urls: &PullUrls, refs: &RefContext) -> Self {
        let mut lines = comment.diff_hunk.lines();
        let hunk_header = lines.next().unwrap_or_default().to_owned();
        let hunk = lines
            .map(|line| {
                let kind = match line.chars().next() {
                    Some('+') => "add",
                    Some('-') => "del",
                    _ => "ctx",
                };
                (kind, line.get(1..).unwrap_or_default().to_owned())
            })
            .collect();
        let side = match comment.side {
            ReviewSide::Left => 'L',
            ReviewSide::Right => 'R',
        };
        Self {
            anchor: review_comment_anchor(comment.id.0),
            author: ActorView::new(author),
            body_html: render_issue_markdown(&comment.body, refs),
            when: comment.created_at.relative(),
            when_iso: comment.created_at.iso8601(),
            path: comment.path.clone(),
            line_label: format!("{side}{}", comment.line),
            hunk_header,
            hunk,
            outdated: comment.outdated,
            pending,
            is_reply: comment.in_reply_to.is_some(),
            line_url: format!("{}#{}", urls.files, line_anchor(&comment.path, comment.side, comment.line)),
            reply_to: comment.in_reply_to.unwrap_or(comment.id).0,
            resolution: ThreadResolutionView { resolved: comment.resolved_at.is_some(), action_url: None },
            replies: Vec::new(),
        }
    }
}

/// A submitted review in the conversation.
#[derive(Debug, Clone)]
pub struct ReviewView {
    pub anchor: String,
    pub id: i64,
    pub reviewer: ActorView,
    /// "approved these changes", "requested changes", "reviewed", …
    pub sentence: &'static str,
    pub icon: &'static str,
    /// `text-success`, `text-danger` or `text-ink-muted`.
    pub tone: &'static str,
    pub body_html: String,
    pub when: String,
    pub when_iso: String,
    pub comments: Vec<ReviewCommentView>,
    pub dismissal: Option<String>,
    pub can_dismiss: bool,
    pub dismiss_url: String,
}

impl ReviewView {
    pub fn new(entry: &ReviewEntry, comments: Vec<ReviewCommentView>, can_dismiss: bool, urls: &PullUrls, refs: &RefContext) -> Self {
        let review = &entry.review;
        let (sentence, icon, tone) = match review.state {
            ReviewState::Approved => ("approved these changes", "circle-check", "text-success"),
            ReviewState::ChangesRequested => ("requested changes", "file-diff", "text-danger"),
            ReviewState::Dismissed => ("reviewed (dismissed)", "eye-off", "text-ink-muted"),
            ReviewState::Commented | ReviewState::Pending => ("reviewed", "eye", "text-ink-muted"),
        };
        let at = review.submitted_at.unwrap_or(review.created_at);
        let dismissal = review.dismissal_message.as_ref().map(|message| {
            let by = entry.dismissed_by.as_ref().map_or_else(|| "ghost".to_owned(), |account| account.name.to_string());
            format!("Dismissed by {by}: {message}")
        });
        Self {
            anchor: review_anchor(review.id.0),
            id: review.id.0,
            reviewer: ActorView::new(entry.reviewer.as_ref()),
            sentence,
            icon,
            tone,
            body_html: if review.body.trim().is_empty() { String::new() } else { render_issue_markdown(&review.body, refs) },
            when: at.relative(),
            when_iso: at.iso8601(),
            comments,
            dismissal,
            can_dismiss: can_dismiss && matches!(review.state, ReviewState::Approved | ReviewState::ChangesRequested),
            dismiss_url: urls.dismiss(review.id.0),
        }
    }
}

/// One entry of the conversation: a core timeline item or a review (exactly one is set).
#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub item: Option<TimelineItemView>,
    pub review: Option<ReviewView>,
}

/// T0's items and the reviews, ordered by time (reviews after items of the same second).
pub fn interleave(items: Vec<TimelineItemView>, reviews: Vec<(i64, ReviewView)>) -> Vec<ConversationEntry> {
    let mut keyed: Vec<(i64, u8, usize, ConversationEntry)> = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| (item.cursor.at.seconds(), 0, index, ConversationEntry { item: Some(item), review: None }))
        .collect();
    keyed.extend(
        reviews.into_iter().enumerate().map(|(index, (at, review))| (at, 1, index, ConversationEntry { item: None, review: Some(review) })),
    );
    keyed.sort_by_key(|(at, rank, index, _)| (*at, *rank, *index));
    keyed.into_iter().map(|(_, _, _, entry)| entry).collect()
}

/// A reviewer in the sidebar with their latest state.
#[derive(Debug, Clone)]
pub struct ReviewerView {
    pub actor: ActorView,
    pub name: String,
    /// "Approved", "Changes requested", "Commented", "Requested", "Dismissed".
    pub state: &'static str,
    pub tone: &'static str,
    pub requested: bool,
}

/// The reviewers block of the sidebar.
#[derive(Debug, Clone)]
pub struct ReviewersView {
    pub reviewers: Vec<ReviewerView>,
    pub can_request: bool,
    pub request_url: String,
    pub remove_url: String,
}

impl ReviewersView {
    pub fn new(data: &ReviewData, can_request: bool, urls: &PullUrls) -> Self {
        let mut reviewers: Vec<ReviewerView> = data
            .latest
            .iter()
            .map(|(account, state)| {
                let (state, tone) = review_state_label(*state);
                ReviewerView { actor: ActorView::new(Some(account)), name: account.name.to_string(), state, tone, requested: false }
            })
            .collect();
        for account in &data.requested {
            match reviewers.iter_mut().find(|reviewer| reviewer.name == account.name.as_str()) {
                Some(reviewer) => reviewer.requested = true,
                None => reviewers.push(ReviewerView {
                    actor: ActorView::new(Some(account)),
                    name: account.name.to_string(),
                    state: "Awaiting review",
                    tone: "text-warning",
                    requested: true,
                }),
            }
        }
        Self { reviewers, can_request, request_url: urls.reviewers.clone(), remove_url: urls.remove_reviewer.clone() }
    }
}

fn review_state_label(state: ReviewState) -> (&'static str, &'static str) {
    match state {
        ReviewState::Approved => ("Approved", "text-success"),
        ReviewState::ChangesRequested => ("Changes requested", "text-danger"),
        ReviewState::Dismissed => ("Dismissed", "text-ink-faint"),
        ReviewState::Commented => ("Commented", "text-ink-muted"),
        ReviewState::Pending => ("Pending", "text-warning"),
    }
}

/// The submit-review form (conversation and Files changed).
#[derive(Debug, Clone)]
pub struct ReviewFormView {
    pub action_url: String,
    pub discard_url: String,
    pub pending_comments: usize,
    /// Approve and Request changes are offered (not to the author).
    pub can_decide: bool,
}
