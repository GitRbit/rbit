//! Discover a bounded set of conventional documents; render only the selected one.
//! Every Git read and generated link uses the tree page's resolved commit snapshot.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_git::{EntryKind, GitError, RepositoryReader, TreeEntry};
use rbit_services::ServiceError;
use std::fmt::Write;

use crate::{
    error::{WebError, WebResult},
    render::{is_markdown, render_markdown},
    urls::{DocumentBase, RepoUrls},
    views::{
        pagination::ListQuery,
        repo::{ReadmeView, RepositoryDocument},
    },
};

const MAX_DOCUMENTS: usize = 32;
const MAX_FALLBACK_ENTRIES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Readme,
    License,
    Conduct,
    Contributing,
    Security,
    Notice,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: String,
    name: String,
    kind: Kind,
}

fn regular(entry: &TreeEntry) -> bool {
    matches!(entry.kind, EntryKind::File | EntryKind::Executable)
}

fn text_stem(name: &str) -> &str {
    for extension in [".markdown", ".md", ".txt", ".rst", ".adoc", ".org"] {
        if let Some(stem) = name.strip_suffix(extension) {
            return stem;
        }
    }
    name
}

fn kind(name: &str) -> Option<Kind> {
    let lower = name.to_ascii_lowercase();
    let stem = text_stem(&lower);
    Some(match stem {
        "readme" => Kind::Readme,
        "code_of_conduct" | "code-of-conduct" => Kind::Conduct,
        "contributing" => Kind::Contributing,
        "security" => Kind::Security,
        "notice" => Kind::Notice,
        _ if ["license", "licence", "copying", "unlicense"]
            .iter()
            .any(|prefix| stem == *prefix || stem.strip_prefix(*prefix).is_some_and(|suffix| suffix.starts_with(['-', '_', '.']))) =>
        {
            Kind::License
        }
        _ => return None,
    })
}

fn candidates(entries: &[TreeEntry], directory: &str, licenses: bool) -> Vec<Candidate> {
    let mut found = Vec::new();
    for entry in entries.iter().filter(|entry| regular(entry)) {
        let Some(kind) = (if licenses { Some(Kind::License) } else { kind(&entry.name) }) else { continue };
        found.push(Candidate { path: super::join_path(directory, &entry.name), name: entry.name.clone(), kind });
        if found.len() > MAX_DOCUMENTS {
            sort_and_bound(&mut found);
        }
    }
    found
}

fn readme_priority(name: &str) -> usize {
    match name.to_ascii_lowercase().as_str() {
        "readme.md" => 0,
        "readme.markdown" => 1,
        "readme" => 2,
        "readme.txt" => 3,
        "readme.rst" => 4,
        _ => 5,
    }
}

fn sort_and_bound(candidates: &mut Vec<Candidate>) {
    candidates.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| {
                if a.kind == Kind::Readme { readme_priority(&a.name).cmp(&readme_priority(&b.name)) } else { std::cmp::Ordering::Equal }
            })
            .then_with(|| a.path.to_ascii_lowercase().cmp(&b.path.to_ascii_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates.dedup_by(|a, b| a.path == b.path);
    candidates.truncate(MAX_DOCUMENTS);
}

fn directory<'a>(entries: &'a [TreeEntry], name: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|entry| entry.kind == EntryKind::Directory && entry.name.eq_ignore_ascii_case(name))
        .map(|entry| entry.name.as_str())
}

async fn discover(reader: &RepositoryReader, commit: &str, path: &str, entries: &[TreeEntry]) -> Result<Vec<Candidate>, GitError> {
    let mut found = candidates(entries, path, false);
    // Fallbacks are repository-level conventions, never inherited by subdirectories.
    if path.is_empty() {
        for fallback in [".github", "docs"] {
            let Some(directory) = directory(entries, fallback) else { continue };
            let view = reader.tree(commit.to_owned(), directory.to_owned()).await?;
            let bounded = &view.entries[..view.entries.len().min(MAX_FALLBACK_ENTRIES)];
            let fallback = candidates(bounded, directory, false);
            // All root variants win over a fallback category; .github wins over docs.
            let missing: Vec<_> = fallback
                .into_iter()
                .filter(|candidate| candidate.kind != Kind::License && !found.iter().any(|current| current.kind == candidate.kind))
                .collect();
            found.extend(missing);
        }
        if let Some(directory) = directory(entries, "LICENSES") {
            let view = reader.tree(commit.to_owned(), directory.to_owned()).await?;
            let bounded = &view.entries[..view.entries.len().min(MAX_FALLBACK_ENTRIES)];
            found.extend(candidates(bounded, directory, true));
        }
    }
    sort_and_bound(&mut found);
    Ok(found)
}

