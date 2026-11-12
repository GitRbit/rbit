//! What a reviewer asked for and what changed, without reading a diff (agent plan steps
//! 5.1 and 5.2).
//!
//! * The diff manifest lists the changed files with their old and new blob ids, their
//!   status and line counts, and no patch text; one file's hunks, or one hunk, on request.
//! * The review inbox lists the unresolved threads a viewer can see, each with its anchor
//!   and short excerpts. An anchor on a file that changed since the comment is looked up in
//!   the head: the commented line found exactly once is `moved` there, otherwise the anchor
//!   is `ambiguous` (the line is gone, or several lines match).

use rbit_core::pull::{ReviewComment, ReviewSide};
use rbit_git::{ChangeStatus, DiffLimits, FileDiffBody, diff::Hunk};

use super::{PullContext, PullService};
use crate::ServiceError;

/// Bytes of a blob read to map an outdated anchor.
const ANCHOR_BLOB_BYTES: usize = 1024 * 1024;
/// Threads the inbox lists.
const INBOX_THREADS: usize = 50;
/// Characters of a comment an inbox excerpt keeps.
const EXCERPT_CHARS: usize = 280;

/// One changed file of the manifest.
#[derive(Debug, Clone)]
pub struct ManifestFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: &'static str,
    pub additions: usize,
    pub deletions: usize,
    pub stats_known: bool,
    pub binary: bool,
    pub old_blob: Option<String>,
    pub new_blob: Option<String>,
}

/// The diff manifest of a pull request.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub base: String,
    pub head: String,
    pub files: Vec<ManifestFile>,
}

/// Where an inbox thread points now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anchor {
    /// The file did not change since the comment.
    Current,
    /// The commented line is on `line` of the head now.
    Moved { line: usize },
    /// The commented line cannot be found once in the head.
    Ambiguous,
}

/// An unresolved thread.
#[derive(Debug, Clone)]
pub struct InboxThread {
    pub root: ReviewComment,
    pub author: Option<String>,
    pub replies: usize,
    pub last: Option<(Option<String>, String)>,
    pub anchor: Anchor,
}

fn status_name(status: ChangeStatus) -> &'static str {
    match status {
        ChangeStatus::Added => "added",
        ChangeStatus::Deleted => "removed",
        ChangeStatus::Modified => "modified",
        ChangeStatus::Renamed => "renamed",
        ChangeStatus::Copied => "copied",
    }
}

/// `text` cut to [`EXCERPT_CHARS`] characters, with `…` when cut.
#[must_use]
pub fn excerpt(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= EXCERPT_CHARS { text.to_owned() } else { format!("{}…", text.chars().take(EXCERPT_CHARS - 1).collect::<String>()) }
}

/// Where line `line` (1-based) of `old` is in `new`: once exactly, or not at all.
fn map_line(old: &str, line: u32, new: &str) -> Anchor {
    let Some(wanted) = old.lines().nth(line.saturating_sub(1) as usize) else { return Anchor::Ambiguous };
    if wanted.trim().is_empty() {
        return Anchor::Ambiguous;
    }
    let mut found = new.lines().enumerate().filter(|(_, candidate)| *candidate == wanted);
    match (found.next(), found.next()) {
        (Some((index, _)), None) => Anchor::Moved { line: index + 1 },
        _ => Anchor::Ambiguous,
    }
}

impl PullService {
    /// The diff manifest of `ctx`: every changed file with its blob ids, no patch text.
    pub async fn manifest(&self, ctx: &PullContext) -> Result<Manifest, ServiceError> {
        let (base, head) = self.compared_ids(ctx).await?;
        let comparison = self.changes(ctx, DiffLimits::default()).await?;
        let olds: Vec<String> = comparison.files.iter().map(|file| file.old_path.clone().unwrap_or_else(|| file.path.clone())).collect();
        let news: Vec<String> = comparison.files.iter().map(|file| file.path.clone()).collect();
        let ids = ctx.repo().reader.path_ids_at(vec![(base.clone(), olds), (head.clone(), news)]).await?;
        let (old_ids, new_ids) = (ids.first().cloned().flatten().unwrap_or_default(), ids.get(1).cloned().flatten().unwrap_or_default());
        let files = comparison
            .files
            .iter()
            .enumerate()
            .map(|(index, file)| ManifestFile {
                path: file.path.clone(),
                old_path: file.old_path.clone(),
                status: status_name(file.status),
                additions: file.additions,
                deletions: file.deletions,
                stats_known: file.stats_known,
                binary: matches!(file.body, FileDiffBody::Binary),
                old_blob: old_ids.get(index).cloned().flatten(),
                new_blob: new_ids.get(index).cloned().flatten(),
            })
            .collect();
        Ok(Manifest { base, head, files })
    }

