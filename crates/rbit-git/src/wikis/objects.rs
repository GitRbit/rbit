//! Native Git object preparation/read with bounded headers and a flat stable-ID tree.
use super::{PrepareWikiPage, WikiObjectError, layout};
use rbit_core::RepoId;
use rbit_wikis::{MAX_WIKI_CONTENT_BYTES, WikiContent, WikiId, WikiPageId, WikiRevisionId, WikiStoreId};
use std::path::Path;

const MAX_COMMIT_BYTES: u64 = 64 * 1024;
const MAX_TREE_BYTES: u64 = 1024 * 1024;
const MAX_PAGES: usize = 10_000;

fn id(value: WikiRevisionId) -> Result<gix::ObjectId, WikiObjectError> {
    gix::ObjectId::try_from(value.as_bytes()).map_err(|_| WikiObjectError::Format)
}

fn checked_header(repo: &gix::Repository, id: gix::ObjectId, kind: gix::objs::Kind, limit: u64) -> Result<(), WikiObjectError> {
    let header = repo.find_header(id).map_err(crate::GitError::backend)?;
    if header.kind() != kind || header.size() > limit {
        return Err(WikiObjectError::Format);
    }
    Ok(())
}

fn tree(repo: &gix::Repository, revision: WikiRevisionId) -> Result<gix::Tree<'_>, WikiObjectError> {
    let id = id(revision)?;
    checked_header(repo, id, gix::objs::Kind::Commit, MAX_COMMIT_BYTES)?;
    let commit = repo.find_commit(id).map_err(crate::GitError::backend)?;
    let tree = commit.tree_id().map_err(crate::GitError::backend)?.detach();
    checked_header(repo, tree, gix::objs::Kind::Tree, MAX_TREE_BYTES)?;
    repo.find_tree(tree).map_err(|error| crate::GitError::backend(error).into())
}

/// One bounded streaming pass: no second decoded-entry vector or formatted name per
/// row. Flat blob names have the same Git and ordinary byte ordering.
fn visit_tree<'a>(tree: &'a gix::Tree<'_>, mut visit: impl FnMut(gix::objs::tree::EntryRef<'a>)) -> Result<(), WikiObjectError> {
    let mut previous = None;
    for (index, entry) in gix::objs::TreeRefIter::from_bytes(&tree.data, tree.id.kind()).enumerate() {
        if index >= MAX_PAGES {
            return Err(WikiObjectError::Format);
        }
        let entry = entry.map_err(crate::GitError::backend)?;
        let bytes: &[u8] = entry.filename.as_ref();
        let name = std::str::from_utf8(bytes).map_err(|_| WikiObjectError::Format)?;
        let valid = name.strip_suffix(".md").is_some_and(|stem| {
            !stem.is_empty()
                && stem.len() <= 19
                && !stem.starts_with('0')
                && stem.bytes().all(|byte| byte.is_ascii_digit())
                && stem.parse::<i64>().ok().and_then(WikiPageId::new).is_some()
        });
        if !valid || entry.mode.kind() != gix::objs::tree::EntryKind::Blob || previous.is_some_and(|previous| previous >= entry) {
            return Err(WikiObjectError::Format);
        }
        previous = Some(entry);
        visit(entry);
    }
    Ok(())
}

pub(super) fn read(
    root: &Path,
    identity: WikiStoreId,
    repository: RepoId,
    wiki: WikiId,
    page: WikiPageId,
    revision: WikiRevisionId,
) -> Result<WikiContent, WikiObjectError> {
    let repo = layout::repository(root, identity, repository, wiki, false)?;
    let tree = tree(&repo, revision)?;
    let filename = format!("{}.md", page.get());
    let mut blob = None;
    visit_tree(&tree, |entry| {
        let name: &[u8] = entry.filename.as_ref();
        if name == filename.as_bytes() {
            blob = Some(entry.oid.to_owned());
        }
    })?;
    let blob = blob.ok_or(WikiObjectError::Format)?;
    checked_header(&repo, blob, gix::objs::Kind::Blob, u64::try_from(MAX_WIKI_CONTENT_BYTES).map_err(|_| WikiObjectError::Format)?)?;
    let blob = repo.find_blob(blob).map_err(crate::GitError::backend)?.take_data();
    let text = String::from_utf8(blob).map_err(|_| WikiObjectError::Format)?;
    WikiContent::try_from(text).map_err(|_| WikiObjectError::Format)
}

pub(super) fn prepare(root: &Path, identity: WikiStoreId, request: PrepareWikiPage) -> Result<WikiRevisionId, WikiObjectError> {
    if request.actor.0 <= 0 {
        return Err(WikiObjectError::Format);
    }
    let repo = layout::repository(root, identity, request.repository, request.wiki, true)?;
    let mut entries = if let Some(parent) = request.parent {
        let tree = tree(&repo, parent)?;
        let mut entries = Vec::with_capacity((tree.data.len() / 32).min(MAX_PAGES));
        visit_tree(&tree, |entry| entries.push(gix::objs::tree::Entry::from(entry)))?;
        entries
    } else {
        Vec::new()
    };
    let filename = format!("{}.md", request.page.get());
    let position = entries.binary_search_by(|entry| {
        let stored: &[u8] = entry.filename.as_ref();
        stored.cmp(filename.as_bytes())
    });
    if position.is_err() && entries.len() == MAX_PAGES {
        return Err(WikiObjectError::Format);
    }
    let blob = repo.write_blob(request.content.as_str()).map_err(crate::GitError::backend)?.detach();
    let entry = gix::objs::tree::Entry { mode: gix::objs::tree::EntryKind::Blob.into(), filename: filename.into(), oid: blob };
    match position {
        Ok(index) => entries[index] = entry,
        Err(index) => entries.insert(index, entry),
    }
    let tree = repo.write_object(gix::objs::Tree { entries }).map_err(crate::GitError::backend)?.detach();
    let signature = gix::actor::Signature {
        name: format!("account-{}", request.actor.0).into(),
        email: "wiki@localhost".into(),
        time: gix::date::Time { seconds: request.at.seconds(), offset: 0 },
    };
    let commit = gix::objs::Commit {
        tree,
        parents: request.parent.map(id).transpose()?.into_iter().collect(),
        author: signature.clone(),
        committer: signature,
        encoding: None,
        message: "Wiki page update\n".into(),
        extra_headers: Vec::new(),
    };
    let prepared = repo.write_object(commit).map_err(crate::GitError::backend)?;
    WikiRevisionId::try_from(prepared.detach().as_slice()).map_err(|_| WikiObjectError::Format)
}
