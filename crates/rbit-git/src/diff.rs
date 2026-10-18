//! Line-level diffs between two versions of a file, shaped for display.
//!
//! Pure functions over byte slices: no repository access happens here, which keeps the
//! hunk logic unit-testable. Diffing works on raw bytes split at `\n` only — exactly like
//! Git — so non-UTF-8 edits are detected and line numbers match `git diff` and the blob
//! view; text is decoded (lossily) only for display.

use std::time::{Duration, Instant};

use similar::{Algorithm, DiffTag, capture_diff_slices_deadline, group_diff_ops};

/// Unchanged lines shown around each change.
const CONTEXT_LINES: usize = 3;
/// Pathological inputs fall back to a coarser (but still correct) diff after this long.
const DIFF_DEADLINE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_number: Option<usize>,
    pub new_number: Option<usize>,
    /// The line without its terminator, decoded for display.
    pub text: String,
    /// The line is the last in its file and has no trailing newline (`\ No newline at end of file`).
    pub missing_newline: bool,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    /// `@@ -a,b +c,d @@`, formatted exactly as Git does.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// The textual difference between two blobs.
#[derive(Debug, Clone, Default)]
pub struct TextChanges {
    pub hunks: Vec<Hunk>,
    pub additions: usize,
    pub deletions: usize,
}

impl TextChanges {
    /// Number of rendered diff lines (context included).
    pub fn line_count(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }
}

/// Splits at `\n`, keeping terminators, so a lone `\r` never starts a new line.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split_inclusive(|b| *b == b'\n').collect()
}

fn display(line: &[u8]) -> String {
    let without_newline = line.strip_suffix(b"\n").unwrap_or(line);
    let without_cr = without_newline.strip_suffix(b"\r").unwrap_or(without_newline);
    String::from_utf8_lossy(without_cr).into_owned()
}

/// Git's range notation: `start,len`, omitting `,1`, and naming the preceding line for empty ranges.
fn git_range(start: usize, len: usize) -> String {
    match len {
        0 => format!("{start},0"),
        1 => (start + 1).to_string(),
        _ => format!("{},{len}", start + 1),
    }
}

/// Diffs two blobs into hunks with surrounding context.
pub fn diff_bytes(old: &[u8], new: &[u8]) -> TextChanges {
    diff_bytes_bounded(old, new, usize::MAX).0
}

/// Keep exact addition/deletion counts, but omit all hunks when their display lines
/// would exceed `max_lines`. The flag distinguishes an omitted body from identical text.
pub(crate) fn diff_bytes_bounded(old: &[u8], new: &[u8], max_lines: usize) -> (TextChanges, bool) {
    diff_bytes_with_display(old, new, max_lines, display)
}

fn diff_bytes_with_display(old: &[u8], new: &[u8], max_lines: usize, mut display_line: impl FnMut(&[u8]) -> String) -> (TextChanges, bool) {
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let ops = capture_diff_slices_deadline(Algorithm::Myers, &old_lines, &new_lines, Some(Instant::now() + DIFF_DEADLINE));
    let groups = group_diff_ops(ops, CONTEXT_LINES);
    let mut changes = TextChanges::default();
    let mut rendered_lines = 0;
    // Operations describe ranges, so count them without decoding or allocating a
    // display String for each line. In particular, an oversized file renders nothing.
    for op in groups.iter().flatten() {
        let (tag, old, new) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            rendered_lines += old.len();
        } else {
            changes.deletions += old.len();
            changes.additions += new.len();
            rendered_lines += old.len() + new.len();
        }
    }
    if rendered_lines > max_lines {
        return (changes, true);
    }

    let mut line = |kind, old_index: Option<usize>, new_index: Option<usize>| {
        let (source, index) = match (kind, old_index, new_index) {
            (LineKind::Added, _, Some(i)) | (LineKind::Context, None, Some(i)) => (&new_lines, i),
            (_, Some(i), _) => (&old_lines, i),
            _ => unreachable!("every diff line has at least one side"),
        };
        let raw = source[index];
        DiffLine {
            kind,
            old_number: old_index.map(|i| i + 1),
            new_number: new_index.map(|i| i + 1),
            text: display_line(raw),
            missing_newline: index + 1 == source.len() && !raw.ends_with(b"\n"),
        }
    };

    for group in groups {
        let (Some(first), Some(last)) = (group.first(), group.last()) else { continue };
        let old_range = first.old_range().start..last.old_range().end;
        let new_range = first.new_range().start..last.new_range().end;
        let header = format!("@@ -{} +{} @@", git_range(old_range.start, old_range.len()), git_range(new_range.start, new_range.len()));

        let mut lines = Vec::new();
        for op in &group {
            let (tag, old_ops, new_ops) = op.as_tag_tuple();
            match tag {
                DiffTag::Equal => {
                    lines.extend(old_ops.zip(new_ops).map(|(o, n)| line(LineKind::Context, Some(o), Some(n))));
                }
                DiffTag::Delete | DiffTag::Insert | DiffTag::Replace => {
                    lines.extend(old_ops.map(|o| line(LineKind::Removed, Some(o), None)));
                    lines.extend(new_ops.map(|n| line(LineKind::Added, None, Some(n))));
                }
            }
        }
        changes.hunks.push(Hunk { header, lines });
    }
    (changes, false)
}

