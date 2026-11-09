//! Repository browsing: `/{owner}/{repo}/…` (code, files, commits).
//!
//! The file table paints immediately; each entry's last commit is filled in afterwards by
//! htmx from [`entry_commits`], whose response consists only of out-of-band swaps.

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Redirect, Response},
};
use rbit_core::mirror::Mirror;
use rbit_git::{EntryKind, GitError, RefKind, RefSummary};
use rbit_localization::{RepoBrowseMessage, RepoNavLabel};
use rbit_services::{RepoContext, ServiceError, StarSummary, timeline::OpenCounts};
use serde::Deserialize;

use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    render::{format_size, is_markdown},
    urls::{DocumentBase, RepoUrls, encode_path},
    views::{
        collaborators::CollaboratorAccess,
        components::Avatar,
        forks::ForkedFrom,
        insights::{EntryCommit, EntryCommitCell, EntryCommitsFragment, LanguageBar, entry_commit_slot},
        layout::Page,
        pagination::{ListQuery, Pagination},
        ref_picker::{RefPickerLinks, RefPickerQuery, RefTarget},
        releases::LatestRelease,
        repo::{
            BlobContent, BlobPage, CommitDay, CommitView, CommitsPage, EmptyRepoPage, EntryRow, RefSwitcher, RepoHeader,
            RepositoryDocumentsFragment, TreePage, breadcrumbs,
        },
        repo_text,
        statuses::{CHECKS_ANCHOR, StatusMark},
    },
};

use rbit_services::repositories::browse::{self, COMMITS_PER_PAGE, MAX_RAW_BYTES};

#[path = "repository_documents.rs"]
mod repository_documents;

/// Kept flat because `serde_urlencoded`'s flattened maps lose the string-to-number conversion used for numeric pagination fields.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TreeQuery {
    q: String,
    page: usize,
    document: Option<String>,
    document_fragment: bool,
}

impl TreeQuery {
    fn listing(&self) -> ListQuery {
        ListQuery { q: self.q.clone(), page: self.page }.normalized()
    }
}

#[derive(Debug, Deserialize)]
pub struct ActivityQuery {
    year: Option<i32>,
    calendar: Option<String>,
}

/// Default-branch contribution calendar, including a useful empty-repository state.
pub async fn activity(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ActivityQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let calendar = crate::views::contributions::ContributionCalendar::repository(query.year, page.locale);
    let (start, end) = calendar.bounds();
    let contributions = if query.calendar.is_some() {
        calendar.populate(&state.services.contributions.repository(&loaded.ctx, start, end).await?)
    } else {
        calendar.deferred()
    };
    if query.calendar.as_deref() == Some("fragment") {
        return Ok(crate::views::contributions::ContributionFragment { contributions }.into_response());
    }
    Ok(crate::views::contributions::ActivityPage {
        page: repo_text::titled(page, RepoNavLabel::Activity, &loaded.ctx.repo.full_name()),
        header: loaded.header(&state, "activity"),
        contributions,
    }
    .into_response())
}
/// Raw downloads are read fully into memory; refuse anything absurdly large.
mod files;
mod navigation;

pub use files::{commit_count, find};

// ------------------------------------------------------------------ browsing helpers

/// Everything a repository page needs before rendering its specific content.
pub(super) struct RepoPage {
    pub(super) ctx: RepoContext,
    pub(super) urls: RepoUrls,
    refs: Vec<RefSummary>,
    stars: StarSummary,
    fork_count: u64,
    forked_from: Option<ForkedFrom>,
    mirror: Option<Mirror>,
    open_counts: OpenCounts,
    // Actions: interface and secrets (U).
    actions_on: bool,
    wiki_on: bool,
}

