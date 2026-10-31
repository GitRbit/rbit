//! Repository pages and their view models.

use std::sync::Arc;

use askama::Template;
use askama_web::WebTemplate;
use rbit_git::{CommitSummary, RefKind, RefSummary};

use crate::{
    render::HighlightedFile,
    urls::RepoUrls,
    views::{
        blocks::{Block, blocks, column_class},
        collaborators::CollaboratorAccess,
        components::Avatar,
        forks::ForkedFrom,
        insights::LanguageBar,
        layout::Page,
        releases::LatestRelease,
    },
};

// ------------------------------------------------------------------ shared pieces

/// Title block and tabs shown at the top of every repository page.
///
/// `Default` exists so tests can build one with `..RepoHeader::default()` and keep
/// compiling when fields are added.
#[derive(Debug, Clone, Default)]
#[expect(clippy::struct_excessive_bools, reason = "independent display flags")]
pub struct RepoHeader {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub owner_avatar: Avatar,
    /// The HTTPS (or HTTP) clone URL.
    pub clone_url: String,
    /// The SSH clone URL, when the SSH server is enabled.
    pub ssh_clone_url: Option<String>,
    pub home_url: String,
    pub commits_url: String,
    pub activity_url: String,
    pub branches_url: String,
    pub tags_url: String,
    pub releases_url: String,
    /// "Download ZIP" in the Clone menu: the landing branch as a `.zip`.
    pub zip_url: String,
    /// `/{owner}/{repo}/settings`, shown only when `can_admin`.
    pub settings_url: String,
    pub can_admin: bool,
    pub star_count: i64,
    pub starred_by_viewer: bool,
    /// Forks the viewer may read.
    pub fork_count: u64,
    /// The fork form (anonymous viewers are sent to sign in first).
    pub fork_url: String,
    pub forks_url: String,
    /// The parent, when this is a fork of a repository the viewer may read.
    pub forked_from: Option<ForkedFrom>,
    /// The upstream URL, when this repository is a pull mirror.
    pub mirror_url: Option<String>,
    /// Archived (read-only): `repo/subnav.html` shows a banner and pages hide their
    /// write controls.
    pub archived: bool,
    /// `/{owner}/{repo}/issues` and `/{owner}/{repo}/pulls`.
    pub issues_url: String,
    pub pulls_url: String,
    /// Open issues and pull requests, shown on the tabs (from
    /// `TimelineService::open_counts`, loaded with the page).
    pub open_issues: u64,
    pub open_pulls: u64,
    /// Repository-scoped feature policies already checked by the host adapter.
    /// A globally active plugin alone does not opt a repository into its UI.
    pub enabled_plugins: Vec<rbit_plugin_api::PluginId>,
    /// `"code"`, `"issues"`, `"pulls"`, `"commits"`, `"branches"`, `"tags"`, `"releases"`
    /// or `"activity"`, `"actions"`, `"settings"`; compared in `repo/subnav.html`.
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
    pub picker: super::ref_picker::RefPickerLinks,
    pub current: String,
    pub current_is_tag: bool,
    pub branch_count: usize,
    pub tag_count: usize,
}