    /// The hunks of `path` in `ctx`'s diff; `None` when the file did not change.
    pub async fn file_hunks(&self, ctx: &PullContext, path: &str) -> Result<Option<Vec<Hunk>>, ServiceError> {
        let comparison = self.changes(ctx, DiffLimits::default()).await?;
        Ok(comparison.files.into_iter().find(|file| file.path == path).map(|file| match file.body {
            FileDiffBody::Text(hunks) => hunks,
            _ => Vec::new(),
        }))
    }

    /// The unresolved threads of `ctx` the viewer can see, oldest first, at most 50.
    pub async fn inbox(&self, viewer: Option<&rbit_core::User>, ctx: &PullContext) -> Result<Vec<InboxThread>, ServiceError> {
        let data = self.reviews(viewer, ctx).await?;
        let comments: Vec<(ReviewComment, Option<String>)> = data
            .reviews
            .iter()
            .flat_map(|entry| entry.comments.iter().map(|(comment, author)| (comment.clone(), author.as_ref().map(|account| account.name.to_string()))))
            .collect();
        let mut threads = Vec::new();
        for (root, author) in comments.iter().filter(|(comment, _)| comment.in_reply_to.is_none() && comment.resolved_at.is_none()).take(INBOX_THREADS) {
            let replies: Vec<&(ReviewComment, Option<String>)> = comments.iter().filter(|(comment, _)| comment.in_reply_to == Some(root.id)).collect();
            let last = replies.last().map(|(comment, author)| (author.clone(), excerpt(&comment.body)));
            let anchor = if root.outdated { self.map_anchor(ctx, root).await } else { Anchor::Current };
            threads.push(InboxThread { root: root.clone(), author: author.clone(), replies: replies.len(), last, anchor });
        }
        Ok(threads)
    }

    /// Where outdated `root` points in the head now.
    async fn map_anchor(&self, ctx: &PullContext, root: &ReviewComment) -> Anchor {
        if root.side != ReviewSide::Right {
            return Anchor::Ambiguous;
        }
        let reader = &ctx.repo().reader;
        let old = reader.blob(root.commit_sha.clone(), root.path.clone(), ANCHOR_BLOB_BYTES).await;
        let new = reader.blob(ctx.pull.head_sha.clone(), root.path.clone(), ANCHOR_BLOB_BYTES).await;
        match (old.ok().and_then(|blob| blob.data), new.ok().and_then(|blob| blob.data)) {
            (Some(old), Some(new)) => map_line(&String::from_utf8_lossy(&old), root.line, &String::from_utf8_lossy(&new)),
            _ => Anchor::Ambiguous,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outdated_anchor_moves_only_to_a_line_found_once() {
        let old = "a\nlet x = 1;\nb\n";
        assert_eq!(map_line(old, 2, "new\na\nlet x = 1;\nb\n"), Anchor::Moved { line: 3 });
        assert_eq!(map_line(old, 2, "a\nb\n"), Anchor::Ambiguous, "the line is gone");
        assert_eq!(map_line(old, 2, "let x = 1;\nlet x = 1;\n"), Anchor::Ambiguous, "two lines match");
        assert_eq!(map_line(old, 9, "a\n"), Anchor::Ambiguous, "no such line");
        assert_eq!(excerpt(&"word ".repeat(200)).chars().count(), EXCERPT_CHARS);
    }
}