impl RepoPage {
    /// The repository, access-checked, with its header's data from the same statement
    /// ([`rbit_services::RepositoryService::open_page`]), and its refs.
    pub(super) async fn load(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<Self> {
        let (mut ctx, header) = state.services.repositories.open_page(viewer.user(), owner, repo).await?;
        ctx.reader = ctx.reader.pinned();
        let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
        // Actions: interface and secrets (U). Only an instance with Actions asks the
        // repository whether it has them on, beside reading the refs.
        let actions_on = async {
            match &state.services.actions {
                Some(actions) => actions.enabled_for(ctx.repo.repo.id).await,
                None => Ok(false),
            }
        };
        let (refs, actions_on) = tokio::join!(ctx.reader.refs(), actions_on);
        let refs = refs.map_err(ServiceError::from)?;
        let forked_from = header.parent.map(|parent| ForkedFrom { url: format!("/{}", parent.full_name()), full_name: parent.full_name() });
        Ok(Self {
            ctx,
            urls,
            refs,
            stars: header.stars,
            fork_count: header.fork_count,
            forked_from,
            mirror: header.mirror,
            open_counts: header.open_counts,
            actions_on: actions_on?,
            wiki_on: header.wiki_enabled,
        })
    }

    /// The repository's pull mirror settings, when it is one.
    pub(super) fn mirror(&self) -> Option<&Mirror> {
        self.mirror.as_ref()
    }

    /// Branches then tags, each sorted by name.
    pub(super) fn refs(&self) -> &[RefSummary] {
        &self.refs
    }

    /// Splits `"<revision>/<path>"` from a URL. Nearly every link names a branch or tag,
    /// which the refs loaded for the page resolve without touching Git; anything else
    /// (commit ids) goes through the reader's longest-prefix search.
    pub(super) async fn split(&self, rest: String) -> WebResult<(String, String)> {
        if let Some(split) = split_known_ref(&self.refs, &rest) {
            return Ok(split);
        }
        Ok(self.ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?)
    }

    pub(super) fn default_branch(&self) -> &str {
        &self.ctx.repo.repo.default_branch
    }

    /// The configured default branch if it exists, otherwise the first branch.
    fn landing_revision(&self) -> Option<String> {
        let branches = self.refs.iter().filter(|r| r.kind == RefKind::Branch).map(|r| r.name.as_str());
        browse::landing_revision(branches, self.default_branch()).map(str::to_owned)
    }

    /// Whether the viewer may edit files on `revision` in the browser (Milestone 3, E):
    /// write access, not archived, not a mirror, and `revision` is a branch.
    pub(super) fn can_edit_on(&self, revision: &str) -> bool {
        self.ctx.can_write_content() && self.mirror.is_none() && self.refs.iter().any(|r| r.kind == RefKind::Branch && r.name == revision)
    }

    pub(super) fn switcher(&self, current: &str, target: RefTarget, path: &str) -> RefSwitcher {
        let query = RefPickerQuery { target, path: path.into(), current: current.into(), ..Default::default() };
        RefSwitcher::deferred(&self.refs, current, RefPickerLinks::new(&self.urls, &query))
    }
}

/// Opens `owner/repo` for `viewer` with a [pinned](rbit_git::RepositoryReader::pinned)
/// reader: the handful of Git reads one page makes share a single opened repository, which is released when the request ends.
pub(super) async fn open_pinned(state: &AppState, viewer: &Viewer, owner: &str, repo: &str) -> WebResult<RepoContext> {
    let mut ctx = state.services.repositories.open(viewer.user(), owner, repo).await?;
    ctx.reader = ctx.reader.pinned();
    Ok(ctx)
}

/// The longest known branch or tag that `combined` starts with, and the path after it.
fn split_known_ref(refs: &[RefSummary], combined: &str) -> Option<(String, String)> {
    browse::split_known_ref(refs.iter().map(|r| r.name.as_str()), combined)
}

#[derive(Deserialize)]
pub struct LanguageFilesQuery {
    revision: String,
    language: String,
    #[serde(default)]
    q: String,
    #[serde(default)]
    page: usize,
}

/// A paged full page without JavaScript, pinned to the commit used for classification;
/// the language dialog extracts its file outline.
pub async fn language_files(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<LanguageFilesQuery>,
) -> WebResult<crate::views::repo_files::LanguageFilesPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let revision = loaded.ctx.reader.commit_id(query.revision).await.map_err(ServiceError::from)?;
    let paths = loaded.ctx.reader.language_files(revision.clone(), query.language.clone()).await.map_err(ServiceError::from)?;
    let search = ListQuery { q: query.q, page: query.page }.normalized();
    let needle = search.q.to_lowercase();
    let paths: Vec<_> = paths.into_iter().filter(|path| needle.is_empty() || path.to_lowercase().contains(&needle)).collect();
    let pagination = Pagination::new(
        paths.len(),
        &search,
        format!("{}/language-files", loaded.urls.home()),
        vec![("revision".into(), revision.clone()), ("language".into(), query.language.clone())],
    );
    // The outline covers only this page's paths, so its size stays bounded.
    let rows = crate::views::repo_files::file_rows(&paths[pagination.start..pagination.end], &loaded.urls, &revision);
    Ok(crate::views::repo_files::LanguageFilesPage {
        page: repo_text::titled(page, RepoBrowseMessage::LanguageFiles { language: &query.language }, &loaded.ctx.repo.full_name()),
        header: loaded.header_at(&state, "code", &revision),
        language: query.language,
        rows,
        pagination,
    })
}