impl RefSwitcher {
    /// Keep large ref collections out of every page; the menu loads a bounded page
    /// on demand and retains a normal link when scripting is unavailable.
    pub fn deferred(refs: &[RefSummary], current: &str, picker: super::ref_picker::RefPickerLinks) -> Self {
        let display = current.strip_prefix("refs/heads/").or_else(|| current.strip_prefix("refs/tags/")).unwrap_or(current);
        let branch_count = refs.iter().filter(|reference| reference.kind == RefKind::Branch).count();
        Self {
            current: shorten_revision(display),
            current_is_tag: current.starts_with("refs/tags/") || refs.iter().any(|r| r.kind == RefKind::Tag && r.name == current),
            picker,
            branch_count,
            tag_count: refs.len() - branch_count,
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
    /// The combined commit status mark (milestone 3, S); `None` without statuses.
    pub status: Option<crate::views::statuses::StatusMark>,
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
            status: None,
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
    /// Stable identity and repository-relative location of the selected document.
    pub key: String,
    pub path: String,
    /// Sanitised HTML (rendered Markdown, or escaped plain text).
    pub html: String,
    pub is_markdown: bool,
}

/// A conventional repository document available in the tree's tab strip/sidebar.
#[derive(Debug, Clone)]
pub struct RepositoryDocument {
    pub key: String,
    pub name: String,
    pub icon: &'static str,
    pub url: String,
    pub path: String,
    pub is_active: bool,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/tree.html")]
pub struct TreePage {
    pub pagination: super::pagination::Pagination,
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
    pub documents: Vec<RepositoryDocument>,
    pub document_scope: String,
    pub history_url: String,
    /// Language breakdown of the selected revision; `None` when empty or unavailable.
    pub languages: Option<LanguageBar>,
    pub language_files_url: String,
    /// The latest published release, for the sidebar.
    pub latest_release: Option<LatestRelease>,
    /// Set when the viewer is a collaborator without admin rights, to offer leaving.
    pub collaborator: Option<CollaboratorAccess>,
    // Milestone 3: editing files in the browser (E); set for writers on a branch.
    pub new_file_url: Option<String>,
    pub upload_url: Option<String>,
}

/// The same document markup as the tree page without page decorations or listings.
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/documents_fragment.html")]
pub struct RepositoryDocumentsFragment {
    pub document_scope: String,
    pub documents: Vec<RepositoryDocument>,
    pub readme: Option<ReadmeView>,
}

/// How a file's contents are presented.
#[derive(Debug, Clone)]
pub enum BlobContent {
    /// One HTML fragment per line (highlighted or escaped), shared with the highlighter's
    /// cache.
    Code(Arc<HighlightedFile>),
    /// Rendered, sanitised Markdown.
    Markdown(String),
    Binary,
    TooLarge,
    /// An image stored with Git LFS, shown from the raw URL.
    Image,
}

/// One line of the file viewer, as rendered in a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeLine<'a> {
    /// 1-based line number (also the `#L…` anchor).
    pub number: usize,
    /// Highlighted (or escaped) HTML of the line.
    pub html: &'a str,
}

/// Numbers `lines` (`lines[i]` is line `i + 1`) and splits them into virtual blocks
/// (`.vblock`, see [`blocks`](crate::views::blocks)): every line is one row.
pub fn code_blocks(lines: &[String]) -> Vec<Block<CodeLine<'_>>> {
    blocks(lines.iter().enumerate().map(|(index, html)| CodeLine { number: index + 1, html }))
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
    pub blame_url: String,
    pub raw_url: String,
    pub content: BlobContent,
    // Milestone 3: editing files in the browser (E); set for writers on a branch (the
    // editor only for text it can display).
    pub edit_url: Option<String>,
    pub delete_url: Option<String>,
    /// `Some` for a Git LFS pointer file: whether the server has its object (shown in
    /// place of the pointer when it does).
    pub lfs_stored: Option<bool>,
}

impl BlobPage {
    /// The code's lines in virtual blocks; none unless the content is [`BlobContent::Code`].
    /// Built here rather than by the handler so it keeps passing the highlighter's lines.
    pub fn code_blocks(&self) -> Vec<Block<CodeLine<'_>>> {
        match &self.content {
            BlobContent::Code(lines) => code_blocks(lines),
            _ => Vec::new(),
        }
    }

    /// The code's minimum width, from its widest line ([`column_class`]), measured once
    /// when the file was highlighted.
    pub fn code_columns(&self) -> String {
        let widest = match &self.content {
            BlobContent::Code(file) => file.widest(),
            _ => 0,
        };
        column_class(widest)
    }
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

    #[test]
    fn code_blocks_number_every_line_once() {
        let lines: Vec<String> = (1..=300).map(|n| format!("line {n}")).collect();
        let blocks = code_blocks(&lines);
        assert_eq!(blocks.iter().map(Block::len).collect::<Vec<_>>(), [128, 128, 44]);
        let flat: Vec<CodeLine<'_>> = blocks.iter().flat_map(|block| block.rows().to_vec()).collect();
        assert_eq!(flat.len(), 300);
        assert!(flat.iter().enumerate().all(|(index, line)| line.number == index + 1 && line.html == lines[index]));
    }

