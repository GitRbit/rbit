//! `/` and `/explore`.

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
};
use rbit_core::RepoSort;
use rbit_services::repositories::RepoSearch;
use serde::Deserialize;

use crate::{
    AppState,
    error::WebResult,
    extract::Viewer,
    views::{
        components::{OrgLink, Pagination},
        home::{DashboardPage, ExplorePage, LandingPage, SortLink, explore_url, sort_label},
        layout::Page,
    },
};

const RECENT_ON_HOME: u32 = 8;

/// Landing page for visitors, dashboard for signed-in users.
pub async fn index(State(state): State<AppState>, viewer: Viewer, page: Page) -> WebResult<Response> {
    let repositories = &state.services.repositories;
    let recent = super::listing::repo_cards(&state, &repositories.recent_public(RECENT_ON_HOME).await?).await?;

    let Some(user) = viewer.user() else {
        let counts = state.services.instance.public_counts().await?;
        return Ok(LandingPage { page, recent, counts }.into_response());
    };
    let repos = super::listing::repo_cards(&state, &repositories.list_for_dashboard(user).await?).await?;
    let orgs = state.services.orgs.memberships(user).await?.iter().map(OrgLink::from).collect();
    let can_create_org = state.services.orgs.can_create(user);
    Ok(DashboardPage { page: page.titled("Dashboard"), repos, recent, orgs, can_create_org }.into_response())
}

/// `?q=…&sort=…&page=…`. Everything is optional and parsed leniently: a bad value falls
/// back to its default instead of failing the request.
#[derive(Debug, Deserialize)]
pub struct ExploreQuery {
    q: Option<String>,
    sort: Option<String>,
    page: Option<String>,
}

pub async fn explore(State(state): State<AppState>, page: Page, Query(params): Query<ExploreQuery>) -> WebResult<ExplorePage> {
    let query = params.q.unwrap_or_default().trim().to_owned();
    let sort = params.sort.as_deref().and_then(RepoSort::parse).unwrap_or_default();
    let requested = params.page.as_deref().and_then(|p| p.parse().ok()).unwrap_or(1);

    let search = RepoSearch { text: query.clone(), sort, page: requested };
    let results = state.services.repositories.explore(&search).await?;

    let pagination = Pagination {
        page: results.page,
        page_count: results.page_count(),
        previous_url: results.has_previous().then(|| explore_url(&query, sort, results.page - 1)),
        next_url: results.has_next().then(|| explore_url(&query, sort, results.page + 1)),
    };
    let sorts = RepoSort::ALL
        .iter()
        .map(|&option| SortLink { label: sort_label(option), url: explore_url(&query, option, 1), is_active: option == sort })
        .collect();
    let title = if query.is_empty() { "Explore".to_owned() } else { format!("{query} · Explore") };

    Ok(ExplorePage {
        page: page.titled(title),
        repos: super::listing::repo_cards(&state, &results.repos).await?,
        query,
        sort: sort.as_str(),
        sorts,
        total: results.total,
        pagination,
    })
}