// ------------------------------------------------------------------ code tab

/// `GET /{owner}/{repo}`: the default branch's root, or setup instructions when empty.
pub async fn home(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<TreeQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    if loaded
        .mirror()
        .is_some_and(|mirror| mirror.last_sync_at.is_none() || (mirror.last_error.is_some() && loaded.landing_revision().is_none()))
    {
        return Ok(Redirect::to(&format!("{}/mirror/progress", loaded.urls.home())).into_response());
    }
    let Some(revision) = loaded.landing_revision() else {
        let header = loaded.header(&state, "code");
        let page = page.titled(loaded.ctx.repo.full_name());
        return Ok(EmptyRepoPage { page, header, default_branch: loaded.default_branch().to_owned() }.into_response());
    };
    Ok(render_tree(&state, &viewer, loaded, page, revision, String::new(), query).await?.into_response())
}

/// `GET /{owner}/{repo}/tree/{revision}/{path…}`
pub async fn tree(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Query(query): Query<TreeQuery>,
) -> WebResult<Response> {
    if query.document_fragment {
        // Tab switches need only the selected bounded document. In particular they
        // must not calculate repository language statistics or page decorations.
        let ctx = open_pinned(&state, &viewer, &owner, &repo).await?;
        let (revision, path) = ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
        let view = ctx.reader.tree(revision, path.clone()).await.map_err(ServiceError::from)?;
        let urls = RepoUrls::new(&owner, &repo);
        let (documents, readme) =
            repository_documents::load(&ctx.reader, &urls, &view.commit.id, &path, &view.entries, &query, &state).await?;
        return Ok(RepositoryDocumentsFragment { page, document_scope: urls.home(), documents, readme }.into_response());
    }
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = loaded.split(rest).await?;
    Ok(render_tree(&state, &viewer, loaded, page, revision, path, query).await?.into_response())
}

