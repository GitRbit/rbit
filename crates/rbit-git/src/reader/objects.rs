//! Resolve and describe commit objects for repository readers.

use gix::{ObjectId, bstr::ByteSlice};
use rbit_core::Timestamp;

use crate::{CommitSummary, GitError};

/// Resolves a branch, tag, or (abbreviated) commit id to a commit id.
pub(crate) fn resolve_commit(repo: &gix::Repository, revision: &str) -> Result<ObjectId, GitError> {
    // Restrict to plain names and hex ids: full rev-parse syntax (`@{-1}`, `^{/regex}`)
    // is unnecessary for URLs and expands the attack surface.
    if revision.is_empty() || revision.contains("..") || revision.contains(['@', '^', '~', ':', '{']) {
        return Err(GitError::NotFound("revision"));
    }
    // The full id of an existing object names that object, as in `git rev-parse`.
    // Looking it up directly skips gix's abbreviated-id lookup, which re-reads the object
    // database's directory state (pack indexes, alternates, `realpath`) on every call:
    // pages pass the commit they resolved once to each later read, so that was several
    // file system scans a page. A name that only looks like an id (a branch named with
    // 40 hex digits) is no object and is resolved below.
    if revision.len() == repo.object_hash().len_in_hex()
        && let Ok(id) = ObjectId::from_hex(revision.as_bytes())
        && let Ok(object) = repo.find_object(id)
    {
        return Ok(object.peel_to_commit().map_err(|_| GitError::NotFound("commit"))?.id);
    }
    let id = repo.rev_parse_single(revision).map_err(|_| GitError::NotFound("revision"))?;
    let object = id.object().map_err(GitError::backend)?.peel_to_commit().map_err(|_| GitError::NotFound("commit"))?;
    Ok(object.id)
}

pub(crate) fn summarize(commit: &gix::Commit<'_>) -> Result<CommitSummary, GitError> {
    let message = commit.message().map_err(GitError::backend)?;
    let author = commit.author().map_err(GitError::backend)?.trim();
    let body = message.body.map(|b| b.to_str_lossy().trim().to_owned()).filter(|b| !b.is_empty());

    Ok(CommitSummary {
        id: commit.id.to_hex().to_string(),
        short_id: commit.id.to_hex_with_len(7).to_string(),
        summary: message.summary().to_str_lossy().into_owned(),
        body,
        author_name: author.name.to_str_lossy().into_owned(),
        author_email: author.email.to_str_lossy().into_owned(),
        authored_at: Timestamp(author.seconds()),
        parent_ids: commit.parent_ids().map(|id| id.to_hex().to_string()).collect(),
    })
}
