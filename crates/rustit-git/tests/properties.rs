//! Property tests for line diffs: hunk headers agree with the lines they introduce, line
//! numbers are consecutive, totals add up, and applying the hunks to the old text
//! reproduces the new text exactly.

use proptest::prelude::*;
use rustit_git::diff::{Hunk, LineKind, diff_bytes};

/// A line as the diff sees it: its text without the terminator, and whether it had one.
type Line = (String, bool);

fn lines_of(text: &str) -> Vec<Line> {
    text.split_inclusive('\n').map(|line| line.strip_suffix('\n').map_or((line.to_owned(), false), |l| (l.to_owned(), true))).collect()
}

/// Texts made of few distinct lines, so diffs mix context, removals and additions.
fn text() -> impl Strategy<Value = String> {
    (prop::collection::vec(prop::sample::select(vec!["alpha", "beta", "gamma", "delta", "", "  indented", "x"]), 0..40), any::<bool>())
        .prop_map(|(lines, trailing_newline)| {
            let mut text = lines.join("\n");
            if trailing_newline && !lines.is_empty() {
                text.push('\n');
            }
            text
        })
}

/// Parses `@@ -a[,b] +c[,d] @@` into `((a, b), (c, d))`, with Git's implicit `,1`.
fn parse_header(header: &str) -> ((usize, usize), (usize, usize)) {
    let ranges = header.strip_prefix("@@ -").and_then(|rest| rest.strip_suffix(" @@")).expect("@@ … @@");
    let (old, new) = ranges.split_once(" +").expect("two ranges");
    let range = |range: &str| match range.split_once(',') {
        Some((start, len)) => (start.parse().expect("start"), len.parse().expect("length")),
        None => (range.parse().expect("start"), 1),
    };
    (range(old), range(new))
}

/// Applies `hunks` to `old`, checking every context and removed line against it.
fn apply(old: &[Line], hunks: &[Hunk]) -> Result<Vec<Line>, String> {
    let mut result = Vec::new();
    let mut next_old = 0; // index of the next old line not yet consumed
    for hunk in hunks {
        let first_old = hunk.lines.iter().find_map(|line| line.old_number);
        if let Some(number) = first_old {
            let start = number - 1;
            if start < next_old {
                return Err(format!("hunk {} overlaps the previous one", hunk.header));
            }
            result.extend_from_slice(&old[next_old..start]);
            next_old = start;
        }
        for line in &hunk.lines {
            match line.kind {
                LineKind::Context | LineKind::Removed => {
                    let expected = old.get(next_old).ok_or("hunk runs past the old text")?;
                    if line.old_number != Some(next_old + 1) || expected.0 != line.text {
                        return Err(format!("{:?} does not match old line {}", line.text, next_old + 1));
                    }
                    if line.kind == LineKind::Context {
                        result.push(expected.clone());
                    }
                    next_old += 1;
                }
                LineKind::Added => result.push((line.text.clone(), !line.missing_newline)),
            }
        }
    }
    result.extend_from_slice(&old[next_old..]);
    Ok(result)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn hunk_headers_match_their_lines(old in text(), new in text()) {
        let changes = diff_bytes(old.as_bytes(), new.as_bytes());
        for hunk in &changes.hunks {
            let ((old_start, old_len), (new_start, new_len)) = parse_header(&hunk.header);
            let old_lines: Vec<usize> = hunk.lines.iter().filter_map(|l| l.old_number).collect();
            let new_lines: Vec<usize> = hunk.lines.iter().filter_map(|l| l.new_number).collect();
            prop_assert_eq!(old_lines.len(), old_len, "old count in {}", hunk.header);
            prop_assert_eq!(new_lines.len(), new_len, "new count in {}", hunk.header);
            // Non-empty ranges start at their first line; empty ones name the line before.
            prop_assert_eq!(old_lines.first().map_or(old_start + 1, |&n| n), old_start + usize::from(old_len == 0));
            prop_assert_eq!(new_lines.first().map_or(new_start + 1, |&n| n), new_start + usize::from(new_len == 0));
            // Numbers are consecutive on each side.
            prop_assert!(old_lines.windows(2).all(|w| w[1] == w[0] + 1), "old numbers in {}", hunk.header);
            prop_assert!(new_lines.windows(2).all(|w| w[1] == w[0] + 1), "new numbers in {}", hunk.header);
            prop_assert!(hunk.lines.iter().any(|l| l.kind != LineKind::Context), "a hunk without changes: {}", hunk.header);
        }
    }

    #[test]
    fn totals_count_the_changed_lines(old in text(), new in text()) {
        let changes = diff_bytes(old.as_bytes(), new.as_bytes());
        let count = |kind| changes.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == kind).count();
        prop_assert_eq!(changes.additions, count(LineKind::Added));
        prop_assert_eq!(changes.deletions, count(LineKind::Removed));
        prop_assert_eq!(changes.line_count(), changes.hunks.iter().map(|h| h.lines.len()).sum::<usize>());
        prop_assert_eq!(changes.hunks.is_empty(), old == new);
    }

    #[test]
    fn applying_the_diff_reproduces_the_new_text(old in text(), new in text()) {
        let changes = diff_bytes(old.as_bytes(), new.as_bytes());
        prop_assert_eq!(apply(&lines_of(&old), &changes.hunks), Ok(lines_of(&new)));
    }

    #[test]
    fn diffing_is_symmetric_in_size(old in text(), new in text()) {
        let forward = diff_bytes(old.as_bytes(), new.as_bytes());
        let backward = diff_bytes(new.as_bytes(), old.as_bytes());
        prop_assert_eq!(forward.additions, backward.deletions);
        prop_assert_eq!(forward.deletions, backward.additions);
    }
}
