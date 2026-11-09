//! Where a line comment is anchored: a side of the unified diff and the diff lines kept
//! with the comment.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Diff lines kept with a line comment, ending at the commented one.
pub const DIFF_HUNK_LINES: usize = 4;

/// The side of a unified diff a review comment is anchored to: `Left` is the old file
/// (a removed line), `Right` the new one (an added or context line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSide {
    Left,
    Right,
}

impl ReviewSide {
    pub const ALL: &'static [Self] = &[Self::Left, Self::Right];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

impl fmt::Display for ReviewSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How one diff line changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMark {
    Context,
    Added,
    Removed,
}

/// One diff line as the anchoring sees it.
#[derive(Debug, Clone, Copy)]
pub struct AnchorLine<'a> {
    pub mark: LineMark,
    pub old_number: Option<usize>,
    pub new_number: Option<usize>,
    pub text: &'a str,
}

/// Up to [`DIFF_HUNK_LINES`] diff lines ending at `line` on `side`, after their hunk
/// header, each prefixed with `+`, `-` or a space. The first hunk with that line wins;
/// `None` when no hunk has it. `view` reads the caller's line type.
pub fn hunk_excerpt<'a, L: 'a>(
    hunks: impl IntoIterator<Item = (&'a str, &'a [L])>,
    side: ReviewSide,
    line: u32,
    view: impl Fn(&'a L) -> AnchorLine<'a>,
) -> Option<String> {
    let line = usize::try_from(line).ok()?;
    for (header, lines) in hunks {
        let position = lines.iter().map(&view).position(|diff_line| match side {
            ReviewSide::Left => diff_line.mark != LineMark::Added && diff_line.old_number == Some(line),
            ReviewSide::Right => diff_line.mark != LineMark::Removed && diff_line.new_number == Some(line),
        });
        if let Some(end) = position {
            let start = (end + 1).saturating_sub(DIFF_HUNK_LINES);
            let mut excerpt = header.to_owned();
            for diff_line in lines[start..=end].iter().map(&view) {
                let sign = match diff_line.mark {
                    LineMark::Added => '+',
                    LineMark::Removed => '-',
                    LineMark::Context => ' ',
                };
                excerpt.push('\n');
                excerpt.push(sign);
                excerpt.push_str(diff_line.text);
            }
            return Some(excerpt);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    type Line = (LineMark, Option<usize>, Option<usize>, &'static str);

    fn view(line: &Line) -> AnchorLine<'_> {
        AnchorLine { mark: line.0, old_number: line.1, new_number: line.2, text: line.3 }
    }

    #[test]
    fn excerpts_end_at_the_commented_line() {
        use LineMark::{Added, Context, Removed};
        let lines: Vec<Line> = vec![
            (Context, Some(1), Some(1), "a"),
            (Context, Some(2), Some(2), "b"),
            (Removed, Some(3), None, "c"),
            (Added, None, Some(3), "C"),
            (Context, Some(4), Some(4), "d"),
        ];
        let hunks = [("@@ -1,4 +1,4 @@", lines.as_slice())];
        assert_eq!(hunk_excerpt(hunks, ReviewSide::Right, 3, view).as_deref(), Some("@@ -1,4 +1,4 @@\n a\n b\n-c\n+C"));
        assert_eq!(hunk_excerpt(hunks, ReviewSide::Left, 3, view).as_deref(), Some("@@ -1,4 +1,4 @@\n a\n b\n-c"));
        assert_eq!(hunk_excerpt(hunks, ReviewSide::Right, 4, view).as_deref(), Some("@@ -1,4 +1,4 @@\n b\n-c\n+C\n d"));
        assert_eq!(hunk_excerpt(hunks, ReviewSide::Left, 9, view), None);
        assert_eq!(ReviewSide::parse(ReviewSide::Left.as_str()), Some(ReviewSide::Left));
    }
}
