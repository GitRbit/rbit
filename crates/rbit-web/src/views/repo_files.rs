//! File outlines for the language browser. Folder rows precede their descendants, and
//! an outline is built from one bounded page of paths at a time.

use crate::{
    urls::RepoUrls,
    views::{layout::Page, pagination::Pagination, repo::RepoHeader},
};
use askama::Template;
use askama_web::WebTemplate;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct FileRow {
    pub name: String,
    pub path: String,
    pub depth: usize,
    pub url: Option<String>,
}

pub fn file_rows(paths: &[String], urls: &RepoUrls, revision: &str) -> Vec<FileRow> {
    let mut outline = BTreeMap::new();
    for path in paths {
        for (end, _) in path.match_indices('/') {
            outline.entry(path[..end].to_owned()).or_insert(false);
        }
        outline.insert(path.clone(), true);
    }
    // Component sorting keeps each directory together even beside e.g. `src.rs`.
    let mut entries: Vec<_> = outline.into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.split('/').cmp(b.split('/')));
    entries
        .into_iter()
        .map(|(path, file)| FileRow {
            name: path.rsplit('/').next().unwrap_or(&path).to_owned(),
            depth: path.matches('/').count(),
            url: file.then(|| urls.blob(revision, &path)),
            path,
        })
        .collect()
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/language_files.html")]
pub struct LanguageFilesPage {
    pub page: Page,
    pub header: RepoHeader,
    pub language: String,
    pub rows: Vec<FileRow>,
    pub pagination: Pagination,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_groups_descendants_and_encodes_file_links() {
        let paths = vec!["src.rs".into(), "src/a #.rs".into(), "src/deep/lib.rs".into()];
        let rows = file_rows(&paths, &RepoUrls::new("acme", "rocket"), "abc123");
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            ["src", "src/a #.rs", "src/deep", "src/deep/lib.rs", "src.rs"]
        );
        assert!(rows[0].url.is_none());
        assert_eq!(rows[1].url.as_deref(), Some("/acme/rocket/blob/abc123/src/a%20%23.rs"));
        assert_eq!(rows[3].depth, 2);
    }
}
