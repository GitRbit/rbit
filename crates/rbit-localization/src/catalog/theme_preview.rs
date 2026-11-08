//! Labels of the theme picker's miniature page. English only so far: every other
//! locale falls back (see [`super::fallback`]). The sample code itself is not text.
use super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemePreviewMessage {
    Star,
    Clone,
    Code,
    Issues,
    PullRequests,
    Releases,
    Lines { count: u64 },
    ChecksPassed,
    ReviewPending,
    Failing { count: u64 },
    Tagline,
    Link,
    Updated,
}

impl MessageFamily for ThemePreviewMessage {
    fn id(self) -> &'static str {
        match self {
            Self::Star => "themes.preview.star",
            Self::Clone => "themes.preview.clone",
            Self::Code => "themes.preview.code",
            Self::Issues => "themes.preview.issues",
            Self::PullRequests => "themes.preview.pull-requests",
            Self::Releases => "themes.preview.releases",
            Self::Lines { .. } => "themes.preview.lines",
            Self::ChecksPassed => "themes.preview.checks-passed",
            Self::ReviewPending => "themes.preview.review-pending",
            Self::Failing { .. } => "themes.preview.failing",
            Self::Tagline => "themes.preview.tagline",
            Self::Link => "themes.preview.link",
            Self::Updated => "themes.preview.updated",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::Lines { count } => {
                text.number(count);
                return text.literal(if count == 1 { " line" } else { " lines" });
            }
            Self::Failing { count } => {
                text.number(count);
                return text.literal(" failing");
            }
            Self::Star => "Star",
            Self::Clone => "Clone",
            Self::Code => "Code",
            Self::Issues => "Issues",
            Self::PullRequests => "Pull requests",
            Self::Releases => "Releases",
            Self::ChecksPassed => "Checks passed",
            Self::ReviewPending => "Review pending",
            Self::Tagline => "Self-hosted Git, fast and quiet.",
            Self::Link => "Read more",
            Self::Updated => "Updated 2 hours ago",
        };
        text.literal(literal);
    }
}