    fn blob_page(content: BlobContent) -> BlobPage {
        let urls = RepoUrls::new("o", "r");
        let commit = CommitSummary {
            id: "a".repeat(40),
            short_id: "a".repeat(7),
            summary: "First".into(),
            body: None,
            author_name: "Ada".into(),
            author_email: "ada@example.com".into(),
            authored_at: rbit_core::Timestamp(0),
            parent_ids: Vec::new(),
        };
        BlobPage {
            page: Page {
                locale: rbit_messages::LocaleSnapshot::english(0),
                ui: Default::default(),
                has_i18n: false,
                updates: rbit_services::updates::UpdateStatus::default(),
                site_name: "rbit".to_owned(),
                plugin_revision: 0,
                branding: rbit_core::config::BrandingConfig::default(),
                title: None,
                viewer: None,
                path: "/o/r/blob/main/a.rs".to_owned(),
                theme: crate::themes::Theme::System,
                has_unread_notifications: false,
            },
            header: RepoHeader::default(),
            switcher: RefSwitcher::deferred(
                &[],
                "main",
                crate::views::ref_picker::RefPickerLinks::new(&urls, &crate::views::ref_picker::RefPickerQuery::default()),
            ),
            breadcrumbs: Vec::new(),
            latest: CommitView::new(&commit, &urls),
            file_name: "a.rs".to_owned(),
            size: "1 KB".to_owned(),
            line_count: None,
            language: None,
            blame_url: "/o/r/blame/main/a.rs".to_owned(),
            raw_url: "/o/r/raw/main/a.rs".to_owned(),
            content,
            edit_url: None,
            delete_url: None,
            lfs_stored: None,
        }
    }

    #[test]
    fn blob_lines_render_in_blocks_with_their_ids() {
        let lines = Arc::new(HighlightedFile::new((1..=130).map(|n| format!("<b>line {n}</b>")).collect()));
        let page = blob_page(BlobContent::Code(Arc::clone(&lines)));
        let blocks = page.code_blocks();
        assert_eq!(blocks[0].rows()[0].html.as_ptr(), lines[0].as_ptr(), "rendered rows borrow the cached strings");
        let html = page.render().unwrap();
        assert!(!html.contains("style="), "no inline styles under the CSP");

        let blocks: Vec<&str> = html.split(r#"<div class="vblock" data-rows=""#).skip(1).collect();
        let rows: Vec<(usize, usize)> = blocks
            .iter()
            .map(|block| {
                let (rows, body) = block.split_once('"').unwrap();
                (rows.parse().unwrap(), body.matches(r#"class="code-line""#).count())
            })
            .collect();
        assert_eq!(rows, [(128, 128), (2, 2)], "data-rows is the number of lines in the block");
        assert!(html.contains(r#"<div class="code-lines cols-8">"#), "as wide as the widest line's text, tags aside");
        for n in 1..=130 {
            assert_eq!(html.matches(&format!(r#"id="L{n}""#)).count(), 1, "line {n} once, with its id");
        }
        assert!(html.contains(
            r#"<div class="code-line" id="L130"><span class="line-number" data-line="130"></span><span class="line-code"><b>line 130</b></span></div>"#
        ));
    }

    #[test]
    fn the_code_column_comes_from_the_cached_width_without_measuring_again() {
        // A stored width that the lines do not have: only a page that reads the cache
        // entry instead of scanning every line reports it.
        let file = HighlightedFile::with_widest(vec!["<b>short</b>".to_owned()], 100);
        assert_eq!(blob_page(BlobContent::Code(Arc::new(file))).code_columns(), "cols-104");
        let measured = HighlightedFile::new(vec!["<b>short</b>".to_owned(), "a&lt;b".to_owned()]);
        assert_eq!(measured.widest(), 5, "tags take no columns, an entity one");
        assert_eq!(blob_page(BlobContent::Binary).code_columns(), "cols-8");
    }

    #[test]
    fn only_code_has_blocks() {
        assert_eq!(blob_page(BlobContent::Binary).code_blocks(), Vec::new());
        let html = blob_page(BlobContent::Markdown("<p>Hi</p>".into())).render().unwrap();
        assert!(!html.contains("class=\"vblock"));
    }
}
