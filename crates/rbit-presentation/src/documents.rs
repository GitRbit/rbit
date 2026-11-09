//! The document tabs of a tree page: which conventional documents (README, licenses, code
//! of conduct, contributing, security, notice) a directory shows, in which order, under
//! which label, and how many.
//!
//! Tree reads stay with the caller: it lists the directory, and for the repository root
//! the [`FALLBACK_DIRECTORIES`] and [`LICENSE_DIRECTORY`] when [`directory`] finds them,
//! reading at most [`MAX_FALLBACK_ENTRIES`] of each; these functions decide everything else.

use std::cmp::Ordering;

/// Most documents a tree page offers.
pub const MAX_DOCUMENTS: usize = 32;
/// Most entries of a fallback or license directory considered.
pub const MAX_FALLBACK_ENTRIES: usize = 512;
/// Repository-level fallbacks, in precedence order; never inherited by subdirectories.
pub const FALLBACK_DIRECTORIES: [&str; 2] = [".github", "docs"];
/// The root directory whose every regular file is a license.
pub const LICENSE_DIRECTORY: &str = "LICENSES";

/// What a tree entry is, as far as documents are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryShape {
    /// A regular or executable file.
    File,
    Directory,
    /// A symbolic link, submodule or anything else: never a document.
    Other,
}

/// One entry of a listed directory.
#[derive(Debug, Clone, Copy)]
pub struct DocumentEntry<'a> {
    pub name: &'a str,
    pub shape: EntryShape,
}

/// The kinds of document, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentKind {
    Readme,
    License,
    Conduct,
    Contributing,
    Security,
    Notice,
}

/// A document a tree page can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentCandidate {
    /// Repository path.
    pub path: String,
    /// File name.
    pub name: String,
    pub kind: DocumentKind,
}

fn text_stem(name: &str) -> &str {
    for extension in [".markdown", ".md", ".txt", ".rst", ".adoc", ".org"] {
        if let Some(stem) = name.strip_suffix(extension) {
            return stem;
        }
    }
    name
}

/// The kind of document a file name denotes, if any.
pub fn kind(name: &str) -> Option<DocumentKind> {
    let lower = name.to_ascii_lowercase();
    let stem = text_stem(&lower);
    Some(match stem {
        "readme" => DocumentKind::Readme,
        "code_of_conduct" | "code-of-conduct" => DocumentKind::Conduct,
        "contributing" => DocumentKind::Contributing,
        "security" => DocumentKind::Security,
        "notice" => DocumentKind::Notice,
        _ if ["license", "licence", "copying", "unlicense"]
            .iter()
            .any(|prefix| stem == *prefix || stem.strip_prefix(*prefix).is_some_and(|suffix| suffix.starts_with(['-', '_', '.']))) =>
        {
            DocumentKind::License
        }
        _ => return None,
    })
}

fn join_path(directory: &str, name: &str) -> String {
    if directory.is_empty() { name.to_owned() } else { format!("{directory}/{name}") }
}

/// The documents among `entries` of `directory`; every regular file counts as a license
/// when `licenses`. Kept within [`MAX_DOCUMENTS`] while collecting.
pub fn candidates<'a>(entries: impl IntoIterator<Item = DocumentEntry<'a>>, directory: &str, licenses: bool) -> Vec<DocumentCandidate> {
    let mut found = Vec::new();
    for entry in entries.into_iter().filter(|entry| entry.shape == EntryShape::File) {
        let Some(kind) = (if licenses { Some(DocumentKind::License) } else { kind(entry.name) }) else { continue };
        found.push(DocumentCandidate { path: join_path(directory, entry.name), name: entry.name.to_owned(), kind });
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

/// Orders documents (kind, README variant, path ignoring case, path), drops repeated
/// paths and keeps the first [`MAX_DOCUMENTS`].
pub fn sort_and_bound(candidates: &mut Vec<DocumentCandidate>) {
    candidates.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| {
                if a.kind == DocumentKind::Readme { readme_priority(&a.name).cmp(&readme_priority(&b.name)) } else { Ordering::Equal }
            })
            .then_with(|| a.path.to_ascii_lowercase().cmp(&b.path.to_ascii_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    candidates.dedup_by(|a, b| a.path == b.path);
    candidates.truncate(MAX_DOCUMENTS);
}

/// The directory among `entries` called `name`, ignoring case, as it is spelled.
pub fn directory<'a>(entries: impl IntoIterator<Item = DocumentEntry<'a>>, name: &str) -> Option<&'a str> {
    entries.into_iter().find(|entry| entry.shape == EntryShape::Directory && entry.name.eq_ignore_ascii_case(name)).map(|entry| entry.name)
}

/// Adds a fallback directory's documents of kinds `found` does not have yet. Licenses are
/// never taken from a fallback; every root variant wins over a fallback category, and an
/// earlier fallback over a later one.
pub fn add_fallback(found: &mut Vec<DocumentCandidate>, fallback: Vec<DocumentCandidate>) {
    let missing: Vec<_> = fallback
        .into_iter()
        .filter(|candidate| candidate.kind != DocumentKind::License && !found.iter().any(|current| current.kind == candidate.kind))
        .collect();
    found.extend(missing);
}

fn license_name(candidate: &DocumentCandidate) -> String {
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

/// The tab label and icon of `candidate` among `all` the page's documents.
pub fn label(candidate: &DocumentCandidate, all: &[DocumentCandidate]) -> (String, &'static str) {
    let (name, icon) = match candidate.kind {
        DocumentKind::Readme => (
            if all.iter().filter(|item| item.kind == DocumentKind::Readme).count() > 1 {
                candidate.name.clone()
            } else {
                "README".to_owned()
            },
            "book",
        ),
        DocumentKind::License => (license_name(candidate), "file-text"),
        DocumentKind::Conduct => ("Code of conduct".to_owned(), "users"),
        DocumentKind::Contributing => ("Contributing".to_owned(), "users"),
        DocumentKind::Security => ("Security".to_owned(), "shield"),
        DocumentKind::Notice => ("Notice".to_owned(), "file-text"),
    };
    // Two generic licenses (e.g. LICENSE and COPYING) need distinct labels.
    let name = if candidate.kind == DocumentKind::License
        && name == "License"
        && all.iter().filter(|other| other.kind == DocumentKind::License && license_name(other) == "License").count() > 1
    {
        format!("License ({})", candidate.name)
    } else {
        name
    };
    (name, icon)
}

#[cfg(test)]
mod tests;
