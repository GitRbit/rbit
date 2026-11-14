//! The standalone file finder and the commit count on a repository's file listing.

use askama::Template;
use askama_web::WebTemplate;

use super::{layout::Page, repo::RepoHeader};

/// The search box on the dedicated find page: a form that opens [`FindPage`] without
/// JavaScript; with it, the box filters every path of the commit in place.
#[derive(Debug, Clone)]
pub struct FileFinder {
    /// The find page for the shown revision (also the form's action).
    pub action: String,
    /// Newline-separated paths of the shown commit, for the in-page filter.
    pub paths_url: String,
    /// A file's page is this followed by its encoded path.
    pub blob_base: String,
    /// What the box already holds (the find page's query).
    pub query: String,
}

/// The commit count beside the latest commit, loaded after first paint.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/commit_count.html")]
pub struct CommitCountFragment {
    pub locale: rbit_localization::Locale,
    pub url: String,
    pub raw_count: usize,
    pub count: String,
    /// The walk stopped at its limit: the count is a lower bound.
    pub capped: bool,
    pub singular: bool,
}

/// One path that matches the find page's query.
#[derive(Debug, Clone)]
pub struct FoundFile {
    pub path: String,
    pub url: String,
}

/// `GET /{owner}/{repo}/find/{revision}`: every file of a revision, filtered by name.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/find.html")]
pub struct FindPage {
    pub page: Page,
    pub header: RepoHeader,
    pub revision: String,
    pub tree_url: String,
    pub finder: FileFinder,
    pub results: Vec<FoundFile>,
    /// Matches beyond the ones shown.
    pub more: usize,
    /// The revision has more files than the finder lists.
    pub truncated: bool,
}

/// How well `path` matches `query` for "Go to file", or `None` when it does not: the
/// query's characters must appear in order (case-insensitively, spaces ignored). Runs of
/// consecutive characters, word starts and matches inside the file name score higher;
/// long paths score lower. The match is tried over the whole path and over the file
/// name alone, keeping the better. `assets/file-finder.js` scores the same way.
pub fn file_match_score(path: &str, query: &str) -> Option<i64> {
    let needle: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Some(0);
    }
    let haystack: Vec<char> = path.chars().flat_map(char::to_lowercase).collect();
    let name_start = haystack.iter().rposition(|&c| c == '/').map_or(0, |slash| slash + 1);
    let whole = match_from(&haystack, &needle, 0, name_start)?;
    let best = match_from(&haystack, &needle, name_start, name_start).map_or(whole, |name| name.max(whole));
    Some(best * 16 - i64::try_from(haystack.len()).unwrap_or(i64::MAX / 2))
}

/// The greedy in-order match of `needle` in `haystack[start..]`, scored.
fn match_from(haystack: &[char], needle: &[char], start: usize, name_start: usize) -> Option<i64> {
    let mut score = 0;
    let mut next = start;
    let mut previous: Option<usize> = None;
    for &wanted in needle {
        let found = haystack.get(next..)?.iter().position(|&c| c == wanted)? + next;
        score += 1;
        if previous.is_some_and(|p| p + 1 == found) {
            score += 5;
        }
        if found == 0 || matches!(haystack[found - 1], '/' | '_' | '-' | '.' | ' ') {
            score += 4;
        }
        if found >= name_start {
            score += 2;
        }
        previous = Some(found);
        next = found + 1;
    }
    Some(score)
}

#[cfg(test)]
mod file_match_tests {
    use super::file_match_score;

    #[test]
    fn matches_in_order_and_prefers_file_names() {
        assert_eq!(file_match_score("src/main.rs", ""), Some(0));
        assert!(file_match_score("src/main.rs", "mainrs").is_some());
        assert!(file_match_score("src/main.rs", "MAIN .rs").is_some());
        assert!(file_match_score("src/main.rs", "rsmain").is_none());
        let name = file_match_score("crates/web/src/router.rs", "router").unwrap();
        let scattered = file_match_score("src/rust/outer.txt", "router").unwrap();
        assert!(name > scattered);
        let short = file_match_score("README.md", "readme").unwrap();
        let long = file_match_score("docs/archive/2019/README.md", "readme").unwrap();
        assert!(short > long);
    }
}