fn license_name(candidate: &Candidate) -> String {
    let lower = candidate.name.to_ascii_lowercase();
    let stem = text_stem(&lower);
    let suffix = ["license", "licence", "copying"]
        .iter()
        .find_map(|prefix| stem.strip_prefix(*prefix))
        .unwrap_or(stem)
        .trim_matches(['-', '_', '.']);
    if suffix.is_empty() {
        return "License".to_owned();
    }
    match suffix {
        "mit" => "MIT license".to_owned(),
        "apache" => "Apache license".to_owned(),
        "apache-2.0" | "apache2" => "Apache 2.0 license".to_owned(),
        "unlicense" => "Unlicense".to_owned(),
        _ => format!("License ({})", candidate.name),
    }
}

fn label(candidate: &Candidate, all: &[Candidate]) -> (String, &'static str) {
    let (name, icon) = match candidate.kind {
        Kind::Readme => (
            if all.iter().filter(|item| item.kind == Kind::Readme).count() > 1 { candidate.name.clone() } else { "README".to_owned() },
            "book",
        ),
        Kind::License => (license_name(candidate), "file-text"),
        Kind::Conduct => ("Code of conduct".to_owned(), "users"),
        Kind::Contributing => ("Contributing".to_owned(), "users"),
        Kind::Security => ("Security".to_owned(), "shield"),
        Kind::Notice => ("Notice".to_owned(), "file-text"),
    };
    // Two generic licenses (e.g. LICENSE and COPYING) need distinct labels.
    let name = if candidate.kind == Kind::License
        && name == "License"
        && all.iter().filter(|other| other.kind == Kind::License && license_name(other) == "License").count() > 1
    {
        format!("License ({})", candidate.name)
    } else {
        name
    };
    (name, icon)
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
    max_bytes: usize,
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
            let (name, icon) = label(candidate, &candidates);
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
        let blob = reader.blob(commit.to_owned(), selected.path.clone(), max_bytes).await.map_err(ServiceError::from)?;
        let (html, markdown) = if let Some(text) = blob.text() {
            let markdown = is_markdown(&selected.name);
            let base = DocumentBase::new(urls.clone(), commit, super::parent_path(&selected.path));
            (if markdown { render_markdown(text, Some(&base)) } else { super::escape(text) }, markdown)
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

    fn file(name: &str) -> TreeEntry {
        TreeEntry { name: name.to_owned(), kind: EntryKind::File, object_id: "0".repeat(40) }
    }

    #[test]
    fn conventional_names_multiple_licenses_and_readme_priority() {
        let entries = [
            file("README.txt"),
            file("README.md"),
            file("LICENSE-MIT"),
            file("LICENSE-APACHE"),
            file("COPYING.LESSER"),
            file("CODE_OF_CONDUCT.md"),
            file("CONTRIBUTING.md"),
            file("SECURITY.md"),
            file("NOTICE"),
            file("readme.rs"),
            file("main.rs"),
        ];
        let mut found = candidates(&entries, "", false);
        sort_and_bound(&mut found);
        assert_eq!(found.len(), 9);
        assert_eq!(found[0].path, "README.md");
        assert_eq!(found[1].path, "README.txt");
        assert_eq!(found.iter().filter(|candidate| candidate.kind == Kind::License).count(), 3);
        let mit = found.iter().find(|candidate| candidate.path == "LICENSE-MIT").unwrap();
        assert_eq!(label(mit, &found), ("MIT license".into(), "file-text"));
    }

    #[test]
    fn only_regular_files_are_candidates_and_license_directory_is_bounded() {
        let mut symlink = file("README.md");
        symlink.kind = EntryKind::Symlink;
        let mut submodule = file("LICENSE");
        submodule.kind = EntryKind::Submodule;
        assert!(candidates(&[symlink, submodule], "", false).is_empty());
        let files: Vec<_> = (0..100).map(|index| file(&format!("MIT-{index}.txt"))).collect();
        let mut found = candidates(&files, "LICENSES", true);
        sort_and_bound(&mut found);
        assert_eq!(found.len(), MAX_DOCUMENTS);
        assert!(found.iter().all(|candidate| candidate.path.starts_with("LICENSES/")));
    }

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