/// Git's heuristic: a NUL byte in the first 8000 bytes means binary.
pub fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(8000)].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(old: &[u8], new: &[u8]) -> Vec<String> {
        diff_bytes(old, new).hunks.into_iter().map(|h| h.header).collect()
    }

    #[test]
    fn counts_and_numbers_changes() {
        let changes = diff_bytes(b"a\nb\nc\nd\n", b"a\nB\nc\nd\ne\n");
        assert_eq!((changes.additions, changes.deletions), (2, 1));
        assert_eq!(changes.hunks.len(), 1);

        let lines = &changes.hunks[0].lines;
        let removed = lines.iter().find(|l| l.kind == LineKind::Removed).unwrap();
        assert_eq!((removed.old_number, removed.new_number, removed.text.as_str()), (Some(2), None, "b"));
        let added_last = lines.iter().rev().find(|l| l.kind == LineKind::Added).unwrap();
        assert_eq!((added_last.new_number, added_last.text.as_str()), (Some(5), "e"));
    }

    #[test]
    fn distant_changes_form_separate_hunks() {
        let old: String = (1..=30).map(|i| i.to_string() + "\n").collect();
        let new: String = (1..=30)
            .map(|i| match i {
                2 => "two\n".to_owned(),
                28 => "twenty-eight\n".to_owned(),
                _ => i.to_string() + "\n",
            })
            .collect();
        assert_eq!(diff_bytes(old.as_bytes(), new.as_bytes()).hunks.len(), 2);
    }

    #[test]
    fn identical_text_has_no_hunks() {
        assert!(diff_bytes(b"same\n", b"same\n").hunks.is_empty());
    }

    /// Expected values taken from `git diff -U3`.
    #[test]
    fn hunk_headers_match_git() {
        assert_eq!(headers(b"", b"a\nb\nc\n"), ["@@ -0,0 +1,3 @@"]);
        assert_eq!(headers(b"a\nb\nc\n", b""), ["@@ -1,3 +0,0 @@"]);
        assert_eq!(headers(b"a\n", b"b\n"), ["@@ -1 +1 @@"]);
    }

    #[test]
    fn non_utf8_edits_are_detected() {
        let changes = diff_bytes(b"caf\xe9\n", b"caf\xe8\n");
        assert_eq!((changes.additions, changes.deletions), (1, 1));
    }

    #[test]
    fn lone_carriage_return_is_not_a_line_break() {
        // Matches git: the change to `f` is on line 6, so the hunk starts at line 3.
        assert_eq!(headers(b"a\rb\nc\nd\ne\nf\n", b"a\rb\nc\nd\ne\nF\n"), ["@@ -2,4 +2,4 @@"]);
    }

    #[test]
    fn flags_missing_trailing_newline() {
        let changes = diff_bytes(b"a\nb", b"a\nc");
        let last_removed = changes.hunks[0].lines.iter().find(|l| l.kind == LineKind::Removed).unwrap();
        assert!(last_removed.missing_newline);
    }

    #[test]
    fn over_budget_files_never_allocate_display_lines() {
        let data = b"new line\n".repeat(20_000);
        let (changes, omitted) = diff_bytes_with_display(b"", &data, 1, |_| panic!("an omitted line was rendered"));
        assert!(omitted);
        assert_eq!((changes.additions, changes.deletions), (20_000, 0));
        assert!(changes.hunks.is_empty());
    }

    #[test]
    fn display_budgets_preserve_counts_and_complete_hunks() {
        use std::fmt::Write;

        let mut old = String::new();
        for i in 1..=30 {
            writeln!(old, "{i}").unwrap();
        }
        let new = old.replace("2\n", "two\n").replace("28\n", "twenty-eight\n");
        let cases: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"same\n", b"same\n"),
            (b"", b"first\nlast"),
            (b"first\nlast", b""),
            (b"a\rb\nc\r\n", b"a\rb\nC\r\n"),
            (b"caf\xe9\n", b"caf\xe8"),
            (old.as_bytes(), new.as_bytes()),
        ];
        for &(old, new) in cases {
            let expected = diff_bytes(old, new);
            let lines = expected.line_count();
            for limit in [0, 1, lines.saturating_sub(1), lines, lines + 1] {
                let mut displayed = 0;
                let (actual, omitted) = diff_bytes_with_display(old, new, limit, |line| {
                    displayed += 1;
                    display(line)
                });
                assert_eq!((actual.additions, actual.deletions), (expected.additions, expected.deletions));
                assert_eq!(omitted, lines > limit);
                if omitted {
                    assert_eq!(displayed, 0, "no partial hunks should be built and discarded");
                    assert!(actual.hunks.is_empty());
                } else {
                    assert_eq!(displayed, lines);
                    assert_eq!(format!("{:?}", actual.hunks), format!("{:?}", expected.hunks));
                }
            }
        }
    }
}
