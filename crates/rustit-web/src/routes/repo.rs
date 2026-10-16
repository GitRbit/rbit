//! Repository browsing: `/{owner}/{repo}/…` (code, files, commits).
//!
//! The file table paints immediately; each entry's last commit is filled in afterwards by
//! htmx from [`entry_commits`], whose response consists only of out-of-band swaps.

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
};
use rustit_git::{EntryKind, GitError, RefKind, RefSummary};
use rustit_services::{RepoContext, ServiceError, StarSummary};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    render::{format_size, is_markdown, render_markdown},
    urls::{DocumentBase, RepoUrls, encode_path},
    views::{
        components::Avatar,
        insights::{EntryCommit, EntryCommitCell, EntryCommitsFragment, LanguageBar, entry_commit_slot},
        layout::Page,
        repo::{
            BlobContent, BlobPage, CommitDay, CommitView, CommitsPage, EmptyRepoPage, EntryRow, ReadmeView, RefSwitcher, RepoHeader,
            TreePage, breadcrumbs,
        },
    },
};

const COMMITS_PER_PAGE: usize = 30;
/// Raw downloads are read fully into memory; refuse anything absurdly large.
const MAX_RAW_BYTES: usize = 100 * 1024 * 1024;

// ------------------------------------------------------------------ browsing helpers

/// Everything a repository page needs before rendering its specific content.
pub(super) struct RepoPage {
    pub(super) ctx: RepoContext,
    pub(super) urls: RepoUrls,
    refs: Vec<RefSummary>,
    stars: StarSummary,
}

impl RepoPage {
    pub(super) async fn load(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<Self> {
        let ctx = state.services.repositories.open(viewer.user(), owner, repo).await?;
        let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
        let refs = ctx.reader.refs().await.map_err(ServiceError::from)?;
        let stars = state.services.stars.summary(ctx.repo.repo.id, viewer.user()).await?;
        Ok(Self { ctx, urls, refs, stars })
    }

    pub(super) fn default_branch(&self) -> &str {
        &self.ctx.repo.repo.default_branch
    }

    /// The configured default branch if it exists, otherwise the first branch.
    fn landing_revision(&self) -> Option<String> {
        let branches = || self.refs.iter().filter(|r| r.kind == RefKind::Branch);
        branches().find(|r| r.name == self.default_branch()).or_else(|| branches().next()).map(|r| r.name.clone())
    }

    pub(super) fn header(&self, state: &AppState, active_tab: &'static str) -> RepoHeader {
        let repo = &self.ctx.repo;
        let landing = self.landing_revision().unwrap_or_else(|| self.default_branch().to_owned());
        RepoHeader {
            owner: repo.owner.name.to_string(),
            name: repo.repo.name.to_string(),
            description: repo.repo.description.clone(),
            is_private: !repo.repo.visibility.is_public(),
            owner_avatar: Avatar::for_account(&repo.owner),
            clone_url: format!("{}/{}.git", state.config.server.base_url(), repo.full_name()),
            ssh_clone_url: state.config.ssh.enabled.then(|| state.config.ssh.clone_url(&state.config.server, &repo.full_name())),
            home_url: self.urls.home(),
            commits_url: self.urls.commits(&landing),
            branches_url: self.urls.branches(),
            tags_url: self.urls.tags(),
            zip_url: self.urls.archive(&landing, ".zip"),
            settings_url: format!("{}/settings", self.urls.home()),
            can_admin: self.ctx.access.can_admin(),
            star_count: self.stars.count,
            starred_by_viewer: self.stars.starred_by_viewer,
            active_tab,
        }
    }

    pub(super) fn switcher(&self, current: &str, link: impl Fn(&str) -> String) -> RefSwitcher {
        RefSwitcher::new(&self.refs, current, self.default_branch(), link)
    }
}

fn join_path(directory: &str, name: &str) -> String {
    if directory.is_empty() { name.to_owned() } else { format!("{directory}/{name}") }
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

// ------------------------------------------------------------------ code tab

/// `GET /{owner}/{repo}`: the default branch's root, or setup instructions when empty.
pub async fn home(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let Some(revision) = loaded.landing_revision() else {
        let header = loaded.header(&state, "code");
        let page = page.titled(loaded.ctx.repo.full_name());
        return Ok(EmptyRepoPage { page, header, default_branch: loaded.default_branch().to_owned() }.into_response());
    };
    Ok(render_tree(&state, loaded, page, revision, String::new()).await?.into_response())
}

/// `GET /{owner}/{repo}/tree/{revision}/{path…}`
pub async fn tree(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<TreePage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = loaded.ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    render_tree(&state, loaded, page, revision, path).await
}

async fn render_tree(state: &AppState, loaded: RepoPage, page: Page, revision: String, path: String) -> WebResult<TreePage> {
    let reader = &loaded.ctx.reader;
    let (view, languages) = tokio::join!(reader.tree(revision.clone(), path.clone()), language_bar(state, &loaded));
    let view = view.map_err(ServiceError::from)?;
    let readme = reader
        .readme(revision.clone(), &view.entries, &path, state.services.repositories.max_render_bytes())
        .await
        .map_err(ServiceError::from)?
        .map(|(name, text)| {
            let is_markdown = is_markdown(&name);
            let base = DocumentBase::new(loaded.urls.clone(), revision.clone(), path.clone());
            let html = if is_markdown { render_markdown(&text, Some(&base)) } else { escape(&text) };
            ReadmeView { name, html, is_markdown }
        });

    let urls = &loaded.urls;
    let entries = view
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let entry_path = join_path(&path, &entry.name);
            EntryRow {
                url: match entry.kind {
                    EntryKind::Directory => urls.tree(&revision, &entry_path),
                    EntryKind::Submodule => urls.tree(&revision, &path),
                    _ => urls.blob(&revision, &entry_path),
                },
                name: entry.name.clone(),
                is_directory: entry.kind.is_directory(),
                is_submodule: entry.kind == EntryKind::Submodule,
                commit_slot: entry_commit_slot(index),
            }
        })
        .collect();

