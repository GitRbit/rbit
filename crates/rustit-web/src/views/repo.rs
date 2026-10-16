//! Repository pages and their view models.

use askama::Template;
use askama_web::WebTemplate;
use rustit_git::{CommitSummary, RefKind, RefSummary};

use crate::{
    urls::RepoUrls,
    views::{
        components::Avatar,
        insights::LanguageBar,
        layout::Page,
    },
};

// ------------------------------------------------------------------ shared pieces

/// Title block and tabs shown at the top of every repository page.
#[derive(Debug, Clone)]
pub struct RepoHeader {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub owner_avatar: Avatar,
    pub clone_url: String,
    pub home_url: String,
    pub commits_url: String,
    pub branches_url: String,
    pub tags_url: String,
    /// `/{owner}/{repo}/settings`, shown only when `can_admin`.
    pub settings_url: String,
    pub can_admin: bool,
    pub star_count: i64,
    pub starred_by_viewer: bool,
    /// `"code"`, `"commits"`, `"branches"` or `"tags"`; compared in `repo/subnav.html`.
    pub active_tab: &'static str,
}

/// One branch or tag in the [`RefSwitcher`].
#[derive(Debug, Clone)]
pub struct RefLink {
    pub name: String,
    pub url: String,
    pub is_current: bool,
    pub is_default: bool,
}

/// The branch/tag dropdown.
#[derive(Debug, Clone)]
pub struct RefSwitcher {
    pub current: String,
    pub current_is_tag: bool,
    pub branches: Vec<RefLink>,
    pub tags: Vec<RefLink>,
}

impl RefSwitcher {
    /// Builds the dropdown; `link` maps a ref name to the URL it should open.
    pub fn new(refs: &[RefSummary], current: &str, default_branch: &str, link: impl Fn(&str) -> String) -> Self {
        let to_link = |r: &RefSummary| RefLink {
            name: r.name.clone(),
            url: link(&r.name),
            is_current: r.name == current,
            is_default: r.kind == RefKind::Branch && r.name == default_branch,
        };
        let (branches, tags): (Vec<_>, Vec<_>) = refs.iter().partition(|r| r.kind == RefKind::Branch);
        Self {
            current: shorten_revision(current),
            current_is_tag: tags.iter().any(|t| t.name == current),
            branches: branches.into_iter().map(to_link).collect(),
            tags: tags.into_iter().map(to_link).collect(),
        }
    }
}

/// Commit ids are shown abbreviated; names are shown as-is.
fn shorten_revision(revision: &str) -> String {
    let is_full_id = revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit());
    if is_full_id { revision[..7].to_owned() } else { revision.to_owned() }
}

/// A commit as shown in banners and history lists.
#[derive(Debug, Clone)]
pub struct CommitView {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub avatar: Avatar,
    pub when: String,
    pub when_iso: String,
    pub date: String,
    /// Browse the repository as of this commit.
    pub browse_url: String,
    /// The commit's own page (message and diff).
    pub url: String,
}

impl CommitView {
    pub fn new(commit: &CommitSummary, urls: &RepoUrls) -> Self {
        Self {
            id: commit.id.clone(),
            short_id: commit.short_id.clone(),
            summary: commit.summary.clone(),
            author: commit.author_name.clone(),
            avatar: Avatar::for_identity(&commit.author_name, &commit.author_email.to_lowercase()),
            when: commit.authored_at.relative(),
            when_iso: commit.authored_at.iso8601(),
            date: commit.authored_at.date(),
            browse_url: urls.tree(&commit.id, ""),
            url: urls.commit(&commit.id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Crumb {
    pub name: String,
    pub url: String,
}

/// `repo / src / main.rs` style breadcrumbs, each linking to its directory.
pub fn breadcrumbs(repo_name: &str, urls: &RepoUrls, revision: &str, path: &str) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb { name: repo_name.to_owned(), url: urls.tree(revision, "") }];
    let mut prefix = String::new();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(segment);
        crumbs.push(Crumb { name: segment.to_owned(), url: urls.tree(revision, &prefix) });
    }
    crumbs
}

// ------------------------------------------------------------------ pages

/// Shown until the first push.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/empty.html")]
pub struct EmptyRepoPage {
    pub page: Page,
    pub header: RepoHeader,
    pub default_branch: String,
}

#[derive(Debug, Clone)]
pub struct EntryRow {
    pub name: String,
    pub url: String,
    pub is_directory: bool,
    pub is_submodule: bool,
    /// Id of the element the lazily loaded last-commit cell replaces (see [`entry_commit_slot`](crate::views::insights::entry_commit_slot)).
    pub commit_slot: String,
}

#[derive(Debug, Clone)]
pub struct ReadmeView {
    pub name: String,
    /// Sanitised HTML (rendered Markdown, or escaped plain text).
    pub html: String,
    pub is_markdown: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/tree.html")]
pub struct TreePage {
    pub page: Page,
    pub header: RepoHeader,
    pub switcher: RefSwitcher,
    pub breadcrumbs: Vec<Crumb>,
    pub parent_url: Option<String>,
    pub latest: CommitView,
    pub entries: Vec<EntryRow>,
    /// Fragment with each entry's last commit, fetched by htmx after first paint.
    pub entry_commits_url: String,
    pub readme: Option<ReadmeView>,
    pub history_url: String,
    /// Language breakdown of the default branch; `None` when empty or unavailable.
    pub languages: Option<LanguageBar>,
}

/// How a file's contents are presented.
#[derive(Debug, Clone)]
pub enum BlobContent {
    /// One HTML fragment per line (highlighted or escaped).
    Code(Vec<String>),
    /// Rendered, sanitised Markdown.
    Markdown(String),
    Binary,
    TooLarge,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/blob.html")]
pub struct BlobPage {
    pub page: Page,
    pub header: RepoHeader,
    pub switcher: RefSwitcher,
    pub breadcrumbs: Vec<Crumb>,
    pub latest: CommitView,
    pub file_name: String,
    pub size: String,
    pub line_count: Option<usize>,
    pub language: Option<String>,
    pub raw_url: String,
    pub content: BlobContent,
}

/// Commits grouped under a calendar-day heading.
#[derive(Debug, Clone)]
pub struct CommitDay {
    pub date: String,
    pub commits: Vec<CommitView>,
}

impl CommitDay {
    /// Groups consecutive commits that share a date (input is newest-first).
    pub fn group(commits: Vec<CommitView>) -> Vec<Self> {
        let mut days: Vec<Self> = Vec::new();
        for commit in commits {
            match days.last_mut() {
                Some(day) if day.date == commit.date => day.commits.push(commit),
                _ => days.push(Self { date: commit.date.clone(), commits: vec![commit] }),
            }
        }
        days
    }
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/commits.html")]
pub struct CommitsPage {
    pub page: Page,
    pub header: RepoHeader,
    pub switcher: RefSwitcher,
    pub days: Vec<CommitDay>,
    pub newer_url: Option<String>,
    pub older_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_link_each_directory() {
        let urls = RepoUrls::new("alice", "proj");
        let crumbs = breadcrumbs("proj", &urls, "main", "src/bin/cli.rs");
        let names: Vec<_> = crumbs.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["proj", "src", "bin", "cli.rs"]);
        assert_eq!(crumbs[2].url, "/alice/proj/tree/main/src/bin");
    }

    #[test]
    fn full_commit_ids_are_shortened() {
        assert_eq!(shorten_revision("0123456789abcdef0123456789abcdef01234567"), "0123456");
        assert_eq!(shorten_revision("main"), "main");
    }
}