async fn render_tree(
    state: &AppState,
    viewer: &Viewer,
    loaded: RepoPage,
    page: Page,
    revision: String,
    path: String,
    query: TreeQuery,
) -> WebResult<TreePage> {
    let reader = &loaded.ctx.reader;
    let commit_id = reader.commit_id(revision.clone()).await.map_err(ServiceError::from)?;
    let listing_query = query.listing();
    // Pin the listing and language statistics to the same revision snapshot.
    let listing = reader.tree(commit_id.clone(), path.clone());
    let (listing, languages, latest_release) =
        tokio::join!(listing, language_bar(state, &loaded, &commit_id), latest_release(state, &loaded));
    let collaborator = collaborator_access(viewer, &loaded);
    let view = listing.map_err(ServiceError::from)?;
    let (documents, readme) =
        repository_documents::load(reader, &loaded.urls, &view.commit.id, &path, &view.entries, &query, state).await?;

    let urls = &loaded.urls;
    let filtered = browse::matching_entries(&view.entries, |entry| entry.name.as_str(), &listing_query.q);
    let document_query = readme.as_ref().map(|document| vec![("document".to_owned(), document.path.clone())]).unwrap_or_default();
    let pagination = Pagination::new(filtered.len(), &listing_query, urls.tree(&view.commit.id, &path), document_query);
    let cells = Pagination::new(filtered.len(), &listing_query, entry_commits_url(urls, &view.commit.id, &path), vec![]);
    let entries = filtered[pagination.start..pagination.end]
        .iter()
        .copied()
        .map(|(index, entry)| {
            let entry_path = browse::join_path(&path, &entry.name);
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
    let editable = loaded.can_edit_on(&revision);
    Ok(TreePage {
        page: page.titled(title),
        header: loaded.header_at(state, "code", &revision),
        switcher: loaded.switcher(&revision, RefTarget::Tree, ""),
        breadcrumbs: breadcrumbs(loaded.ctx.repo.repo.name.as_str(), urls, &revision, &path),
        parent_url: (!path.is_empty()).then(|| urls.tree(&revision, browse::parent_path(&path))),
        latest: CommitView::new(&view.commit, urls),
        entries,
        // Pinned to the resolved commit so the cells always describe the rows on screen.
        entry_commits_url: cells.url(cells.page),
        pagination,
        readme,
        documents,
        document_scope: urls.home(),
        history_url: urls.commits(&revision),
        commit_count_url: files::commit_count_url(urls, &view.commit.id),
        finder: files::finder(urls, &revision, &view.commit.id),
        languages,
        language_files_url: format!("{}/language-files?revision={}", urls.home(), crate::urls::encode_path(&view.commit.id)),
        latest_release,
        collaborator,
        new_file_url: editable.then(|| urls.edit("new", &revision, &path)),
        upload_url: editable.then(|| urls.edit("upload", &revision, &path)),
    })
}

/// The viewer's collaborator grant, when they have one and do not administer the
/// repository, so the sidebar can offer "Leave". It was read with the viewer's other
/// grants when the repository was opened for this request.
fn collaborator_access(viewer: &Viewer, loaded: &RepoPage) -> Option<CollaboratorAccess> {
    let user = viewer.user().filter(|_| !loaded.ctx.access.can_admin())?;
    debug_assert_eq!(loaded.ctx.viewer, Some(user.id()));
    let permission = loaded.ctx.collaborator?;
    Some(CollaboratorAccess::new(permission, &loaded.urls.home(), user.name().as_str()))
}

/// The sidebar language bar for the selected revision. Insights are decoration: a failure
/// is logged and the section omitted rather than failing the page.
async fn language_bar(state: &AppState, loaded: &RepoPage, revision: &str) -> Option<LanguageBar> {
    match state.services.insights.languages(&loaded.ctx, revision).await {
        Ok(breakdown) => LanguageBar::new(&breakdown),
        Err(error) => {
            tracing::warn!(%error, repo = %loaded.ctx.repo.full_name(), "language statistics unavailable");
            None
        }
    }
}

/// The sidebar's latest release. Like insights it is decoration: failures are logged.
async fn latest_release(state: &AppState, loaded: &RepoPage) -> Option<LatestRelease> {
    match state.services.releases.latest(&loaded.ctx).await {
        Ok(release) => release.map(|release| LatestRelease::new(&release, &loaded.urls)),
        Err(error) => {
            tracing::warn!(%error, repo = %loaded.ctx.repo.full_name(), "latest release unavailable");
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
    Query(query): Query<ListQuery>,
) -> WebResult<Response> {
    let ctx = open_pinned(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    let view = ctx.reader.tree(revision.clone(), path.clone()).await.map_err(ServiceError::from)?;
    let query = query.normalized();
    let filtered = browse::matching_entries(&view.entries, |entry| entry.name.as_str(), &query.q);
    let pagination = Pagination::new(filtered.len(), &query, String::new(), vec![]);
    let selected = &filtered[pagination.start..pagination.end];
    let entries: Vec<_> = selected.iter().map(|(_, entry)| (*entry).clone()).collect();
    let result = state.services.insights.last_commits_page(&ctx, &view, &path, &entries).await?;
    let commits = &result.commits;

    let urls = RepoUrls::new(ctx.repo.owner.name.as_str(), ctx.repo.repo.name.as_str());
    let cells = selected
        .iter()
        .copied()
        .map(|(index, entry)| EntryCommitCell {
            slot_id: entry_commit_slot(index),
            commit: commits.get(&entry.name).map(|c| EntryCommit::new(c, &urls)),
        })
        .collect();
    // The tree page links here by commit id, which pins the answer. Only when the revision
    // resolved to the commit it spells: a branch may be named with 40 hex digits.
    let pinned = view.commit.id == revision && ctx.repo.repo.visibility.is_public() && !result.timed_out && commits.len() == entries.len();
    Ok(crate::middleware::cache_pinned(EntryCommitsFragment { cells }.into_response(), pinned))
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
    let (revision, path) = loaded.split(rest).await?;
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
    // Milestone 3: a pointer file shows its Git LFS object instead (L).
    let (view, lfs) = super::lfs::lfs_blob(&state, &loaded.ctx, viewer.user(), view, max_bytes).await;
    let lfs_stored = lfs.map(|lfs| lfs.stored);

    let name = browse::file_name(&path).to_owned();
    let text = view.text().map(str::to_owned);
    let line_count = text.as_ref().map(|t| t.lines().count());
    let language = text.as_ref().and_then(|t| state.highlighter.language_name(&name, t));

    let content = match text {
        _ if lfs_stored == Some(true) && browse::image_content_type(&path).is_some() => BlobContent::Image,
        _ if view.data.is_none() => BlobContent::TooLarge,
        None => BlobContent::Binary,
        Some(text) if is_markdown(&name) => {
            let base = DocumentBase::new(loaded.urls.clone(), revision.clone(), browse::parent_path(&path));
            BlobContent::Markdown(state.documents.document(text, base).await?)
        }
        Some(text) => BlobContent::Code(state.highlighter.highlight_lines_cached(&view.id, &name, text).await?),
    };

    let urls = &loaded.urls;
    let editable = loaded.can_edit_on(&revision);
    // A Git LFS pointer is shown as its object, which the editor cannot change.
    let is_text = lfs_stored.is_none() && matches!(content, BlobContent::Code(_) | BlobContent::Markdown(_));
    Ok(BlobPage {
        size: format_size(view.size, page.locale),
        page: page.titled(format!("{path} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header_at(&state, "code", &revision),
        switcher: loaded.switcher(&revision, RefTarget::Tree, ""),
        breadcrumbs: breadcrumbs(loaded.ctx.repo.repo.name.as_str(), urls, &revision, &path),
        latest: CommitView::new(&view.commit, urls),
        file_name: name,
        line_count,
        language,
        blame_url: urls.blame(&revision, &path),
        raw_url: urls.raw(&revision, &path),
        content,
        edit_url: (editable && is_text).then(|| urls.edit("edit", &revision, &path)),
        delete_url: editable.then(|| urls.edit("delete", &revision, &path)),
        lfs_stored,
    }
    .into_response())
}

/// `GET /{owner}/{repo}/raw/{revision}/{path…}`: the file's bytes.
///
/// Served as `text/plain` or `application/octet-stream`, except common image types (so
/// README images display), always with a sandboxing CSP and `nosniff`: a committed HTML
/// file is never rendered, and an SVG opened directly cannot run script in this origin
/// (as an `<img>` it never can).
pub async fn raw(
    State(state): State<AppState>,
    viewer: Viewer,
    Path((owner, repo, rest)): Path<(String, String, String)>,
) -> WebResult<Response> {
    let ctx = open_pinned(&state, &viewer, &owner, &repo).await?;
    let (revision, path) = ctx.reader.split_revision_path(rest).await.map_err(ServiceError::from)?;
    let image_type = browse::image_content_type(&path);
    let view = ctx.reader.blob(revision, path, MAX_RAW_BYTES).await.map_err(ServiceError::from)?;
    // Milestone 3: a pointer file serves its Git LFS object, streamed (L).
    if let Some(response) = super::lfs::lfs_raw(&state, &ctx, viewer.user(), &view, image_type).await {
        return Ok(response);
    }
    let Some(data) = view.data else {
        return Err(WebError::BadRequest("file is too large to download through the web interface; clone the repository instead".into()));
    };
    let content_type = match image_type {
        Some(image) => image,
        None if view.is_binary => "application/octet-stream",
        None => "text/plain; charset=utf-8",
    };
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
    let revision = loaded.landing_revision().unwrap_or_else(|| loaded.default_branch().to_owned());
    Ok(render_commits(&state, loaded, page, revision, query).await?.into_response())
}

/// `GET /{owner}/{repo}/commits/{revision}`
pub async fn commits(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, rest)): Path<(String, String, String)>,
    Query(query): Query<PageQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (revision, _) = loaded.split(rest).await?;
    render_commits(&state, loaded, page, revision, query).await
}

async fn render_commits(state: &AppState, loaded: RepoPage, page: Page, revision: String, query: PageQuery) -> WebResult<Response> {
    let page_number = query.page.unwrap_or(1).max(1);
    let skip = commit_offset(page_number)?;
    // Fetch one extra to learn whether an older page exists.
    let mut history = if loaded.landing_revision().is_none() {
        Vec::new()
    } else {
        loaded.ctx.reader.history(revision.clone(), skip, COMMITS_PER_PAGE + 1).await.map_err(ServiceError::from)?
    };
    let has_older = history.len() > COMMITS_PER_PAGE;
    history.truncate(COMMITS_PER_PAGE);

    let urls = &loaded.urls;
    let base = urls.commits(&revision);
    let mut commits: Vec<CommitView> = history.iter().map(|c| CommitView::new(c, urls)).collect();
    // Milestone 3 (S): each commit's combined status mark, one query for the page.
    let ids: Vec<String> = history.iter().map(|commit| commit.id.clone()).collect();
    let combined = state.services.statuses.combined_of(&loaded.ctx, &ids).await?;
    for commit in &mut commits {
        if let Some(status) = combined.get(&commit.id) {
            commit.status = StatusMark::new(status, format!("{}#{CHECKS_ANCHOR}", commit.url));
        }
    }

    Ok(CommitsPage {
        page: repo_text::titled(page, RepoNavLabel::Commits, &loaded.ctx.repo.full_name()),
        header: loaded.header_at(state, "commits", &revision),
        switcher: loaded.switcher(&revision, RefTarget::Commits, ""),
        days: CommitDay::group(commits),
        newer_url: (page_number > 1).then(|| format!("{base}?page={}", page_number - 1)),
        older_url: has_older.then(|| format!("{base}?page={}", page_number + 1)),
    }
    .into_response())
}

/// Reject overflowing offsets instead of panicking or wrapping back to earlier commits.
fn commit_offset(page: usize) -> WebResult<usize> {
    browse::commit_offset(page).ok_or_else(|| WebError::BadRequest("page number is too large".into()))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use rbit_git::{RefKind, RefSummary};

    use super::{COMMITS_PER_PAGE, commit_offset, split_known_ref};

    #[test]
    fn commit_pagination_rejects_overflow() {
        assert_eq!(commit_offset(0).unwrap(), 0);
        assert_eq!(commit_offset(1).unwrap(), 0);
        assert_eq!(commit_offset(2).unwrap(), COMMITS_PER_PAGE);
        assert!(commit_offset(usize::MAX).is_err());
        assert!(commit_offset(usize::MAX / COMMITS_PER_PAGE + 2).is_err());
    }

    fn refs(names: &[&str]) -> Vec<RefSummary> {
        names.iter().map(|name| RefSummary { name: (*name).to_owned(), kind: RefKind::Branch, target: String::new() }).collect()
    }

    #[test]
    fn known_refs_split_at_the_longest_whole_segment() {
        let refs = refs(&["main", "feature", "feature/login", "v1.0"]);
        let split = |s: &str| split_known_ref(&refs, s);
        assert_eq!(split("main"), Some(("main".into(), String::new())));
        assert_eq!(split("main/src/lib.rs"), Some(("main".into(), "src/lib.rs".into())));
        assert_eq!(split("/feature/login/src/"), Some(("feature/login".into(), "src".into())));
        assert_eq!(split("feature/logout"), Some(("feature".into(), "logout".into())));
        assert_eq!(split("v1.0/README.md"), Some(("v1.0".into(), "README.md".into())));
        // Not a whole segment, or not a ref at all: left to the reader.
        assert_eq!(split("mainline/src"), None);
        assert_eq!(split("3f9a2c1/src"), None);
    }
}
