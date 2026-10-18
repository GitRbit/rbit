//! A single commit with its diff, and the branch and tag listings.

use crate::views::pagination::{LIST_PAGE_SIZE, ListQuery, Pagination};
use axum::extract::{Path, Query, State};
use rbit_git::{DiffLimits, RefKind};
use rbit_services::ServiceError;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        commit::CommitPage,
        layout::Page,
        refs::{RefRow, RefsPage},
    },
};

/// `GET /{owner}/{repo}/commit/{revision}`
pub async fn commit(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, revision)): Path<(String, String, String)>,
) -> WebResult<CommitPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    // The revision is resolved first, so the diff and the statuses (Milestone 3, S) can be
    // read at the same time, both for the commit it names. Even a full-length id is
    // resolved: it may be an annotated tag's object, or a branch named with hex digits.
    let id = loaded.ctx.reader.commit_id(revision).await.map_err(ServiceError::from)?;
    let (detail, combined) = tokio::join!(
        loaded.ctx.reader.commit_detail(id.clone(), DiffLimits::default()),
        state.services.statuses.combined_for(&loaded.ctx, &id),
    );
    // The diff's error wins: an unknown commit is 404 whatever the statuses say.
    let detail = detail.map_err(ServiceError::from)?;
    let combined = combined?;

    let title = format!("{} · {}", detail.commit.summary, loaded.ctx.repo.full_name());
    Ok(CommitPage::new(page.titled(title), loaded.header(&state, "commits"), &detail, &loaded.urls).with_statuses(&combined))
}

/// Whether `revision` has the shape of a full commit id (SHA-1 or SHA-256, lowercase hex).
/// Shape only: a branch or tag may have such a name (Git resolves it as a reference when
/// no object has that id), so whether a URL pins its answer is decided by comparing the
/// commit it resolved to with the URL's id.
pub(super) fn is_full_commit_id(revision: &str) -> bool {
    matches!(revision.len(), 40 | 64) && revision.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
pub struct RefsQuery {
    q: String,
    page: usize,
    view: Option<String>,
}

/// `GET /{owner}/{repo}/branches`
pub async fn branches(
    state: State<AppState>,
    viewer: Viewer,
    page: Page,
    path: Path<(String, String)>,
    Query(query): Query<RefsQuery>,
) -> WebResult<RefsPage> {
    refs(
        state,
        viewer,
        page,
        path,
        RefKind::Branch,
        ListQuery { q: query.q, page: query.page }.normalized(),
        query.view.as_deref() == Some("tree"),
    )
    .await
}

/// `GET /{owner}/{repo}/tags`
pub async fn tags(
    state: State<AppState>,
    viewer: Viewer,
    page: Page,
    path: Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> WebResult<RefsPage> {
    refs(state, viewer, page, path, RefKind::Tag, query.normalized(), false).await
}

async fn refs(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    kind: RefKind,
    query: ListQuery,
    show_graph: bool,
) -> WebResult<RefsPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    if show_graph {
        let references = loaded.ctx.reader.branch_tips().await.map_err(ServiceError::from)?;
        let mut commits = if references.is_empty() {
            Vec::new()
        } else {
            loaded
                .ctx
                .reader
                .branch_history(references.iter().map(|reference| reference.target.clone()).collect(), 201)
                .await
                .map_err(ServiceError::from)?
        };
        let truncated = commits.len() > 200;
        commits.truncate(200);
        return Ok(RefsPage {
            pagination: Pagination::new(references.len(), &query, loaded.urls.branches(), vec![]),
            page: page.titled(format!("Branches · {}", loaded.ctx.repo.full_name())),
            header: loaded.header(&state, "branches"),
            kind: "branch",
            rows: Vec::new(),
            graph: Some(crate::views::branch_graph::BranchGraph::new(&commits, &references, &loaded.urls, truncated)),
        });
    }
    let (details, total) = loaded
        .ctx
        .reader
        .ref_details_page(kind, query.q.clone(), loaded.default_branch().into(), query.page, LIST_PAGE_SIZE)
        .await
        .map_err(ServiceError::from)?;

    let mut rows: Vec<RefRow> = details.iter().map(|d| RefRow::new(d, loaded.default_branch(), &loaded.urls)).collect();
    // The default branch leads the list; the rest stay most-recent-first.
    rows.sort_by_key(|row| !row.is_default);

    let (label, tab, kind_name) = match kind {
        RefKind::Branch => ("Branches", "branches", "branch"),
        RefKind::Tag => ("Tags", "tags", "tag"),
    };
    // Only branches carry the "Protected" badge.
    let protected = match kind {
        RefKind::Branch => {
            state
                .services
                .protection
                .protected_branch_names(&loaded.ctx, &rows.iter().map(|row| row.name.clone()).collect::<Vec<_>>())
                .await?
        }
        RefKind::Tag => Vec::new(),
    };
    Ok(RefsPage {
        graph: None,
        pagination: Pagination::new(total, &query, format!("{}/{tab}", loaded.urls.home()), vec![]),
        page: page.titled(format!("{label} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, tab),
        kind: kind_name,
        rows,
    }
    .with_protected(&protected))
}