    let title = if path.is_empty() { loaded.ctx.repo.full_name() } else { format!("{path} · {}", loaded.ctx.repo.full_name()) };
    Ok(TreePage {
        page: page.titled(title),
        header: loaded.header(state, "code"),
        switcher: loaded.switcher(&revision, |r| urls.tree(r, "")),
        breadcrumbs: breadcrumbs(loaded.ctx.repo.repo.name.as_str(), urls, &revision, &path),
        parent_url: (!path.is_empty()).then(|| urls.tree(&revision, parent_path(&path))),
        latest: CommitView::new(&view.commit, urls),
        entries,
        // Pinned to the resolved commit so the cells always describe the rows on screen.
        entry_commits_url: entry_commits_url(urls, &view.commit.id, &path),
        readme,
        history_url: urls.commits(&revision),
        languages,
    })
}

/// The sidebar language bar for the default branch. Insights are decoration: a failure
/// is logged and the section omitted rather than failing the page.
async fn language_bar(state: &AppState, loaded: &RepoPage) -> Option<LanguageBar> {
    let revision = loaded.landing_revision()?;
    match state.services.insights.languages(&loaded.ctx, &revision).await {
        Ok(breakdown) => LanguageBar::new(&breakdown),
        Err(error) => {
            tracing::warn!(%error, repo = %loaded.ctx.repo.full_name(), "language statistics unavailable");
            None
        }
    }
}

fn entry_commits_url(urls: &RepoUrls, commit_id: &str, path: &str) -> String {
    let mut url = format!("{}/tree-commits/{commit_id}", urls.home());
    if !path.is_empty() {
        url.push('/');
        url.push_str(&encode_path(path));
    }
    url
}

/// `GET /{owner}/{repo}/tree-commits/{revision}/{path…}`: the last commit of every entry
/// in a directory, as out-of-band swaps into the rows of the tree page.
pub async fn entry_commits(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<EntryCommitsFragment> {
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let (revision, path) = ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    let view = ctx.reader.tree(revision.clone(), path.clone()).await.map_err(ServiceError::from)?;
    let commits = state.services.insights.last_commits(&ctx, &revision, &path, &view.entries).await?;

    let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
    let cells = view
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| EntryCommitCell {
            slot_id: entry_commit_slot(index),
            commit: commits.get(&entry.name).map(|c| EntryCommit::new(c, &urls)),
        })
        .collect();
    Ok(EntryCommitsFragment { cells })
}

