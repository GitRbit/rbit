//! Branch compare: `/{owner}/{repo}/compare` and `/compare/{base}...{head}`.
//!
//! Milestone 2 workstream R (repository essentials) owns this file.
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically. Comparisons stay within one repository in
//! wave A; cross-fork comparison arrives with pull requests.

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_git::{DiffLimits, RefKind};
use rbit_services::ServiceError;
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        compare::{ComparePage, ComparePicker, CompareSummary},
        layout::Page,
    },
};

/// Commits listed under a comparison; the count above the list is always complete.
const MAX_LISTED_COMMITS: usize = 100;

pub fn routes() -> Router<AppState> {
    Router::new().route("/{owner}/{repo}/compare", get(picker)).route("/{owner}/{repo}/compare/{*spec}", get(compare))
}

#[derive(Debug, Deserialize)]
pub struct PickerQuery {
    #[serde(default)]
    base: String,
    #[serde(default)]
    head: String,
}

fn empty_page(state: &AppState, loaded: &RepoPage, page: Page, base: &str, head: &str) -> ComparePage {
    ComparePage {
        page: page.titled(format!("Compare · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "code"),
        picker: ComparePicker::new(&loaded.urls, loaded.refs(), base, head),
        summary: None,
        files: Vec::new(),
        additions: 0,
        deletions: 0,
        truncated: false,
    }
}

/// `GET /{owner}/{repo}/compare[?base=…&head=…]`: the pickers, or a redirect to the
/// comparison once both sides are chosen.
async fn picker(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<PickerQuery>,
) -> WebResult<Response> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (base, head) = (query.base.trim(), query.head.trim());
    if !base.is_empty() && !head.is_empty() {
        return Ok(Redirect::to(&loaded.urls.compare(base, head)).into_response());
    }
    // Suggest the most natural comparison: the first other branch against the default.
    let base = if base.is_empty() { loaded.default_branch().to_owned() } else { base.to_owned() };
    let head = if head.is_empty() {
        loaded.refs().iter().find(|r| r.kind == RefKind::Branch && r.name != base).map_or_else(|| base.clone(), |r| r.name.clone())
    } else {
        head.to_owned()
    };
    Ok(empty_page(&state, &loaded, page, &base, &head).into_response())
}

/// `GET /{owner}/{repo}/compare/{base}...{head}`; `/compare/{head}` compares against the
/// default branch. Either side may contain `/`; Git forbids `..` in ref names, so the
/// first `...` is the separator.
async fn compare(
    State(state): State<AppState>,
    viewer: Viewer,
    page: Page,
    Path((owner, repo, spec)): Path<(String, String, String)>,
) -> WebResult<ComparePage> {
    let loaded = RepoPage::load(&state, &viewer, &owner, &repo).await?;
    let (base, head) = match spec.split_once("...") {
        Some((base, head)) => (base.to_owned(), head.to_owned()),
        None => (loaded.default_branch().to_owned(), spec.clone()),
    };
    let comparison = loaded
        .ctx
        .reader
        .compare(base.clone(), head.clone(), DiffLimits::default(), MAX_LISTED_COMMITS)
        .await
        .map_err(ServiceError::from)?;

    let mut view = empty_page(&state, &loaded, page, &base, &head);
    view.page.title = Some(format!("Compare {base}...{head} · {}", loaded.ctx.repo.full_name()));
    view.summary = Some(CompareSummary::new(&comparison, &base, &head, &loaded.urls));
    Ok(view.with_comparison(&comparison, &loaded.urls))
}
