//! A single commit with its diff, and the branch and tag listings.

use axum::extract::{Path, State};
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
    let detail = loaded.ctx.reader.commit_detail(revision, DiffLimits::default()).await.map_err(ServiceError::from)?;

    let title = format!("{} · {}", detail.commit.summary, loaded.ctx.repo.full_name());
    Ok(CommitPage::new(page.titled(title), loaded.header(&state, "commits"), &detail, &loaded.urls))
}

/// `GET /{owner}/{repo}/branches`
pub async fn branches(state: State<AppState>, viewer: Viewer, page: Page, path: Path<(String, String)>) -> WebResult<RefsPage> {
    refs(state, viewer, page, path, RefKind::Branch).await
}

/// `GET /{owner}/{repo}/tags`
pub async fn tags(state: State<AppState>, viewer: Viewer, page: Page, path: Path<(String, String)>) -> WebResult<RefsPage> {
    refs(state, viewer, page, path, RefKind::Tag).await
}

async fn refs(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    kind: RefKind,
) -> WebResult<RefsPage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let details = loaded.ctx.reader.ref_details(kind).await.map_err(ServiceError::from)?;

    let mut rows: Vec<RefRow> = details.iter().map(|d| RefRow::new(d, loaded.default_branch(), &loaded.urls)).collect();
    // The default branch leads the list; the rest stay most-recent-first.
    rows.sort_by_key(|row| !row.is_default);

    let (label, tab, kind_name) = match kind {
        RefKind::Branch => ("Branches", "branches", "branch"),
        RefKind::Tag => ("Tags", "tags", "tag"),
    };
    // Only branches carry the "Protected" badge.
    let protected = match kind {
        RefKind::Branch => state.services.protection.protected_branches(&loaded.ctx).await?,
        RefKind::Tag => Vec::new(),
    };
    Ok(RefsPage {
        page: page.titled(format!("{label} · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(&state, tab),
        kind: kind_name,
        rows,
    }
    .with_protected(&protected))
}