/// `GET /{owner}/{repo}/blob/{revision}/{path…}`. A directory path redirects to its
/// tree page, so relative links in documents may name directories without a trailing `/`.
pub async fn blob(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = loaded.ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    if path.is_empty() {
        return Err(WebError::NotFound);
    }
    let max_bytes = state.services.repositories.max_render_bytes();
    let view = match loaded.ctx.reader.blob(revision.clone(), path.clone(), max_bytes).await {
        Ok(view) => view,
        Err(GitError::NotFound(_)) if loaded.ctx.reader.tree(revision.clone(), path.clone()).await.is_ok() => {
            return Ok(Redirect::to(&loaded.urls.tree(&revision, &path)).into_response());
        }
        Err(error) => return Err(ServiceError::from(error).into()),
    };

    let name = file_name(&path).to_owned();
    let text = view.text().map(str::to_owned);
    let line_count = text.as_ref().map(|t| t.lines().count());
    let language = text.as_ref().and_then(|t| state.highlighter.language_name(&name, t));

    let content = match text {
        _ if view.data.is_none() => BlobContent::TooLarge,
        None => BlobContent::Binary,
        Some(text) if is_markdown(&name) => {
            let base = DocumentBase::new(loaded.urls.clone(), revision.clone(), parent_path(&path));
            BlobContent::Markdown(render_markdown(&text, Some(&base)))
        }
        Some(text) => {
            // Highlighting is CPU-bound; keep it off the async workers.
            let highlighter_state = state.clone();
            let highlight_name = name.clone();
            BlobContent::Code(
                tokio::task::spawn_blocking(move || highlighter_state.highlighter.highlight_lines(&highlight_name, &text))
                    .await
                    .map_err(WebError::internal)?,
            )
        }
    };

    let urls = &loaded.urls;
    Ok(BlobPage {
        page: page.titled(format!("{path} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, "code"),
        switcher: loaded.switcher(&revision, |r| urls.tree(r, "")),
        breadcrumbs: breadcrumbs(loaded.ctx.repo.repo.name.as_str(), urls, &revision, &path),
        latest: CommitView::new(&view.commit, urls),
        file_name: name,
        size: format_size(view.size),
        line_count,
        language,
        blame_url: urls.blame(&revision, &path),
        raw_url: urls.raw(&revision, &path),
        content,
    }
    .into_response())
}

/// `GET /{owner}/{repo}/raw/{revision}/{path…}`: the file's bytes.
///
/// Always served as `text/plain` or `application/octet-stream` with a sandboxing CSP, so
/// a committed HTML or SVG file can never execute script in this origin.
pub async fn raw(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let ctx = state.services.repositories.open(viewer.user(), &owner, &repo).await?;
    let (revision, path) = ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    let view = ctx.reader.blob(revision, path, MAX_RAW_BYTES).await.map_err(ServiceError::from)?;
    let Some(data) = view.data else {
        return Err(WebError::BadRequest("file is too large to download through the web interface; clone the repository instead".into()));
    };
    let content_type = if view.is_binary { "application/octet-stream" } else { "text/plain; charset=utf-8" };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_SECURITY_POLICY, "sandbox; default-src 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        data,
    )
        .into_response())
}

// ------------------------------------------------------------------ commits tab

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    page: Option<usize>,
}

/// `GET /{owner}/{repo}/commits` — history of the default branch.
pub async fn commits_default(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<PageQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let Some(revision) = loaded.landing_revision() else {
        return Ok(Redirect::to(&loaded.urls.home()).into_response());
    };
    Ok(render_commits(&state, loaded, page, revision, query.page.unwrap_or(1)).await?.into_response())
}

/// `GET /{owner}/{repo}/commits/{revision}`
pub async fn commits(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Query(query): Query<PageQuery>,
) -> WebResult<CommitsPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, _) = loaded.ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    render_commits(&state, loaded, page, revision, query.page.unwrap_or(1)).await
}

async fn render_commits(state: &AppState, loaded: RepoPage, page: Page, revision: String, page_number: usize) -> WebResult<CommitsPage> {
    let page_number = page_number.max(1);
    let skip = (page_number - 1) * COMMITS_PER_PAGE;
    // Fetch one extra to learn whether an older page exists.
    let mut history = loaded.ctx.reader.history(revision.clone(), skip, COMMITS_PER_PAGE + 1).await.map_err(ServiceError::from)?;
    let has_older = history.len() > COMMITS_PER_PAGE;
    history.truncate(COMMITS_PER_PAGE);

    let urls = &loaded.urls;
    let base = urls.commits(&revision);
    let commits = history.iter().map(|c| CommitView::new(c, urls)).collect();

    Ok(CommitsPage {
        page: page.titled(format!("Commits · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "commits"),
        switcher: loaded.switcher(&revision, |r| urls.commits(r)),
        days: CommitDay::group(commits),
        newer_url: (page_number > 1).then(|| format!("{base}?page={}", page_number - 1)),
        older_url: has_older.then(|| format!("{base}?page={}", page_number + 1)),
    })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
