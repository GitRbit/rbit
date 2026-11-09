//! Discover a bounded set of conventional documents; render only the selected one.
//! Which documents, their order and labels are [`rbit_presentation::documents`]; this
//! module reads the trees and blobs and renders the selected document.
//! Every Git read and generated link uses the tree page's resolved commit snapshot.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_git::{EntryKind, GitError, RepositoryReader, TreeEntry};
use rbit_presentation::documents::{self, DocumentCandidate, DocumentEntry, EntryShape};
use rbit_services::ServiceError;
use std::fmt::Write;

use crate::{
    AppState,
    error::{WebError, WebResult},
    render::is_markdown,
    urls::{DocumentBase, RepoUrls},
    views::{
        pagination::ListQuery,
        repo::{ReadmeView, RepositoryDocument},
    },
};

/// The document view of a tree entry.
fn entry(tree_entry: &TreeEntry) -> DocumentEntry<'_> {
    let shape = match tree_entry.kind {
        EntryKind::File | EntryKind::Executable => EntryShape::File,
        EntryKind::Directory => EntryShape::Directory,
        EntryKind::Submodule | EntryKind::Symlink => EntryShape::Other,
    };
    DocumentEntry { name: &tree_entry.name, shape }
}

/// At most [`documents::MAX_FALLBACK_ENTRIES`] entries of a fallback directory.
fn bounded(entries: &[TreeEntry]) -> impl Iterator<Item = DocumentEntry<'_>> {
    entries[..entries.len().min(documents::MAX_FALLBACK_ENTRIES)].iter().map(entry)
}

async fn discover(reader: &RepositoryReader, commit: &str, path: &str, entries: &[TreeEntry]) -> Result<Vec<DocumentCandidate>, GitError> {
    let mut found = documents::candidates(entries.iter().map(entry), path, false);
    // Fallbacks are repository-level conventions, never inherited by subdirectories.
    if path.is_empty() {
        for fallback in documents::FALLBACK_DIRECTORIES {
            let Some(directory) = documents::directory(entries.iter().map(entry), fallback) else { continue };
            let view = reader.tree(commit.to_owned(), directory.to_owned()).await?;
            documents::add_fallback(&mut found, documents::candidates(bounded(&view.entries), directory, false));
        }
        if let Some(directory) = documents::directory(entries.iter().map(entry), documents::LICENSE_DIRECTORY) {
            let view = reader.tree(commit.to_owned(), directory.to_owned()).await?;
            found.extend(documents::candidates(bounded(&view.entries), directory, true));
        }
    }
    documents::sort_and_bound(&mut found);
    Ok(found)
}

fn document_url(urls: &RepoUrls, commit: &str, tree_path: &str, document_path: &str, query: &ListQuery) -> String {
    let encode = |value: &str| utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
    let mut url = format!("{}?document={}", urls.tree(commit, tree_path), encode(document_path));
    if !query.q.is_empty() {
        let _ = write!(url, "&q={}", encode(&query.q));
    }
    if query.page > 1 {
        let _ = write!(url, "&page={}", query.page);
    }
    url.push_str("#readme");
    url
}

pub(super) async fn load(
    reader: &RepositoryReader,
    urls: &RepoUrls,
    commit: &str,
    path: &str,
    entries: &[TreeEntry],
    query: &super::TreeQuery,
    state: &AppState,
) -> WebResult<(Vec<RepositoryDocument>, Option<ReadmeView>)> {
    let listing_query = query.listing();
    let candidates = discover(reader, commit, path, entries).await.map_err(ServiceError::from)?;
    let selected = match query.document.as_deref() {
        Some(path) => Some(candidates.iter().find(|candidate| candidate.path == path).ok_or(WebError::NotFound)?),
        None => candidates.first(),
    };
    let documents = candidates
        .iter()
        .map(|candidate| {
            let (name, icon) = documents::label(candidate, &candidates);
            RepositoryDocument {
                key: candidate.path.clone(),
                name,
                icon,
                url: document_url(urls, commit, path, &candidate.path, &listing_query),
                path: candidate.path.clone(),
                is_active: selected.is_some_and(|selected| selected.path == candidate.path),
            }
        })
        .collect();
    let readme = if let Some(selected) = selected {
        let max_bytes = state.services.repositories.max_render_bytes();
        let blob = reader.blob(commit.to_owned(), selected.path.clone(), max_bytes).await.map_err(ServiceError::from)?;
        let (html, markdown) = if let Some(text) = blob.text() {
            let markdown = is_markdown(&selected.name);
            let base = DocumentBase::new(urls.clone(), commit, super::browse::parent_path(&selected.path));
            (if markdown { state.documents.document(text.to_owned(), base).await? } else { super::escape(text) }, markdown)
        } else {
            (
                if blob.data.is_none() {
                    "This document is too large to display."
                } else {
                    "This document is binary or is not valid UTF-8 and cannot be displayed."
                }
                .to_owned(),
                false,
            )
        };
        Some(ReadmeView {
            name: selected.name.clone(),
            key: selected.path.clone(),
            path: selected.path.clone(),
            html,
            is_markdown: markdown,
        })
    } else {
        None
    };
    Ok((documents, readme))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_links_pin_the_snapshot_and_escape_queries() {
        let url = document_url(
            &RepoUrls::new("alice", "project"),
            "abc123",
            "docs",
            "docs/LICENSE-A&B #.md",
            &ListQuery { q: "a & b".into(), page: 2 },
        );
        assert_eq!(url, "/alice/project/tree/abc123/docs?document=docs%2FLICENSE%2DA%26B%20%23%2Emd&q=a%20%26%20b&page=2#readme");
    }
}
