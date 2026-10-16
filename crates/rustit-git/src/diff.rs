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
    let old_lines = split_lines(old);
    let new_lines = split_lines(new);
    let ops = capture_diff_slices_deadline(Algorithm::Myers, &old_lines, &new_lines, Some(Instant::now() + DIFF_DEADLINE));

    let line = |kind, old_index: Option<usize>, new_index: Option<usize>| {
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
            text: display(raw),
            missing_newline: index + 1 == source.len() && !raw.ends_with(b"\n"),
        }
    };

    let mut changes = TextChanges::default();
    for group in group_diff_ops(ops, CONTEXT_LINES) {
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
                    changes.deletions += old_ops.len();
                    changes.additions += new_ops.len();
                    lines.extend(old_ops.map(|o| line(LineKind::Removed, Some(o), None)));
                    lines.extend(new_ops.map(|n| line(LineKind::Added, None, Some(n))));
                }
            }
        }
        changes.hunks.push(Hunk { header, lines });
    }
    changes
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
}
