//! Stable machine reasons, ordering and original English label compatibility.
/// Why a user hears about a thread, weakest first; the strongest wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NotificationReason {
    /// Subscribed by hand.
    Manual,
    Comment,
    /// Reviewed the pull request.
    Reviewed,
    /// Opened the thread.
    Author,
    Mention,
    Assigned,
    ReviewRequested,
}

impl NotificationReason {
    pub const ALL: [Self; 7] =
        [Self::Manual, Self::Comment, Self::Reviewed, Self::Author, Self::Mention, Self::Assigned, Self::ReviewRequested];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Comment => "comment",
            Self::Reviewed => "reviewed",
            Self::Author => "author",
            Self::Mention => "mention",
            Self::Assigned => "assigned",
            Self::ReviewRequested => "review_requested",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == value)
    }

    /// How the inbox explains it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Manual => "Subscribed",
            Self::Comment => "Commented",
            Self::Reviewed => "Reviewed",
            Self::Author => "Author",
            Self::Mention => "Mentioned",
            Self::Assigned => "Assigned",
            Self::ReviewRequested => "Review requested",
        }
    }
}
