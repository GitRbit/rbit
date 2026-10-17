//! Branch compare: `/{owner}/{repo}/compare` and `/compare/{base}...{head}`.
//!
//! Milestone 2 workstream R (repository essentials) wrote this file; wave B's pull request
//! workstream (P) owns it now and made comparisons work across a fork network: `head` may
//! be `branch`, `owner:branch` (that owner's repository in the network) or
//! `owner/name:branch`, read through `PullService::compare` (an object overlay, so nothing
//! is written to either repository).
//!
//! Merged into the page router, so its routes get sessions, CSRF protection, security
//! headers and error pages automatically.

use axum::{
    Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use rbit_core::pull::HeadSpec;
use rbit_git::{DiffLimits, RefKind};
use rbit_services::{RepoContext, ServiceError};
use serde::Deserialize;

use super::repo::RepoPage;
use crate::{
    AppState,
    error::{WebError, WebResult},
    extract::Viewer,
    urls::RepoUrls,
    views::{
        compare::{ComparePage, ComparePicker, CompareSummary},
        layout::Page,
        pulls::new_pull_url,
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
    /// `owner/name` of the base repository (another repository of the network).
    #[serde(default)]
    base_repo: String,
    /// `owner/name` of the head repository.
    #[serde(default)]
    head_repo: String,
}

/// The head spec naming `branch` of `head` as seen from `base`: `branch` in the same
/// repository, `owner:branch` when the names match, else `owner/name:branch`.
pub(super) fn head_spec(base: &RepoContext, head_full_name: &str, branch: &str) -> String {
    let base_name = base.repo.full_name();
    match head_full_name.split_once('/') {
        Some(_) if head_full_name.eq_ignore_ascii_case(&base_name) => branch.to_owned(),
        Some((owner, name)) if name.eq_ignore_ascii_case(base.repo.repo.name.as_str()) => format!("{owner}:{branch}"),
        Some(_) => format!("{head_full_name}:{branch}"),
        None => branch.to_owned(),
    }
}

/// The pickers for `loaded`, listing the readable fork network.
pub(super) async fn pickers(
    state: &AppState,
    viewer: &Viewer,
    loaded: &RepoPage,
    base: &str,
    head: &str,
    head_repo: &str,
) -> ComparePicker {
    let network = state.services.pulls.network(viewer.user(), &loaded.ctx).await.unwrap_or_default();
    let names: Vec<String> = network.iter().map(rbit_core::OwnedRepository::full_name).collect();
    ComparePicker::new(&loaded.urls, loaded.refs(), base, head).with_network(names, head_repo)
}

fn empty_page(loaded: &RepoPage, state: &AppState, page: Page, picker: ComparePicker) -> ComparePage {
    ComparePage {
        page: page.titled(format!("Compare · {}", loaded.ctx.repo.full_name())),
        header: loaded.header(state, "code"),
        picker,
        summary: None,
        files: Vec::new(),
        additions: 0,
        deletions: 0,
        truncated: false,
    }
}

/// `GET /{owner}/{repo}/compare[?base=…&head=…&base_repo=…&head_repo=…]`: the pickers, or a
/// redirect to the comparison once both sides are chosen.
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
        let own = loaded.ctx.repo.full_name();
        let base_repo = if query.base_repo.trim().is_empty() { own.clone() } else { query.base_repo.trim().to_owned() };
        let head_repo = if query.head_repo.trim().is_empty() { own.clone() } else { query.head_repo.trim().to_owned() };
        // The base may be another repository of the network: compare there.
        let base_ctx = if base_repo.eq_ignore_ascii_case(&own) {
            loaded.ctx.clone()
        } else {
            let (base_owner, base_name) = base_repo.split_once('/').ok_or(WebError::NotFound)?;
            state.services.repositories.open(viewer.user(), base_owner, base_name).await?
        };
        let spec = head_spec(&base_ctx, &head_repo, head);
        let urls = RepoUrls::new(base_ctx.repo.owner.name.as_str(), base_ctx.repo.repo.name.as_str());
        return Ok(Redirect::to(&urls.compare(base, &spec)).into_response());
    }
    // Suggest the most natural comparison: the first other branch against the default.
    let base = if base.is_empty() { loaded.default_branch().to_owned() } else { base.to_owned() };
    let head = if head.is_empty() {
        loaded.refs().iter().find(|r| r.kind == RefKind::Branch && r.name != base).map_or_else(|| base.clone(), |r| r.name.clone())
    } else {
        head.to_owned()
    };
    let own = loaded.ctx.repo.full_name();
    let picker = pickers(&state, &viewer, &loaded, &base, &head, &own).await;
    Ok(empty_page(&loaded, &state, page, picker).into_response())
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
    let head_spec_parsed = HeadSpec::parse(&head).ok_or(WebError::NotFound)?;
    let pulls = &state.services.pulls;
    let head_ctx = pulls.resolve_head(viewer.user(), &loaded.ctx, &head_spec_parsed).await?;
    let same = head_ctx.repo.repo.id == loaded.ctx.repo.repo.id;
    let comparison = pulls
        .compare(&loaded.ctx, &base, &head_ctx, &head_spec_parsed.branch, DiffLimits::default(), MAX_LISTED_COMMITS)
        .await
        .map_err(|error| match error {
            ServiceError::Internal(_) => WebError::from(error),
            _ => WebError::NotFound,
        })?;

    let head_full_name = head_ctx.repo.full_name();
    let picker = pickers(&state, &viewer, &loaded, &base, &head_spec_parsed.branch, &head_full_name).await;
    let mut view = empty_page(&loaded, &state, page, picker);
    view.page.title = Some(format!("Compare {base}...{head} · {}", loaded.ctx.repo.full_name()));
    let head_urls = RepoUrls::new(head_ctx.repo.owner.name.as_str(), head_ctx.repo.repo.name.as_str());
    let mut summary = CompareSummary::new(&comparison, &base, &head, &loaded.urls, &head_urls, same);
    if comparison.ahead > 0 && comparison.merge_base.is_some() {
        summary.create_url = Some(new_pull_url(loaded.ctx.repo.owner.name.as_str(), loaded.ctx.repo.repo.name.as_str(), &base, &head));
    }
    view.summary = Some(summary);
    Ok(view.with_comparison(&comparison, &head_urls))
}
